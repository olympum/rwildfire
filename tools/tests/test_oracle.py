# SPDX-License-Identifier: GPL-3.0-or-later
"""Focused orchestration tests for the Java reference oracle.

These tests protect the oracle's identity, protocol, and checked-suite
contract. The real JDK execution is a separate gate command; no test here
pretends a missing toolchain or upstream checkout is a pass.
"""

from __future__ import annotations

import json
import math
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))

from tools import oracle  # noqa: E402


def git(repo: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", *oracle.GIT_SAFE_CONFIG, "-C", str(repo), *args],
        env=oracle.git_environment(),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
        text=True,
    )
    if result.returncode != 0:
        raise AssertionError(
            f"git {' '.join(args)} failed: {result.stderr or result.stdout}"
        )
    return result.stdout.strip()


def make_upstream_repo(root: Path) -> tuple[Path, str]:
    repo = root / "upstream"
    repo.mkdir()
    git(repo, "init", "-q")
    (repo / "src").mkdir()
    (repo / "lib").mkdir()
    (repo / "src" / "Example.java").write_text("class Example {}\n", encoding="utf-8")
    (repo / "lib" / "dependency.jar").write_bytes(b"not a real jar")
    git(repo, "add", ".")
    git(
        repo,
        "-c",
        "user.email=oracle@example.invalid",
        "-c",
        "user.name=Oracle Test",
        "commit",
        "-q",
        "-m",
        "fixture upstream",
    )
    commit = git(repo, "rev-parse", "HEAD")
    return repo, commit


