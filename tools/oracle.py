#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Materialize and execute the pinned JWildfire Java reference oracle.

This tool owns the primitives-v1 oracle for rwildfire. It never edits the
upstream checkout, never vendors upstream source or binaries, and never
downloads a toolchain. A JDK 8 must be supplied through JAVA_HOME (preferred)
or PATH.

Commands:
  python3 tools/oracle.py prepare [--source PATH]
  python3 tools/oracle.py capture --output PATH [--source PATH]

capture always compiles the probe into a fresh ignored .cache build directory
and deletes it afterwards, so stale or tampered class files cannot be taken as
evidence. Provenance is written beside the capture as <stem>.provenance.json.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path, PurePosixPath
from typing import Any, Iterable, Mapping

PINNED_COMMIT = "2a26806eb7487ea60eb3b1a554c8a73120d54343"
UPSTREAM_URL = "https://github.com/thargor6/JWildfire.git"
SUITE = "primitives-v1"
SEED = 123456789
SCHEMA_VERSION = 1
RNG_VALUE_COUNT = 16

ROOT = Path(__file__).resolve().parent.parent
CACHE = ROOT / ".cache"
MANIFEST = CACHE / "prepared.json"
SOURCE_ROOT = CACHE / "source"
BARE_REPO = CACHE / "upstream" / "JWildfire.git"
PROBE_REL_PATH = "oracle/PrimitivesV1Probe.java"
PROBE = ROOT / PROBE_REL_PATH
SUITE_REL_PATH = "fixtures/primitives-v1.suite.json"
SUITE_PATH = ROOT / SUITE_REL_PATH

# Git exports repository-routing variables into hooks. An oracle or fixture
# subprocess must never inherit the caller's repository selection.
GIT_ROUTING_ENV = frozenset(
    {
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_COMMON_DIR",
        "GIT_NAMESPACE",
        "GIT_QUARANTINE_PATH",
        "GIT_SHALLOW_FILE",
        "GIT_GRAFT_FILE",
        "GIT_REPLACE_REF_BASE",
        "GIT_CEILING_DIRECTORIES",
        "GIT_DISCOVERY_ACROSS_FILESYSTEM",
        "GIT_PREFIX",
    }
)
GIT_CONFIG_ENV = frozenset(
    {
        "GIT_CONFIG",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG_COUNT",
    }
)
GIT_CONFIG_ENV_PREFIXES = ("GIT_CONFIG_KEY_", "GIT_CONFIG_VALUE_")
GIT_ATTRIBUTE_ENV = frozenset({"GIT_ATTR_SOURCE"})

# Command-line overrides keep user configuration out of fixture commits and
# archive/status behavior without editing any user or repository config.
GIT_SAFE_CONFIG = (
    "-c",
    "commit.gpgsign=false",
    "-c",
    "tag.gpgsign=false",
    "-c",
    f"core.hooksPath={os.devnull}",
    "-c",
    f"core.attributesFile={os.devnull}",
    "-c",
    f"core.excludesFile={os.devnull}",
)

FULL_SHA_RE = re.compile(r"^[0-9a-f]{40}$")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
JAVA8_VERSION_RE = re.compile(r"^1\.8\.")
JAVAC8_VERSION_RE = re.compile(r"^javac 1\.8\.")

TOP_LEVEL_KEYS = {"schema_version", "upstream_commit", "suite", "seed", "cases"}
CASE_KEYS = {"id", "values"}
DESCRIPTOR_KEYS = {
    "schema_version",
    "suite",
    "seed",
    "probe",
    "probe_sha256",
    "case_count",
    "cases",
}
DESCRIPTOR_CASE_KEYS = {"id", "dimension"}
MANIFEST_KEYS = {
    "schema_version",
    "upstream_commit",
    "upstream_source",
    "source_clean",
    "source_check",
    "source_tree",
    "source_digest_sha256",
    "export",
}


class OracleError(Exception):
    """A clear, user-facing failure. Never reports success."""


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _decode(data: bytes) -> str:
    return data.decode("utf-8", errors="replace")


def git_environment() -> dict[str, str]:
    """Return a Git environment with repository/config routing removed.

    Non-Git variables and ordinary Git variables (SSH, prompts, tracing) are
    preserved. System and global configuration are forced off; command-level
    config variables are removed and counted as zero.
    """

    env = {
        key: value
        for key, value in os.environ.items()
        if key not in GIT_ROUTING_ENV
        and key not in GIT_CONFIG_ENV
        and key not in GIT_ATTRIBUTE_ENV
        and not key.startswith(GIT_CONFIG_ENV_PREFIXES)
    }
    env["GIT_CONFIG_NOSYSTEM"] = "1"
    env["GIT_CONFIG_GLOBAL"] = os.devnull
    env["GIT_CONFIG_SYSTEM"] = os.devnull
    env["GIT_CONFIG_COUNT"] = "0"
    env["GIT_ATTR_NOSYSTEM"] = "1"
    return env


