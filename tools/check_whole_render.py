"""Check repeatability through the pinned production Java CPU renderer."""

import argparse
import json
import math
import os
from pathlib import Path
import struct
import subprocess
import tempfile

import oracle


WIDTH, HEIGHT, QUALITY, SEED = 160, 120, 20, 741103597
AA_AMOUNT, AA_RADIUS = 0.25, 0.5
SCENES = ("TINA0010", "TINA0012")


def read_capture(prefix, quality):
    raster = prefix.with_suffix(".raster").read_bytes()
    rgb = prefix.with_suffix(".rgb").read_bytes()
    # Filter radius 1, default Mitchell Smooth, oversampling 1.
    w, h = WIDTH + 4, HEIGHT + 4
    if len(raster) != 8 + 16 * w * h or raster[:8] != struct.pack(">II", w, h):
        raise oracle.OracleError(f"{prefix.name}: invalid padded raster layout")
    if len(rgb) != 3 * WIDTH * HEIGHT or not any(rgb):
        raise oracle.OracleError(f"{prefix.name}: invalid or black decoded image")
    hits = 0
    for count, r, g, b in struct.iter_unpack(">ifff", memoryview(raster)[8:]):
        if count < 0 or not all(math.isfinite(c) for c in (r, g, b)):
            raise oracle.OracleError(f"{prefix.name}: invalid raster bin")
        hits += count
    budget = w * h * quality
    if not 0 < hits <= budget:
        raise oracle.OracleError(f"{prefix.name}: invalid hit count {hits} for budget {budget}")
    return raster, rgb, hits, budget


def compare(reference, candidate, label):
    for index, description in ((0, "padded raster"), (1, "decoded RGB")):
        if reference[index] != candidate[index]:
            start, stride, width = (8, 16, WIDTH + 4) if index == 0 else (0, 3, WIDTH)
            a, b = reference[index], candidate[index]
            first = next(i for i in range(start, len(a), stride) if a[i:i + stride] != b[i:i + stride])
            y, x = divmod((first - start) // stride, width)
            raise oracle.OracleError(f"{label}: {description} mismatch at ({x}, {y})")


def check(source):
    info = oracle.resolve_source(source)
    export = oracle.CACHE / info["export"]
    jdk = oracle.resolve_toolchain()
    artifacts = oracle.ROOT / "artifacts"
    artifacts.mkdir(exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="whole-render.", dir=artifacts))
    print(f"check-whole-render: artifacts={work}", flush=True)
    classes = work / "classes"
    classes.mkdir()
    probe = oracle.ROOT / "oracle/WholeRenderProbe.java"
    cp = oracle.classpath(export)
    subprocess.run([
        jdk["javac"], "-encoding", "ISO-8859-1", "-proc:none", "-cp", cp,
        "-sourcepath", str(export / "src"), "-d", str(classes), str(probe),
    ], check=True, timeout=600)
    runtime_cp = os.pathsep.join((str(classes), str(export / "src"), str(export / "resources"), cp))
    provenance = {
        "suite": "whole-render-v2", "upstream_commit": info["upstream_commit"],
        "source_tree": info["source_tree"], "source_digest_sha256": info["source_digest_sha256"],
        "java_version": jdk["java_version"], "javac_version": jdk["javac_version"],
        "probe_sha256": oracle.sha256_file(probe), "workers": 1, "seed": SEED,
        "width": WIDTH, "height": HEIGHT, "quality": QUALITY, "scenes": {},
    }
    for name in SCENES:
        scene = export / "resources/flames" / (name + ".flame")
        record = {"input_sha256": oracle.sha256_file(scene), "runs": {}}

        def run(label, quality, amount=AA_AMOUNT):
            prefix = work / f"{name}-{label}"
            home = prefix.with_suffix(".home")
            home.mkdir()
            with prefix.with_suffix(".log").open("wb") as log:
                subprocess.run([
                    jdk["java"], "-Djava.awt.headless=true", f"-Duser.home={home}",
                    "-cp", runtime_cp, "WholeRenderProbe", str(scene), str(prefix),
                    str(WIDTH), str(HEIGHT), str(quality), str(SEED), str(amount), str(AA_RADIUS),
                ], stdout=log, stderr=subprocess.STDOUT, check=True, timeout=120)
            raster, rgb, hits, budget = read_capture(prefix, quality)
            record["runs"][label] = {
                "quality": quality, "hits": hits, "sample_budget": budget,
                "antialias_amount": amount, "antialias_radius": AA_RADIUS,
                **{suffix + "_sha256": oracle.sha256_file(prefix.with_suffix("." + suffix))
                   for suffix in ("raster", "rgb", "png")},
            }
            return raster, rgb, hits, budget

        a = run("a", QUALITY)
        b = run("b", QUALITY)
        if a[0] != b[0]:
            raise oracle.OracleError(f"{name}: independent Java padded rasters differ")
        if a[1] != b[1]:
            raise oracle.OracleError(f"{name}: independent Java decoded RGB differs")
        print(f"{name}: {((WIDTH + 4) * (HEIGHT + 4)):,} bins and {WIDTH * HEIGHT:,} RGB pixels match across JVMs "
              f"({a[2]:,} hits / {a[3]:,} budget)", flush=True)
        changed = run("budget-control", QUALITY + 1)
        if changed[0] == a[0] or changed[2] <= a[2]:
            raise oracle.OracleError(f"{name}: extra sample budget did not increase raster hits")
        print(f"{name}: budget control {a[3]:,} -> {changed[3]:,} changed the raster "
              f"({a[2]:,} -> {changed[2]:,} hits)", flush=True)
        disabled = run("no-aa", QUALITY, 0.0)
        for mode, amount, reference in (("no-aa", 0.0, disabled), ("aa", AA_AMOUNT, a)):
            prefix = work / f"{name}-rust-{mode}"
            subprocess.run([
                "cargo", "run", "--quiet", "--locked", "--release", "-p", "rwildfire-cpu",
                "--example", "capture_whole_render", "--", str(scene), str(prefix),
                str(WIDTH), str(HEIGHT), str(QUALITY), str(SEED), str(amount), str(AA_RADIUS),
            ], check=True, timeout=120)
            candidate = read_capture(prefix, QUALITY)
            compare(reference, candidate, f"{name} {mode}")
            record["runs"][f"rust-{mode}"] = {
                "quality": QUALITY, "hits": candidate[2], "sample_budget": candidate[3],
                "antialias_amount": amount, "antialias_radius": AA_RADIUS,
                **{suffix + "_sha256": oracle.sha256_file(prefix.with_suffix("." + suffix))
                   for suffix in ("raster", "rgb")},
            }
            print(f"{name} {mode}: Rust matches Java in 20,336 bins and 19,200 RGB pixels", flush=True)
        provenance["scenes"][name] = record
    (work / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", nargs="?", type=Path, help="clean pinned JWildfire checkout")
    args = parser.parse_args()
    try:
        check(args.source)
    except (oracle.OracleError, OSError, subprocess.SubprocessError) as exc:
        parser.exit(1, f"check-whole-render: {exc}\n")
