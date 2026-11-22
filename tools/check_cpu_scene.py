"""Capture a deterministic CPU scene trace from the pinned Java source."""

import argparse
import json
import os
from pathlib import Path
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
    cp = oracle.classpath(export)
    subprocess.run([
        jdk["javac"], "-encoding", "ISO-8859-1", "-proc:none", "-cp", cp,
        "-sourcepath", str(export / "src"), "-d", str(classes), str(probe),
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
    for name in ("TINA0010", "TINA0004"):
        scene = export / "resources/flames" / (name + ".flame")
        trace = work / (name + ".txt")
        with (work / (name + ".stdout")).open("wb") as stdout, (work / (name + ".stderr")).open("wb") as stderr:
            subprocess.run([
                jdk["java"], "-Djava.awt.headless=true", f"-Duser.home={home}",
                "-cp", runtime_cp, "SceneV1Probe", str(scene), str(trace),
            ], cwd=oracle.ROOT, stdout=stdout, stderr=stderr, check=True, timeout=120)
        provenance["scenes"][name] = {"input_sha256": oracle.sha256_file(scene), "trace_sha256": oracle.sha256_file(trace)}
    (work / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n", encoding="utf-8")
    subprocess.run([
        "cargo", "run", "--locked", "-p", "rwildfire-cpu", "--example", "check_java_scene",
        "--", str(export), str(work),
    ], cwd=oracle.ROOT, check=True, timeout=600)


if __name__ == "__main__":
    main()