def git_argv(args: list[str], *, git_dir: Path | None = None) -> list[str]:
    argv = ["git", *GIT_SAFE_CONFIG]
    if git_dir is not None:
        argv.append(f"--git-dir={git_dir}")
    argv.extend(args)
    return argv


def run_process(
    argv: list[str],
    *,
    cwd: Path | None = None,
    env: Mapping[str, str] | None = None,
    timeout: int = 600,
    label: str | None = None,
) -> subprocess.CompletedProcess[bytes]:
    description = label or " ".join(argv[:3])
    try:
        return subprocess.run(
            argv,
            cwd=str(cwd) if cwd is not None else None,
            env=dict(env) if env is not None else None,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
            check=False,
        )
    except FileNotFoundError as exc:
        raise OracleError(
            f"required executable not found for {description}: {exc.filename}"
        ) from exc
    except subprocess.TimeoutExpired as exc:
        raise OracleError(f"{description} timed out after {timeout} seconds") from exc
    except OSError as exc:
        raise OracleError(f"could not execute {description}: {exc}") from exc


def run_git(
    args: list[str],
    *,
    cwd: Path | None = None,
    git_dir: Path | None = None,
    timeout: int = 600,
    label: str | None = None,
) -> subprocess.CompletedProcess[bytes]:
    return run_process(
        git_argv(args, git_dir=git_dir),
        cwd=cwd,
        env=git_environment(),
        timeout=timeout,
        label=label or f"git {' '.join(args[:2])}",
    )


def git(
    args: list[str],
    *,
    cwd: Path | None = None,
    git_dir: Path | None = None,
    label: str | None = None,
) -> subprocess.CompletedProcess[bytes]:
    argv = git_argv(args, git_dir=git_dir)
    result = run_git(args, cwd=cwd, git_dir=git_dir, label=label)
    if result.returncode != 0:
        detail = _decode(result.stderr).strip() or _decode(result.stdout).strip()
        raise OracleError(
            f"git command failed ({' '.join(argv)}): {detail or 'no diagnostic'}"
        )
    return result


def _git_text(args: list[str], *, cwd: Path | None = None, git_dir: Path | None = None) -> str:
    return _decode(git(args, cwd=cwd, git_dir=git_dir).stdout).strip()


def _status_paths(raw: bytes) -> list[str]:
    paths: list[str] = []
    for entry in raw.split(b"\0"):
        if not entry:
            continue
        # status -z is "XY path" for ordinary entries and "XY old\0new" for
        # renames. The path after the first three bytes is enough for reporting.
        text = _decode(entry)
        paths.append(text[3:] if len(text) > 3 else text)
    return paths


def verify_worktree(source: Path) -> tuple[str, str, str]:
    """Reject anything that is not an unmodified checkout of the pinned commit.

    Returns (head_commit, tree_sha, source_check). The check deliberately
    rejects untracked files and hidden index flags (assume-unchanged /
    skip-worktree) as well as ordinary staged or working-tree changes.
    """

    if not source.is_dir():
        raise OracleError(f"--source is not a directory: {source}")
    try:
        inside = _git_text(["rev-parse", "--is-inside-work-tree"], cwd=source)
    except OracleError as exc:
        raise OracleError(f"--source is not a Git working tree: {source}") from exc
    if inside != "true":
        raise OracleError(f"--source is not a Git working tree: {source}")

    head = _git_text(["rev-parse", "--verify", "HEAD^{commit}"], cwd=source)
    if head != PINNED_COMMIT:
        raise OracleError(
            "upstream checkout is at "
            f"{head}, expected pinned commit {PINNED_COMMIT}"
        )

    status = git(
        ["status", "--porcelain=v1", "-z", "--untracked-files=all", "--ignore-submodules=none"],
        cwd=source,
        label="git status",
    ).stdout
    changed = _status_paths(status)
    if changed:
        shown = ", ".join(changed[:5])
        more = "" if len(changed) <= 5 else f" (+{len(changed) - 5} more)"
        raise OracleError(
            "upstream checkout is not clean; changed tracked or untracked path(s): "
            f"{shown}{more}. The oracle refuses to claim pinned identity for a dirty tree."
        )

    flags_raw = git(["ls-files", "-v", "-z"], cwd=source, label="git ls-files -v").stdout
    hidden: list[str] = []
    for entry in flags_raw.split(b"\0"):
        if not entry:
            continue
        if entry[:1] != b"H":
            hidden.append(_decode(entry[2:] if len(entry) > 2 else entry))
    if hidden:
        shown = ", ".join(hidden[:5])
        more = "" if len(hidden) <= 5 else f" (+{len(hidden) - 5} more)"
        raise OracleError(
            "upstream checkout has hidden index flags (assume-unchanged or "
            f"skip-worktree): {shown}{more}. Refusing to trust git status alone."
        )

    tree = _git_text(["rev-parse", "--verify", "HEAD^{tree}"], cwd=source)
    return head, tree, "git-worktree-clean+index-flags-clean-v1"