class OracleCacheTestCase(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = Path(tempfile.mkdtemp(prefix="oracle-test-"))
        self.original = (
            oracle.CACHE,
            oracle.SOURCE_ROOT,
            oracle.MANIFEST,
            oracle.BARE_REPO,
            oracle.UPSTREAM_URL,
            oracle.PINNED_COMMIT,
        )
        oracle.CACHE = self.temp / ".cache"
        oracle.SOURCE_ROOT = oracle.CACHE / "source"
        oracle.MANIFEST = oracle.CACHE / "prepared.json"
        oracle.BARE_REPO = oracle.CACHE / "upstream" / "JWildfire.git"
        self.repo, self.commit = make_upstream_repo(self.temp)
        oracle.PINNED_COMMIT = self.commit
        oracle.UPSTREAM_URL = self.repo.as_uri()

    def tearDown(self) -> None:
        (
            oracle.CACHE,
            oracle.SOURCE_ROOT,
            oracle.MANIFEST,
            oracle.BARE_REPO,
            oracle.UPSTREAM_URL,
            oracle.PINNED_COMMIT,
        ) = self.original
        shutil.rmtree(self.temp, ignore_errors=True)


class SourceIdentityTests(OracleCacheTestCase):
    def test_clean_pinned_checkout_is_exported_and_manifested(self) -> None:
        info = oracle.prepare_from_source(self.repo)
        self.assertEqual(info["upstream_commit"], self.commit)
        self.assertTrue(info["source_clean"])
        self.assertIn("index-flags-clean", info["source_check"])
        self.assertEqual(info["source_tree"], git(self.repo, "rev-parse", "HEAD^{tree}"))
        export = oracle.CACHE / info["export"]
        self.assertEqual(oracle.tree_digest(export), info["source_digest_sha256"])
        manifest = oracle.load_manifest()
        self.assertEqual(manifest, info)

    def test_default_bare_cache_fetches_the_pinned_commit(self) -> None:
        info = oracle.prepare_from_cache()
        self.assertEqual(info["source_check"], "git-bare-object-v1")
        self.assertEqual(oracle.load_manifest(), info)
        self.assertEqual(oracle.tree_digest(oracle.CACHE / info["export"]), info["source_digest_sha256"])

    def test_tracked_change_is_rejected(self) -> None:
        (self.repo / "src" / "Example.java").write_text("class Changed {}\n", encoding="utf-8")
        with self.assertRaisesRegex(oracle.OracleError, "not clean"):
            oracle.prepare_from_source(self.repo)

    def test_untracked_path_is_rejected(self) -> None:
        (self.repo / "untracked.txt").write_text("unexpected\n", encoding="utf-8")
        with self.assertRaisesRegex(oracle.OracleError, "untracked path"):
            oracle.prepare_from_source(self.repo)

    def test_hidden_assume_unchanged_flag_is_rejected(self) -> None:
        tracked = self.repo / "src" / "Example.java"
        tracked.write_text("class Hidden {}\n", encoding="utf-8")
        git(self.repo, "update-index", "--assume-unchanged", "src/Example.java")
        # The ordinary status porcelain is intentionally blind here; only the
        # index-flag check catches it.
        self.assertEqual(git(self.repo, "status", "--porcelain=v1"), "")
        with self.assertRaisesRegex(oracle.OracleError, "hidden index flags"):
            oracle.prepare_from_source(self.repo)

    def test_wrong_head_is_rejected(self) -> None:
        (self.repo / "src" / "Second.java").write_text("class Second {}\n", encoding="utf-8")
        git(self.repo, "add", ".")
        git(
            self.repo,
            "-c",
            "user.email=oracle@example.invalid",
            "-c",
            "user.name=Oracle Test",
            "commit",
            "-q",
            "-m",
            "second",
        )
        with self.assertRaisesRegex(oracle.OracleError, "expected pinned commit"):
            oracle.prepare_from_source(self.repo)


class GitRoutingIsolationTests(unittest.TestCase):
    """A poisoned Git environment must not reach either fixture or oracle.

    This is the regression for the pre-fix failure where a hook-exported
    GIT_DIR/GIT_WORK_TREE routed fixture git init/config/add/commit into the
    caller's repository, while global signing and hooks also applied.
    """

    def setUp(self) -> None:
        self.temp = Path(tempfile.mkdtemp(prefix="oracle-git-env-test-"))
        self.original = (
            oracle.CACHE,
            oracle.SOURCE_ROOT,
            oracle.MANIFEST,
            oracle.BARE_REPO,
            oracle.PINNED_COMMIT,
        )
        oracle.CACHE = self.temp / ".cache"
        oracle.SOURCE_ROOT = oracle.CACHE / "source"
        oracle.MANIFEST = oracle.CACHE / "prepared.json"
        oracle.BARE_REPO = oracle.CACHE / "upstream" / "JWildfire.git"

        self.victim = self.temp / "victim"
        self.victim.mkdir()
        git(self.victim, "init", "-q")
        git(self.victim, "config", "user.email", "victim@example.invalid")
        git(self.victim, "config", "user.name", "Victim")
        (self.victim / "victim.txt").write_text("victim\n", encoding="utf-8")
        git(self.victim, "add", "victim.txt")
        git(
            self.victim,
            "-c",
            "user.email=victim@example.invalid",
            "-c",
            "user.name=Victim",
            "commit",
            "-q",
            "-m",
            "victim base",
        )
        self.victim_state = self._state()

        self.hooks = self.temp / "poisoned-hooks"
        self.hooks.mkdir()
        self.hook_marker = self.temp / "poisoned-hook-ran"
        hook = self.hooks / "pre-commit"
        hook.write_text(
            "#!/bin/sh\nprintf 'hook ran\\n' > "
            + str(self.hook_marker)
            + "\nexit 0\n",
            encoding="utf-8",
        )
        hook.chmod(0o755)

        self.global_config = self.temp / "poisoned-gitconfig"
        self.global_config.write_text(
            "[user]\n"
            "    email = poisoned@example.invalid\n"
            "    name = Poisoned\n"
            "[commit]\n"
            "    gpgsign = true\n"
            "[tag]\n"
            "    gpgsign = true\n"
            "[gpg]\n"
            "    program = /nonexistent/gpg\n"
            "[core]\n"
            f"    hooksPath = {self.hooks}\n",
            encoding="utf-8",
        )

        self.poison = {
            "GIT_DIR": str(self.victim / ".git"),
            "GIT_WORK_TREE": str(self.victim),
            "GIT_INDEX_FILE": str(self.victim / ".git" / "index"),
            "GIT_OBJECT_DIRECTORY": str(self.victim / ".git" / "objects"),
            "GIT_ALTERNATE_OBJECT_DIRECTORIES": str(self.victim / ".git" / "objects"),
            "GIT_COMMON_DIR": str(self.victim / ".git"),
            "GIT_NAMESPACE": "poison",
            "GIT_CONFIG_GLOBAL": str(self.global_config),
            "GIT_CONFIG_COUNT": "1",
            "GIT_CONFIG_KEY_0": "core.hooksPath",
            "GIT_CONFIG_VALUE_0": str(self.hooks),
        }

    def tearDown(self) -> None:
        (
            oracle.CACHE,
            oracle.SOURCE_ROOT,
            oracle.MANIFEST,
            oracle.BARE_REPO,
            oracle.PINNED_COMMIT,
        ) = self.original
        shutil.rmtree(self.temp, ignore_errors=True)

    def _state(self) -> tuple:
        return (
            git(self.victim, "rev-parse", "HEAD"),
            git(self.victim, "symbolic-ref", "--short", "HEAD"),
            git(self.victim, "config", "user.email"),
            git(self.victim, "config", "user.name"),
            oracle.sha256_file(self.victim / ".git" / "index"),
            oracle.sha256_file(self.victim / ".git" / "config"),
        )

    def test_fixture_and_source_identity_ignore_poisoned_git_environment(self) -> None:
        with mock.patch.dict(os.environ, self.poison, clear=False):
            repo, commit = make_upstream_repo(self.temp)
            oracle.PINNED_COMMIT = commit

            # The fixture commit landed in the intended temporary repository.
            self.assertEqual(git(repo, "rev-parse", "HEAD"), commit)
            self.assertEqual(
                git(repo, "rev-parse", "--show-toplevel"), str(repo.resolve())
            )
            self.assertEqual(git(repo, "log", "-1", "--format=%s"), "fixture upstream")

            # Source identity is still validated, not bypassed, under poison.
            info = oracle.prepare_from_source(repo)
            self.assertEqual(info["upstream_commit"], commit)
            self.assertEqual(oracle.load_manifest(), info)

            # A real identity violation is still rejected under the same poison.
            (repo / "src" / "Second.java").write_text(
                "class Second {}\n", encoding="utf-8"
            )
            git(repo, "add", ".")
            git(
                repo,
                "-c",
                "user.email=oracle@example.invalid",
                "-c",
                "user.name=Oracle Test",
                "commit",
                "-q",
                "-m",
                "second",
            )
            with self.assertRaisesRegex(oracle.OracleError, "expected pinned commit"):
                oracle.prepare_from_source(repo)

        self.assertEqual(self._state(), self.victim_state)
        self.assertFalse(
            self.hook_marker.exists(),
            "a poisoned global or command-line hook executed",
        )


class ProtocolTests(unittest.TestCase):
    def descriptor(self) -> dict:
        return {
            "schema_version": 1,
            "suite": oracle.SUITE,
            "seed": oracle.SEED,
            "probe": oracle.PROBE_REL_PATH,
            "probe_sha256": "a" * 64,
            "case_count": 1,
            "cases": [{"id": "one", "dimension": 2}],
        }

    def trace(self) -> dict:
        return {
            "schema_version": 1,
            "upstream_commit": oracle.PINNED_COMMIT,
            "suite": oracle.SUITE,
            "seed": oracle.SEED,
            "cases": [{"id": "one", "values": [1.0, -2.0]}],
        }

    def validate(self, trace: dict | None = None, descriptor: dict | None = None) -> None:
        oracle.validate_trace(
            self.trace() if trace is None else trace,
            self.descriptor() if descriptor is None else descriptor,
            "a" * 64,
        )

    def test_exact_trace_is_accepted(self) -> None:
        self.validate()

    def test_empty_cases_are_rejected(self) -> None:
        trace = self.trace()
        trace["cases"] = []
        with self.assertRaisesRegex(oracle.OracleError, "nonempty"):
            self.validate(trace=trace)

    def test_missing_descriptor_case_is_rejected(self) -> None:
        descriptor = self.descriptor()
        descriptor["case_count"] = 2
        descriptor["cases"].append({"id": "two", "dimension": 1})
        with self.assertRaisesRegex(oracle.OracleError, r"missing=\['two'\]"):
            self.validate(descriptor=descriptor)

    def test_extra_case_is_rejected(self) -> None:
        trace = self.trace()
        trace["cases"].append({"id": "two", "values": [3.0]})
        with self.assertRaisesRegex(oracle.OracleError, r"extra=\['two'\]"):
            self.validate(trace=trace)

    def test_duplicate_case_is_rejected(self) -> None:
        trace = self.trace()
        trace["cases"].append({"id": "one", "values": [3.0]})
        with self.assertRaisesRegex(oracle.OracleError, "duplicate id"):
            self.validate(trace=trace)

    def test_dimension_mismatch_is_rejected(self) -> None:
        trace = self.trace()
        trace["cases"][0]["values"] = [1.0, 2.0, 3.0]
        with self.assertRaisesRegex(oracle.OracleError, "dimension 3"):
            self.validate(trace=trace)

    def test_nonfinite_value_is_rejected(self) -> None:
        trace = self.trace()
        trace["cases"][0]["values"] = [math.inf, 1.0]
        with self.assertRaisesRegex(oracle.OracleError, "must be finite"):
            self.validate(trace=trace)

    def test_unknown_trace_field_is_rejected(self) -> None:
        trace = self.trace()
        trace["note"] = "unexpected"
        with self.assertRaisesRegex(oracle.OracleError, r"extra=\['note'\]"):
            self.validate(trace=trace)

    def test_wrong_commit_is_rejected(self) -> None:
        trace = self.trace()
        trace["upstream_commit"] = "0" * 40
        with self.assertRaisesRegex(oracle.OracleError, "does not match pinned"):
            self.validate(trace=trace)

    def test_probe_digest_mismatch_is_rejected(self) -> None:
        with self.assertRaisesRegex(oracle.OracleError, "probe digest"):
            oracle.validate_trace(self.trace(), self.descriptor(), "b" * 64)

    def test_strict_json_rejects_duplicate_keys(self) -> None:
        with self.assertRaisesRegex(oracle.OracleError, "duplicate JSON key"):
            oracle.read_json_strict(b'{"a": 1, "a": 2}', "test")

    def test_strict_json_rejects_nonfinite_literals(self) -> None:
        with self.assertRaisesRegex(oracle.OracleError, "non-finite"):
            oracle.read_json_strict(b'{"a": NaN}', "test")


class CheckedFixtureTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.fixture_path = ROOT / "fixtures" / "primitives-v1.json"
        cls.provenance_path = ROOT / "fixtures" / "primitives-v1.provenance.json"
        cls.descriptor = oracle.load_descriptor()
        cls.fixture = oracle.read_json_strict(
            cls.fixture_path.read_bytes(), "committed fixture"
        )
        oracle.validate_trace(
            cls.fixture,
            cls.descriptor,
            oracle.sha256_file(oracle.PROBE),
        )
        cls.provenance = oracle.read_json_strict(
            cls.provenance_path.read_bytes(), "committed provenance"
        )

    def test_fixture_matches_checked_descriptor_and_probe(self) -> None:
        self.assertEqual(self.descriptor["case_count"], len(self.fixture["cases"]))
        self.assertEqual(self.descriptor["probe_sha256"], oracle.sha256_file(oracle.PROBE))

    def test_provenance_binds_fixture_bytes_and_checked_inputs(self) -> None:
        self.assertEqual(
            self.provenance["reference_sha256"],
            oracle.sha256_file(self.fixture_path),
        )
        self.assertEqual(
            self.provenance["suite_descriptor_sha256"],
            oracle.sha256_file(oracle.SUITE_PATH),
        )
        self.assertEqual(self.provenance["upstream_commit"], oracle.PINNED_COMMIT)
        self.assertEqual(self.provenance["suite"], oracle.SUITE)
        self.assertEqual(self.provenance["seed"], oracle.SEED)
        self.assertEqual(self.provenance["probe_sha256"], oracle.sha256_file(oracle.PROBE))
        self.assertTrue(self.provenance["source_clean"])

    def test_provenance_has_no_host_absolute_paths(self) -> None:
        def strings(value):
            if isinstance(value, str):
                yield value
            elif isinstance(value, list):
                for item in value:
                    yield from strings(item)
            elif isinstance(value, dict):
                for item in value.values():
                    yield from strings(item)

        for value in strings(self.provenance):
            self.assertFalse(
                value.startswith("/"),
                f"absolute path leaked into tracked provenance: {value}",
            )
            self.assertNotIn(str(ROOT), value)


if __name__ == "__main__":
    unittest.main()
