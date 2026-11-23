"""Capture a deterministic CPU scene trace from the pinned Java source."""

import argparse
import json
import os
from pathlib import Path
import struct
import subprocess
import tempfile

import oracle


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", nargs="?", type=Path, help="clean pinned checkout; defaults to the prepared export")
    args = parser.parse_args()
    try:
        capture(args.source)
    except (oracle.OracleError, OSError, subprocess.SubprocessError) as exc:
        parser.exit(1, f"check-cpu-scene: {exc}\n")


def compare_padded_rasters(reference, candidate, name, layout):
    width, height = layout[0] + 4, layout[1] + 4
    bins = width * height
    header = struct.pack(">III", *layout)
    if any(len(data) != len(header) + 16 * bins or not data.startswith(header)
           for data in (reference, candidate)):
        raise oracle.OracleError(f"invalid {name} padded raster dimensions, quality or length")
    if candidate != reference:
        for offset in range(len(header), len(reference), 16):
            if reference[offset:offset + 16] != candidate[offset:offset + 16]:
                y, x = divmod((offset - len(header)) // 16, width)
                java = struct.unpack_from(">Ifff", reference, offset)
                rust = struct.unpack_from(">Ifff", candidate, offset)
                raise oracle.OracleError(f"padded raster mismatch at ({x}, {y}): Java {java}; Rust {rust}")
    hits = sum(count for count, *_ in struct.iter_unpack(">Ifff", memoryview(reference)[12:]))
    budget = bins * layout[2]
    if not 0 < hits <= budget:
        raise oracle.OracleError(f"invalid accumulated hit count: {hits} for {budget} samples")
    print(f"{name}: {bins:,} padded bins match bit-for-bit ({hits:,} hits, {budget:,} sample budget)", flush=True)


def capture(source):
    info = oracle.resolve_source(source)
    export = oracle.CACHE / info["export"]
    jdk = oracle.resolve_toolchain()
    artifacts = oracle.ROOT / "artifacts"
    artifacts.mkdir(exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="cpu-scene.", dir=artifacts))
    classes = work / "classes"
    home = work / "home"
    classes.mkdir()
    home.mkdir()
    print(f"check-cpu-scene: artifacts={work}", flush=True)
    probe = oracle.ROOT / "oracle/SceneV1Probe.java"
    image_probe = oracle.ROOT / "oracle/ImageV1Probe.java"
    raster_probe = oracle.ROOT / "oracle/PaddedRasterProbe.java"
    cp = oracle.classpath(export)
    subprocess.run([
        jdk["javac"], "-encoding", "ISO-8859-1", "-proc:none", "-cp", cp,
        "-sourcepath", str(export / "src"), "-d", str(classes), str(probe), str(image_probe), str(raster_probe),
    ], cwd=oracle.ROOT, check=True, timeout=600)
    runtime_cp = os.pathsep.join((str(classes), str(export / "src"), str(export / "resources"), cp))
    provenance = {
        "upstream_commit": info["upstream_commit"],
        "source_tree": info["source_tree"],
        "source_digest_sha256": info["source_digest_sha256"],
        "java_version": jdk["java_version"],
        "javac_version": jdk["javac_version"],
        "probe_sha256": oracle.sha256_file(probe),
        "seed": 741103597,
        "scenes": {},
    }
    for name in ("TINA0010", "TINA0004", "TINA0006", "TINA0008", "TINA0011", "TINA0021", "TINA0009", "TINA0018", "TINA0013", "TINA0016", "TINA0003"):
        scene = export / "resources/flames" / (name + ".flame")
        trace = work / (name + ".txt")
        with (work / (name + ".stdout")).open("wb") as stdout, (work / (name + ".stderr")).open("wb") as stderr:
            subprocess.run([
                jdk["java"], "-Djava.awt.headless=true", f"-Duser.home={home}",
                "-cp", runtime_cp, "SceneV1Probe", str(scene), str(trace),
            ], cwd=oracle.ROOT, stdout=stdout, stderr=stderr, check=True, timeout=120)
        provenance["scenes"][name] = {"input_sha256": oracle.sha256_file(scene), "trace_sha256": oracle.sha256_file(trace)}
    subprocess.run([
        "cargo", "run", "--locked", "-p", "rwildfire-cpu", "--example", "check_java_scene",
        "--", str(export), str(work),
    ], cwd=oracle.ROOT, check=True, timeout=600)

    example = ["cargo", "run", "--release", "--locked", "-p", "rwildfire-cpu",
               "--example", "check_java_image", "--"]
    java = [jdk["java"], "-Djava.awt.headless=true", f"-Duser.home={home}", "-cp", runtime_cp]
    provenance["image"] = {"probe_sha256": oracle.sha256_file(image_probe),
                           "raster_probe_sha256": oracle.sha256_file(raster_probe)}
    for name, layout, mode in (
        ("TINA0010", (1014, 668, 100), "unfiltered"),
        ("TINA0010", (1014, 668, 100), "filtered"),
        ("TINA0011", (932, 662, 3), "filtered"),
        ("TINA0021", (932, 662, 100), "filtered"),
        ("TINA0009", (932, 662, 3), "filtered"),
        ("TINA0018", (932, 662, 3), "filtered"),
        ("TINA0013", (932, 662, 100), "filtered"),
        ("TINA0016", (932, 662, 3), "filtered"),
        ("TINA0003", (932, 662, 100), "filtered"),
        ("TINA0004", (932, 662, 3), "filtered"),
        ("TINA0006", (932, 662, 100), "filtered"),
        ("TINA0008", (1014, 668, 100), "filtered"),
    ):
        flame = export / "resources/flames" / (name + ".flame")
        provenance["image"].setdefault(name, {})
        shade = [*java, "ImageV1Probe", str(flame)]
        image = work / name / mode
        image.mkdir(parents=True)
        print(f"{name} {mode} image:", flush=True)
        subprocess.run([*example, mode, str(flame), str(image), str(layout[2])],
                       cwd=oracle.ROOT, check=True, timeout=600)
        raster = str(image / "raster.bin")
        if mode == "filtered":
            reference = str(image / "java-raster.bin")
            subprocess.run([
                *java, "PaddedRasterProbe", str(flame), reference,
            ], cwd=oracle.ROOT, check=True, timeout=120)
            java_bins, rust_bins = Path(reference).read_bytes(), Path(raster).read_bytes()
            compare_padded_rasters(java_bins, rust_bins, name, layout)
            mutant_raster = bytearray(rust_bins)
            mutant_raster[15] ^= 1  # flip the low count bit in the first bin
            try:
                compare_padded_rasters(java_bins, mutant_raster, name, layout)
            except oracle.OracleError as exc:
                if not str(exc).startswith("padded raster mismatch at (0, 0):"):
                    raise
                print(f"raster negative control rejected: {exc}", flush=True)
            else:
                raise oracle.OracleError("one-bin raster mutation was accepted")
            raster = reference
        subprocess.run([*shade, raster, str(image / "java.png"), mode],
                       cwd=oracle.ROOT, check=True, timeout=120)
        subprocess.run([*example, "compare", str(image)], cwd=oracle.ROOT, check=True, timeout=120)
        control = "mutate"
        outputs = ["raster.bin", "java.png", "rust.png", "diff.png", "mutant-diff.png"]
        if mode == "filtered":
            subprocess.run([*shade, raster, str(image / "java-omitted.png"), "omit"],
                           cwd=oracle.ROOT, check=True, timeout=120)
            control = "omit"
            outputs[-1:] = ["java-raster.bin", "java-omitted.png", "omitted-diff.png"]
        mutant = subprocess.run([*example, control, str(image)], cwd=oracle.ROOT, timeout=120)
        if mutant.returncode != 1:
            raise oracle.OracleError(f"image {control} control exited {mutant.returncode}, expected 1")
        provenance["image"][name][mode] = {name: oracle.sha256_file(image / name) for name in outputs}
    (work / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