def tree_digest(root: Path) -> str:
    """Hash the exact exported file set and contents, independent of paths."""

    digest = hashlib.sha256()
    entries: list[tuple[str, Path]] = []
    for dirpath, dirnames, filenames in os.walk(root, followlinks=False):
        dirnames.sort()
        base = Path(dirpath)
        for name in sorted(filenames):
            path = base / name
            entries.append((path.relative_to(root).as_posix(), path))
        for name in sorted(dirnames):
            path = base / name
            if path.is_symlink():
                entries.append((path.relative_to(root).as_posix(), path))

    for relative, path in sorted(entries, key=lambda item: item[0]):
        encoded_path = relative.encode("utf-8")
        if path.is_symlink():
            target = os.readlink(path).encode("utf-8")
            digest.update(b"symlink\0" + encoded_path + b"\0" + target + b"\0")
        elif path.is_file():
            size = path.stat().st_size
            digest.update(
                b"file\0"
                + encoded_path
                + b"\0"
                + str(size).encode("ascii")
                + b"\0"
            )
            with path.open("rb") as handle:
                for chunk in iter(lambda: handle.read(1024 * 1024), b""):
                    digest.update(chunk)
            digest.update(b"\0")
        else:
            raise OracleError(f"unsupported exported entry type: {relative}")
    return digest.hexdigest()


def _safe_relative_member(name: str) -> PurePosixPath:
    relative = PurePosixPath(name)
    if relative.is_absolute() or ".." in relative.parts or not relative.parts:
        raise OracleError(f"unsafe path in git archive: {name!r}")
    return relative


def extract_git_archive(source: Path | None, git_dir: Path | None, staging: Path) -> None:
    """Stream git archive of the pinned commit into staging without a worktree."""

    if source is not None:
        argv = git_argv(["-C", str(source), "archive", "--format=tar", PINNED_COMMIT])
    else:
        if git_dir is None:
            raise OracleError("internal error: no source checkout or Git object store supplied")
        argv = git_argv(["archive", "--format=tar", PINNED_COMMIT], git_dir=git_dir)

    CACHE.mkdir(parents=True, exist_ok=True)
    fd, err_name = tempfile.mkstemp(dir=CACHE, prefix=".archive-stderr-")
    os.close(fd)
    err_path = Path(err_name)
    process = None
    try:
        with err_path.open("wb") as err_file:
            try:
                process = subprocess.Popen(
                    argv,
                    env=git_environment(),
                    stdout=subprocess.PIPE,
                    stderr=err_file,
                )
            except FileNotFoundError as exc:
                raise OracleError(f"required executable not found: {exc.filename}") from exc
            except OSError as exc:
                raise OracleError(f"could not execute git archive: {exc}") from exc

            try:
                if process.stdout is None:
                    raise OracleError("git archive produced no stdout stream")
                with tarfile.open(fileobj=process.stdout, mode="r|") as archive:
                    seen: set[str] = set()
                    for member in archive:
                        relative = _safe_relative_member(member.name)
                        key = relative.as_posix()
                        if key in seen:
                            raise OracleError(f"duplicate path in git archive: {key}")
                        seen.add(key)
                        destination = staging.joinpath(*relative.parts)

                        if member.isdir():
                            destination.mkdir(parents=True, exist_ok=True)
                        elif member.isfile():
                            destination.parent.mkdir(parents=True, exist_ok=True)
                            extracted = archive.extractfile(member)
                            if extracted is None:
                                raise OracleError(f"could not read archive file: {key}")
                            with destination.open("wb") as handle:
                                shutil.copyfileobj(extracted, handle)
                            os.chmod(destination, member.mode & 0o777)
                        elif member.issym():
                            raise OracleError(
                                f"git archive contains a symlink, which this oracle cannot "
                                f"verify safely: {key}"
                            )
                        elif member.islnk():
                            raise OracleError(
                                f"git archive contains a hardlink, which this oracle cannot "
                                f"verify safely: {key}"
                            )
                        else:
                            raise OracleError(f"unsupported git archive entry: {key}")

                returncode = process.wait()
                if returncode != 0:
                    err_file.flush()
                    detail = err_path.read_text(encoding="utf-8", errors="replace").strip()
                    raise OracleError(
                        f"git archive failed (exit {returncode}): {detail or 'no diagnostic'}"
                    )
            except tarfile.TarError as exc:
                raise OracleError(f"could not read git archive: {exc}") from exc
            finally:
                if process is not None and process.poll() is None:
                    process.kill()
                    process.wait()
                if process is not None and process.stdout is not None:
                    process.stdout.close()
    finally:
        err_path.unlink(missing_ok=True)


def export_source(source: Path | None, git_dir: Path | None, source_check: str, tree: str) -> dict[str, Any]:
    """Export the pinned commit to an immutable cache directory and describe it."""

    SOURCE_ROOT.mkdir(parents=True, exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix="export-", dir=SOURCE_ROOT))
    keep = False
    try:
        extract_git_archive(source, git_dir, staging)
        for required in (staging / "src", staging / "lib"):
            if not required.is_dir():
                raise OracleError(
                    f"pinned upstream export is missing required directory: {required.name}"
                )
        if not list((staging / "lib").glob("*.jar")):
            raise OracleError("pinned upstream export has no lib/*.jar dependencies")
        digest = tree_digest(staging)

        dest = SOURCE_ROOT / PINNED_COMMIT / digest[:16]
        if dest.exists():
            existing_digest = tree_digest(dest)
            if existing_digest != digest:
                raise OracleError(
                    "cached source export digest mismatch; refusing to reuse it: "
                    f"{dest}"
                )
        else:
            dest.parent.mkdir(parents=True, exist_ok=True)
            try:
                os.replace(staging, dest)
                keep = True
            except OSError as exc:
                raise OracleError(f"could not publish source export: {exc}") from exc

        info = {
            "schema_version": SCHEMA_VERSION,
            "upstream_commit": PINNED_COMMIT,
            "upstream_source": UPSTREAM_URL,
            "source_clean": True,
            "source_check": source_check,
            "source_tree": tree,
            "source_digest_sha256": digest,
            "export": dest.relative_to(CACHE).as_posix(),
        }
        write_json_atomic(MANIFEST, info)
        return info
    finally:
        if not keep and staging.exists():
            shutil.rmtree(staging, ignore_errors=True)


def ensure_bare_repo() -> Path:
    """Fetch and cache the pinned commit as a bare Git object store."""

    BARE_REPO.parent.mkdir(parents=True, exist_ok=True)
    if not BARE_REPO.exists():
        result = run_git(
            ["init", "--bare", str(BARE_REPO)],
            label="git init --bare",
        )
        if result.returncode != 0:
            raise OracleError(
                "git init --bare failed: "
                + (_decode(result.stderr).strip() or "no diagnostic")
            )

    def has_commit() -> bool:
        result = run_git(
            ["cat-file", "-e", f"{PINNED_COMMIT}^{{commit}}"],
            git_dir=BARE_REPO,
            label="git cat-file",
        )
        return result.returncode == 0

    if not has_commit():
        result = run_git(
            [
                "fetch",
                "--depth",
                "1",
                UPSTREAM_URL,
                PINNED_COMMIT,
            ],
            git_dir=BARE_REPO,
            timeout=900,
            label="git fetch pinned commit",
        )
        if result.returncode != 0:
            detail = _decode(result.stderr).strip() or _decode(result.stdout).strip()
            raise OracleError(
                "could not fetch pinned upstream commit "
                f"{PINNED_COMMIT} from {UPSTREAM_URL}: {detail or 'no diagnostic'}. "
                "No implicit source substitution is allowed; pass --source with a "
                "clean pinned checkout instead."
            )
    if not has_commit():
        raise OracleError(
            f"cached upstream repository is missing pinned commit {PINNED_COMMIT}"
        )
    return BARE_REPO


def prepare_from_source(source: Path) -> dict[str, Any]:
    head, tree, check = verify_worktree(source)
    if head != PINNED_COMMIT:
        raise OracleError(
            f"upstream checkout is at {head}, expected pinned commit {PINNED_COMMIT}"
        )
    return export_source(source, None, check, tree)


def prepare_from_cache() -> dict[str, Any]:
    repo = ensure_bare_repo()
    tree = _git_text(["rev-parse", "--verify", f"{PINNED_COMMIT}^{{tree}}"], git_dir=repo)
    return export_source(None, repo, "git-bare-object-v1", tree)


def load_manifest() -> dict[str, Any]:
    if not MANIFEST.is_file():
        raise OracleError(
            "no prepared upstream source; run 'python3 tools/oracle.py prepare' "
            "or pass --source PATH"
        )
    info = read_json_strict(MANIFEST.read_bytes(), "prepared source manifest")
    if not isinstance(info, dict) or set(info) != MANIFEST_KEYS:
        raise OracleError("prepared source manifest has an unexpected schema")
    if info.get("schema_version") != SCHEMA_VERSION:
        raise OracleError("prepared source manifest schema_version is not 1")
    if info.get("upstream_commit") != PINNED_COMMIT:
        raise OracleError("prepared source manifest does not reference the pinned commit")
    if info.get("source_clean") is not True:
        raise OracleError("prepared source manifest does not claim a clean pinned source")
    if not isinstance(info.get("source_check"), str) or not info["source_check"]:
        raise OracleError("prepared source manifest has no source check method")
    if not isinstance(info.get("source_tree"), str) or not FULL_SHA_RE.match(
        info["source_tree"]
    ):
        raise OracleError("prepared source manifest has an invalid source tree")
    if not isinstance(info.get("source_digest_sha256"), str) or not SHA256_RE.match(
        info["source_digest_sha256"]
    ):
        raise OracleError("prepared source manifest has an invalid source digest")
    export_value = info.get("export")
    if not isinstance(export_value, str):
        raise OracleError("prepared source manifest has no export path")
    export = (CACHE / export_value).resolve()
    try:
        export.relative_to(CACHE.resolve())
    except ValueError as exc:
        raise OracleError("prepared source manifest export path escapes .cache") from exc
    if not export.is_dir():
        raise OracleError(
            "prepared source export is missing; rerun 'python3 tools/oracle.py prepare'"
        )
    actual_digest = tree_digest(export)
    if actual_digest != info.get("source_digest_sha256"):
        raise OracleError(
            "prepared source export digest mismatch; rerun "
            "'python3 tools/oracle.py prepare'"
        )
    return info


def resolve_source(source: Path | None) -> dict[str, Any]:
    if source is not None:
        return prepare_from_source(source)
    return load_manifest()


def resolve_toolchain() -> dict[str, Any]:
    java_home = os.environ.get("JAVA_HOME", "").strip()
    if java_home:
        java = Path(java_home) / "bin" / "java"
        javac = Path(java_home) / "bin" / "javac"
        if not (java.is_file() and os.access(java, os.X_OK)):
            raise OracleError(
                f"JAVA_HOME is set but {java} is missing or not executable"
            )
        if not (javac.is_file() and os.access(javac, os.X_OK)):
            raise OracleError(
                f"JAVA_HOME is set but {javac} is missing or not executable"
            )
    else:
        java_name = shutil.which("java")
        javac_name = shutil.which("javac")
        if not java_name:
            raise OracleError(
                "no Java runtime supplied; set JAVA_HOME to a JDK 8 or put java on PATH"
            )
        if not javac_name:
            raise OracleError(
                "no Java compiler supplied; set JAVA_HOME to a JDK 8 or put javac on PATH"
            )
        java = Path(java_name)
        javac = Path(javac_name)

    java_result = run_process([str(java), "-version"], label="java -version")
    if java_result.returncode != 0:
        detail = _decode(java_result.stderr).strip() or _decode(java_result.stdout).strip()
        raise OracleError(f"java -version failed: {detail or 'no diagnostic'}")
    java_lines = [
        line
        for line in (_decode(java_result.stdout) + _decode(java_result.stderr)).splitlines()
        if line.strip()
    ]
    java_version = None
    for line in java_lines:
        match = re.search(r'version "([^"]+)"', line)
        if match:
            java_version = match.group(1)
            break
    if not java_version or not JAVA8_VERSION_RE.match(java_version):
        raise OracleError(
            "oracle requires a JDK 8 runtime (found "
            f"{java_version or 'unknown'}); no toolchain download is attempted"
        )

    javac_result = run_process([str(javac), "-version"], label="javac -version")
    javac_text = (
        _decode(javac_result.stdout).strip() or _decode(javac_result.stderr).strip()
    )
    if javac_result.returncode != 0:
        raise OracleError(f"javac -version failed: {javac_text or 'no diagnostic'}")
    javac_version = javac_text.splitlines()[0] if javac_text else ""
    if not JAVAC8_VERSION_RE.match(javac_version):
        raise OracleError(
            "oracle requires a JDK 8 compiler (found "
            f"{javac_version or 'unknown'}); no toolchain download is attempted"
        )

    return {
        "java": str(java),
        "javac": str(javac),
        "java_version": java_version,
        "java_version_output": java_lines,
        "javac_version": javac_version,
    }


def classpath(export: Path) -> str:
    jars = sorted((export / "lib").glob("*.jar"))
    if not jars:
        raise OracleError("no lib/*.jar files found in the pinned source export")
    return os.pathsep.join(str(path) for path in jars)


def compile_and_run(export: Path, toolchain: dict[str, Any], probe_digest: str) -> bytes:
    CACHE.mkdir(parents=True, exist_ok=True)
    build_root = CACHE / "build"
    build_root.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="capture-", dir=build_root))
    try:
        classes = work / "classes"
        classes.mkdir()
        cp = classpath(export)
        compile_cmd = [
            toolchain["javac"],
            "-encoding",
            "ISO-8859-1",
            "-source",
            "1.8",
            "-target",
            "1.8",
            "-proc:none",
            "-nowarn",
            "-cp",
            cp,
            "-sourcepath",
            str(export / "src"),
            "-d",
            str(classes),
            str(PROBE),
        ]
        compiled = run_process(compile_cmd, cwd=ROOT, timeout=600, label="javac oracle probe")
        if compiled.returncode != 0:
            detail = _decode(compiled.stderr).strip() or _decode(compiled.stdout).strip()
            raise OracleError(
                "javac failed for the pinned source tree: "
                f"{detail[-4000:] if detail else 'no diagnostic'}"
            )
        if not (classes / "PrimitivesV1Probe.class").is_file():
            raise OracleError("javac produced no PrimitivesV1Probe.class")
        class_count = sum(1 for _ in classes.rglob("*.class"))
        if class_count == 0:
            raise OracleError("javac produced an empty class tree")

        # The build is intentionally unique per capture. Nothing from a prior
        # run is reused, so class-file freshness does not depend on a stamp.
        run_cmd = [
            toolchain["java"],
            "-cp",
            os.pathsep.join([str(classes), cp]),
            "PrimitivesV1Probe",
            "--commit",
            PINNED_COMMIT,
        ]
        executed = run_process(run_cmd, cwd=work, timeout=300, label="java oracle probe")
        if executed.returncode != 0:
            detail = _decode(executed.stderr).strip() or _decode(executed.stdout).strip()
            raise OracleError(
                f"oracle probe failed (exit {executed.returncode}, probe {probe_digest[:12]}): "
                f"{detail[-4000:] if detail else 'no diagnostic'}"
            )
        if not executed.stdout.strip():
            raise OracleError("oracle probe produced empty stdout; refusing to report success")
        return executed.stdout
    finally:
        shutil.rmtree(work, ignore_errors=True)


def read_json_strict(raw: bytes, label: str) -> Any:
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise OracleError(f"{label} is not valid UTF-8: {exc}") from exc

    def reject_constant(value: str) -> None:
        raise OracleError(f"{label} contains a non-finite JSON literal: {value}")

    def no_duplicate_keys(pairs: Iterable[tuple[str, Any]]) -> dict[str, Any]:
        result: dict[str, Any] = {}
        for key, value in pairs:
            if key in result:
                raise OracleError(f"{label} contains duplicate JSON key: {key}")
            result[key] = value
        return result

    try:
        return json.loads(
            text,
            parse_constant=reject_constant,
            object_pairs_hook=no_duplicate_keys,
        )
    except OracleError:
        raise
    except json.JSONDecodeError as exc:
        raise OracleError(f"{label} is not valid JSON: {exc}") from exc


def _finite_number(value: Any, label: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise OracleError(f"{label} must be a JSON number")
    try:
        as_float = float(value)
    except (OverflowError, ValueError) as exc:
        raise OracleError(f"{label} cannot be represented as a finite f64") from exc
    if not math.isfinite(as_float):
        raise OracleError(f"{label} must be finite")
    return as_float


def load_descriptor() -> dict[str, Any]:
    if not SUITE_PATH.is_file():
        raise OracleError(f"checked suite descriptor is missing: {SUITE_REL_PATH}")
    descriptor = read_json_strict(SUITE_PATH.read_bytes(), "suite descriptor")
    if not isinstance(descriptor, dict):
        raise OracleError("suite descriptor must be a JSON object")
    if set(descriptor) != DESCRIPTOR_KEYS:
        missing = sorted(DESCRIPTOR_KEYS - set(descriptor))
        extra = sorted(set(descriptor) - DESCRIPTOR_KEYS)
        raise OracleError(
            "suite descriptor fields do not match the checked contract "
            f"(missing={missing}, extra={extra})"
        )
    if (
        isinstance(descriptor["schema_version"], bool)
        or not isinstance(descriptor["schema_version"], int)
        or descriptor["schema_version"] != SCHEMA_VERSION
    ):
        raise OracleError("suite descriptor schema_version is not 1")
    if descriptor["suite"] != SUITE:
        raise OracleError(f"suite descriptor suite is not {SUITE!r}")
    if descriptor["seed"] != SEED or isinstance(descriptor["seed"], bool):
        raise OracleError(f"suite descriptor seed is not {SEED}")
    if descriptor["probe"] != PROBE_REL_PATH:
        raise OracleError("suite descriptor does not name the checked probe")
    if not isinstance(descriptor["probe_sha256"], str) or not SHA256_RE.match(
        descriptor["probe_sha256"]
    ):
        raise OracleError("suite descriptor probe_sha256 must be a lowercase SHA-256 hex digest")
    cases = descriptor["cases"]
    if not isinstance(cases, list) or not cases:
        raise OracleError("suite descriptor cases must be a nonempty array")
    if descriptor["case_count"] != len(cases) or isinstance(descriptor["case_count"], bool):
        raise OracleError("suite descriptor case_count does not match its cases array")
    seen: set[str] = set()
    for index, case in enumerate(cases):
        if not isinstance(case, dict) or set(case) != DESCRIPTOR_CASE_KEYS:
            raise OracleError(f"suite descriptor case {index} has an invalid schema")
        case_id = case["id"]
        dimension = case["dimension"]
        if not isinstance(case_id, str) or not case_id:
            raise OracleError(f"suite descriptor case {index} has an empty id")
        if case_id in seen:
            raise OracleError(f"suite descriptor contains duplicate id: {case_id}")
        seen.add(case_id)
        if isinstance(dimension, bool) or not isinstance(dimension, int) or dimension <= 0:
            raise OracleError(f"suite descriptor case {case_id} has an invalid dimension")
    return descriptor


def validate_trace(doc: Any, descriptor: dict[str, Any], probe_digest: str) -> None:
    if not isinstance(doc, dict):
        raise OracleError("oracle output must be a JSON object")
    if set(doc) != TOP_LEVEL_KEYS:
        missing = sorted(TOP_LEVEL_KEYS - set(doc))
        extra = sorted(set(doc) - TOP_LEVEL_KEYS)
        raise OracleError(
            f"oracle output fields do not match the trace contract "
            f"(missing={missing}, extra={extra})"
        )
    if (
        isinstance(doc["schema_version"], bool)
        or not isinstance(doc["schema_version"], int)
        or doc["schema_version"] != SCHEMA_VERSION
    ):
        raise OracleError("oracle output schema_version is not 1")
    commit = doc["upstream_commit"]
    if not isinstance(commit, str) or not FULL_SHA_RE.match(commit):
        raise OracleError("oracle output upstream_commit is not a full lowercase SHA")
    if commit != PINNED_COMMIT:
        raise OracleError(
            f"oracle output upstream_commit {commit} does not match pinned {PINNED_COMMIT}"
        )
    if doc["suite"] != SUITE:
        raise OracleError(f"oracle output suite is not {SUITE!r}")
    if doc["seed"] != SEED or isinstance(doc["seed"], bool):
        raise OracleError(f"oracle output seed is not {SEED}")

    cases = doc["cases"]
    if not isinstance(cases, list) or not cases:
        raise OracleError("oracle output cases must be a nonempty array")
    descriptor_by_id = {case["id"]: case["dimension"] for case in descriptor["cases"]}
    seen: set[str] = set()
    for index, case in enumerate(cases):
        if not isinstance(case, dict) or set(case) != CASE_KEYS:
            raise OracleError(f"oracle output case {index} has an invalid schema")
        case_id = case["id"]
        if not isinstance(case_id, str) or not case_id:
            raise OracleError(f"oracle output case {index} has an empty id")
        if case_id in seen:
            raise OracleError(f"oracle output contains duplicate id: {case_id}")
        seen.add(case_id)
        values = case["values"]
        if not isinstance(values, list) or not values:
            raise OracleError(f"oracle output case {case_id} has no values")
        for component, value in enumerate(values):
            _finite_number(value, f"oracle output case {case_id} component {component}")

    expected_ids = set(descriptor_by_id)
    if seen != expected_ids:
        missing = sorted(expected_ids - seen)
        extra = sorted(seen - expected_ids)
        raise OracleError(
            "oracle case set does not match the checked suite descriptor "
            f"(missing={missing}, extra={extra})"
        )
    for case in cases:
        expected_dimension = descriptor_by_id[case["id"]]
        if len(case["values"]) != expected_dimension:
            raise OracleError(
                f"oracle output case {case['id']} has dimension {len(case['values'])}, "
                f"descriptor requires {expected_dimension}"
            )

    expected_probe_digest = descriptor["probe_sha256"]
    if probe_digest != expected_probe_digest:
        raise OracleError(
            "checked probe digest does not match the suite descriptor; update the "
            "descriptor deliberately when the probe changes"
        )


def write_bytes_atomic(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temporary = Path(temporary_name)
    try:
        with os.fdopen(fd, "wb") as handle:
            handle.write(data)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
    finally:
        if temporary.exists():
            temporary.unlink()


def write_json_atomic(path: Path, value: Any) -> None:
    rendered = json.dumps(
        value,
        indent=2,
        ensure_ascii=False,
        allow_nan=False,
        sort_keys=False,
    ).encode("utf-8") + b"\n"
    write_bytes_atomic(path, rendered)


def sidecar_for(output: Path) -> Path:
    return output.with_name(output.stem + ".provenance.json")


def cmd_prepare(args: argparse.Namespace) -> int:
    toolchain = resolve_toolchain()
    info = (
        prepare_from_source(args.source)
        if args.source is not None
        else prepare_from_cache()
    )
    print(
        "prepared "
        f"{SUITE}: commit {info['upstream_commit']} tree {info['source_tree']} "
        f"digest {info['source_digest_sha256']} at .cache/{info['export']}"
    )
    print(
        f"toolchain: java {toolchain['java_version']}, {toolchain['javac_version']}"
    )
    return 0


def cmd_capture(args: argparse.Namespace) -> int:
    toolchain = resolve_toolchain()
    descriptor = load_descriptor()
    probe_digest = sha256_file(PROBE)
    info = resolve_source(args.source)

    raw = compile_and_run(
        CACHE / info["export"],
        toolchain,
        probe_digest,
    )
    doc = read_json_strict(raw, "oracle output")
    validate_trace(doc, descriptor, probe_digest)

    rendered = json.dumps(
        doc,
        indent=2,
        ensure_ascii=False,
        allow_nan=False,
        sort_keys=False,
    ).encode("utf-8") + b"\n"

    output = Path(args.output).expanduser()
    if output.exists() and output.is_dir():
        raise OracleError(f"--output is a directory: {output}")

    descriptor_digest = sha256_file(SUITE_PATH)
    provenance = {
        "schema_version": SCHEMA_VERSION,
        "upstream_commit": info["upstream_commit"],
        "upstream_source": UPSTREAM_URL,
        "suite": SUITE,
        "seed": SEED,
        "source_clean": True,
        "source_check": info["source_check"],
        "source_tree": info["source_tree"],
        "source_digest_sha256": info["source_digest_sha256"],
        "probe_path": PROBE_REL_PATH,
        "probe_sha256": probe_digest,
        "suite_descriptor_path": SUITE_REL_PATH,
        "suite_descriptor_sha256": descriptor_digest,
        "java_version": toolchain["java_version"],
        "java_version_output": toolchain["java_version_output"],
        "javac_version": toolchain["javac_version"],
        "math_library": "FAST_MATH",
        "oracle_protocol": "json-stdout-v1",
        "build_mode": "fresh-temp-v1",
        "reference_sha256": sha256_bytes(rendered),
    }

    write_bytes_atomic(output, rendered)
    sidecar = sidecar_for(output)
    write_json_atomic(sidecar, provenance)
    print(
        f"captured {len(doc['cases'])} cases to {output} "
        f"(sha256 {provenance['reference_sha256']})"
    )
    print(f"wrote provenance to {sidecar}")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="tools/oracle.py",
        description="Prepare and capture the pinned JWildfire primitives-v1 oracle.",
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    prepare = subparsers.add_parser(
        "prepare",
        help="verify and export the pinned upstream source into ignored .cache",
    )
    prepare.add_argument(
        "--source",
        type=Path,
        help="clean checkout of the pinned commit to use instead of the cached bare fetch",
    )
    prepare.set_defaults(func=cmd_prepare)

    capture = subparsers.add_parser(
        "capture",
        help="run the real Java probe and write an explicit reference capture",
    )
    capture.add_argument("--output", required=True, help="reference JSON path to write")
    capture.add_argument(
        "--source",
        type=Path,
        help="clean checkout of the pinned commit; omit to reuse the last prepared export",
    )
    capture.set_defaults(func=cmd_capture)
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        return args.func(args)
    except OracleError as exc:
        print(f"oracle: error: {exc}", file=sys.stderr)
        return 2
    except OSError as exc:
        print(f"oracle: error: filesystem operation failed: {exc}", file=sys.stderr)
        return 2
    except KeyboardInterrupt:
        print("oracle: interrupted", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
