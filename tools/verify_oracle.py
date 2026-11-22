#!/usr/bin/env python3
"""Gate-local evidence checks for the parity harness.

The Rust comparator owns trace schema, finite values, metadata, IDs, and
dimensions. This helper adds only what the comparator cannot know:

* a capture and a report cover the reviewed suite descriptor exactly;
* a numeric mutant names exactly the case, component, and values changed;
* mutant generation writes a copied capture and never touches the golden.
"""

from __future__ import annotations

import argparse
import copy
import json
import math
import re
import sys
from pathlib import Path
from typing import Any

SCHEMA_VERSION = 1


class Invalid(Exception):
    """A document is not valid evidence for this gate."""


def _constant(token: str) -> None:
    raise Invalid(f"non-finite JSON constant {token!r} is not allowed")


def _int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def _finite(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def _req(document: Any, fields: tuple[str, ...], label: str) -> dict[str, Any]:
    if not isinstance(document, dict):
        raise Invalid(f"{label} must be a JSON object")
    missing = [field for field in fields if field not in document]
    if missing:
        raise Invalid(f"{label} is missing {missing}")
    return document


def load_json(path: Path) -> Any:
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as error:
        raise Invalid(f"cannot read {path}: {error}") from error
    try:
        return json.loads(text, parse_constant=_constant)
    except Invalid:
        raise
    except json.JSONDecodeError as error:
        raise Invalid(f"invalid JSON in {path}: {error}") from error


def load_descriptor(path: Path, expected_suite: str) -> dict[str, Any]:
    raw = _req(
        load_json(path),
        ("schema_version", "suite", "seed", "case_count", "cases"),
        "suite descriptor",
    )
    if raw["schema_version"] != SCHEMA_VERSION:
        raise Invalid(f"suite descriptor schema_version is not {SCHEMA_VERSION}")
    if raw["suite"] != expected_suite:
        raise Invalid(f"suite descriptor suite {raw['suite']!r} is not {expected_suite!r}")
    if not _int(raw["seed"]) or raw["seed"] < 0:
        raise Invalid("suite descriptor seed must be a nonnegative integer")
    if not isinstance(raw["cases"], list) or not raw["cases"]:
        raise Invalid("suite descriptor cases must be a nonempty array")
    if raw["case_count"] != len(raw["cases"]):
        raise Invalid("suite descriptor case_count does not match its cases")

    dimensions: dict[str, int] = {}
    for index, case in enumerate(raw["cases"]):
        case = _req(case, ("id", "dimension"), f"suite descriptor case {index}")
        case_id, dimension = case["id"], case["dimension"]
        if not isinstance(case_id, str) or not case_id or case_id in dimensions:
            raise Invalid(f"suite descriptor case {index} has an empty or duplicate id")
        if not _int(dimension) or dimension <= 0:
            raise Invalid(f"suite descriptor case {case_id!r} dimension must be positive")
        dimensions[case_id] = dimension
    return {
        "suite": raw["suite"],
        "seed": raw["seed"],
        "dimensions": dimensions,
        "case_count": len(dimensions),
        "component_count": sum(dimensions.values()),
    }


def _identity(document: dict[str, Any], descriptor: dict[str, Any], commit: str, label: str) -> None:
    if document["upstream_commit"] != commit:
        raise Invalid(f"{label} upstream_commit does not match pinned {commit}")
    if document["suite"] != descriptor["suite"] or document["seed"] != descriptor["seed"]:
        raise Invalid(f"{label} suite/seed does not match the suite descriptor")


def check_capture(document: Any, descriptor: dict[str, Any], commit: str) -> None:
    """Check identity and exact case/dimension scope; the comparator checks values."""
    document = _req(
        document,
        ("schema_version", "upstream_commit", "suite", "seed", "cases"),
        "capture",
    )
    if document["schema_version"] != SCHEMA_VERSION:
        raise Invalid(f"capture schema_version is not {SCHEMA_VERSION}")
    _identity(document, descriptor, commit, "capture")
    if not isinstance(document["cases"], list) or not document["cases"]:
        raise Invalid("capture cases must be a nonempty array")

    actual: dict[str, int] = {}
    for index, case in enumerate(document["cases"]):
        case = _req(case, ("id", "values"), f"capture case {index}")
        case_id, values = case["id"], case["values"]
        if not isinstance(case_id, str) or not case_id or case_id in actual:
            raise Invalid(f"capture case {index} has an empty or duplicate id")
        if not isinstance(values, list):
            raise Invalid(f"capture case {case_id!r} values must be an array")
        actual[case_id] = len(values)

    if actual != descriptor["dimensions"]:
        missing = sorted(descriptor["dimensions"].keys() - actual.keys())
        extra = sorted(actual.keys() - descriptor["dimensions"].keys())
        wrong = sorted(
            case_id
            for case_id in actual.keys() & descriptor["dimensions"].keys()
            if actual[case_id] != descriptor["dimensions"][case_id]
        )
        raise Invalid(
            f"capture scope differs from suite descriptor: missing={missing} "
            f"extra={extra} wrong_dimensions={wrong}"
        )


def _report(document: Any, descriptor: dict[str, Any], commit: str) -> tuple[dict[str, Any], list]:
    document = _req(
        document,
        ("schema_version", "upstream_commit", "suite", "seed", "summary", "mismatches"),
        "report",
    )
    if document["schema_version"] != SCHEMA_VERSION:
        raise Invalid(f"report schema_version is not {SCHEMA_VERSION}")
    _identity(document, descriptor, commit, "report")
    summary = _req(
        document["summary"],
        ("cases", "components", "mismatched_cases", "mismatched_components", "passed"),
        "report summary",
    )
    if summary["cases"] != descriptor["case_count"]:
        raise Invalid(
            f"report compared {summary['cases']} cases, descriptor has "
            f"{descriptor['case_count']}"
        )
    if summary["components"] != descriptor["component_count"]:
        raise Invalid(
            f"report compared {summary['components']} components, descriptor has "
            f"{descriptor['component_count']}"
        )
    if not isinstance(summary["passed"], bool) or not isinstance(document["mismatches"], list):
        raise Invalid("report passed/mismatches fields are malformed")
    return summary, document["mismatches"]


def check_pass_report(document: Any, descriptor: dict[str, Any], commit: str) -> None:
    summary, mismatches = _report(document, descriptor, commit)
    if (
        summary["passed"] is not True
        or summary["mismatched_cases"] != 0
        or summary["mismatched_components"] != 0
        or mismatches
    ):
        raise Invalid("report is not a clean full-suite pass")


def check_fail_report(
    document: Any, descriptor: dict[str, Any], commit: str, mutation: dict[str, Any]
) -> None:
    summary, mismatches = _report(document, descriptor, commit)
    if (
        summary["passed"] is not False
        or summary["mismatched_cases"] != 1
        or summary["mismatched_components"] != 1
        or len(mismatches) != 1
    ):
        raise Invalid("numeric-mutant report must name exactly one mismatched case/component")
    mismatch = _req(mismatches[0], ("id", "components"), "mismatch")
    if mismatch["id"] != mutation["case_id"]:
        raise Invalid(f"mismatch is for {mismatch['id']!r}, not mutated {mutation['case_id']!r}")
    components = mismatch["components"]
    if not isinstance(components, list) or len(components) != 1:
        raise Invalid("numeric-mutant report must name exactly one component")
    component = _req(components[0], ("index", "expected", "actual"), "mismatch component")
    for field in ("index", "expected", "actual"):
        if component[field] != mutation[field]:
            raise Invalid(
                f"mismatch {field} is {component[field]!r}, expected mutated "
                f"{mutation[field]!r}"
            )


def mutate_capture(
    document: Any, descriptor: dict[str, Any], commit: str
) -> tuple[Any, dict[str, Any]]:
    check_capture(document, descriptor, commit)
    mutated = copy.deepcopy(document)
    case_id = next(iter(descriptor["dimensions"]))
    case = next(case for case in mutated["cases"] if case["id"] == case_id)
    value = case["values"][0]
    if not _finite(value):
        raise Invalid(f"capture case {case_id!r} value 0 is not finite")
    for candidate in (
        value + 1.0,
        value * 2.0 + 1.0,
        0.0 if value != 0.0 else 1.0,
        -value if value != 0.0 else -1.0,
    ):
        if math.isfinite(candidate) and candidate != value:
            case["values"][0] = candidate
            return mutated, {
                "case_id": case_id,
                "index": 0,
                "expected": float(value),
                "actual": float(candidate),
            }
    raise Invalid(f"no finite mutation found for case {case_id!r} value 0")


CARGO_RESULT_RE = re.compile(
    r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;"
)
RUNNING_RE = re.compile(r"^\s*Running (.+?) \(")
PY_RAN_RE = re.compile(r"^Ran (\d+) tests? in ")
PY_SKIPPED_RE = re.compile(r"skipped=(\d+)")


def load_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as error:
        raise Invalid(f"cannot read {path}: {error}") from error


def check_cargo_log(text: str, path: Path) -> int:
    """Require passing, nonempty cli and library integration test execution."""
    target = None
    results: list[tuple[str, int]] = []
    for line_number, line in enumerate(text.splitlines(), 1):
        stripped = line.strip()
        if stripped.startswith("Doc-tests "):
            target = "doc"
            continue
        running = RUNNING_RE.match(line)
        if running:
            description = running.group(1)
            target = description if description.startswith("tests/") else "unit"
            continue
        if not stripped.startswith("test result:"):
            continue
        result = CARGO_RESULT_RE.match(stripped)
        if result is None:
            raise Invalid(f"{path}:{line_number}: unrecognized cargo test result line")
        status, passed, failed, ignored = (
            result.group(1), int(result.group(2)), int(result.group(3)), int(result.group(4))
        )
        label = target or "unknown"
        if status != "ok" or failed > 0:
            raise Invalid(f"{path}: target {label} reported {status} with {failed} failed tests")
        if ignored > 0:
            raise Invalid(
                f"{path}: target {label} ignored {ignored} tests; ignored tests are not evidence"
            )
        results.append((label, passed))

    if not results:
        raise Invalid(f"{path}: no cargo test result lines; cargo test did not execute")
    by_target: dict[str, int] = {}
    for label, passed in results:
        by_target[label] = by_target.get(label, 0) + passed
    for required in ("tests/cli.rs", "tests/library.rs"):
        if required not in by_target:
            raise Invalid(
                f"{path}: no result for integration target {required}; tests were deleted or not collected"
            )
        if by_target[required] < 1:
            raise Invalid(
                f"{path}: integration target {required} passed 0 tests; empty execution is not evidence"
            )
    total = sum(passed for _, passed in results)
    if total < 1:
        raise Invalid(f"{path}: cargo test passed zero tests in every target")
    return total


def check_python_log(text: str, path: Path) -> int:
    """Require a nonempty Python run with no skipped tests."""
    ran = None
    ok = False
    for line in text.splitlines():
        match = PY_RAN_RE.match(line)
        if match:
            ran = int(match.group(1))
        if line == "OK" or line.startswith("OK ("):
            ok = True
        if line.startswith("FAILED"):
            raise Invalid(f"{path}: unittest reported FAILED")
        for skip in PY_SKIPPED_RE.finditer(line):
            if int(skip.group(1)) > 0:
                raise Invalid(
                    f"{path}: unittest skipped={skip.group(1)}; skipped tests are not passing evidence"
                )
    if ran is None or ran < 1:
        raise Invalid(f"{path}: no nonempty 'Ran N tests' line; the suite did not execute")
    if not ok:
        raise Invalid(f"{path}: unittest did not report an OK summary")
    return ran


def _write_json(path: Path, document: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(document, allow_nan=False, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )


def self_test() -> None:
    descriptor = {
        "suite": "primitives-v1",
        "seed": 7,
        "dimensions": {"a": 2, "b": 1},
        "case_count": 2,
        "component_count": 3,
    }
    commit = "2a26806eb7487ea60eb3b1a554c8a73120d54343"
    capture = {
        "schema_version": 1,
        "upstream_commit": commit,
        "suite": "primitives-v1",
        "seed": 7,
        "cases": [{"id": "a", "values": [1.0, 2.0]}, {"id": "b", "values": [3.0]}],
    }
    check_capture(capture, descriptor, commit)
    passing = {
        "schema_version": 1,
        "upstream_commit": commit,
        "suite": "primitives-v1",
        "seed": 7,
        "summary": {
            "cases": 2,
            "components": 3,
            "mismatched_cases": 0,
            "mismatched_components": 0,
            "passed": True,
        },
        "mismatches": [],
    }
    check_pass_report(passing, descriptor, commit)
    mutated, mutation = mutate_capture(capture, descriptor, commit)
    if mutated == capture or mutation["expected"] == mutation["actual"]:
        raise Invalid("self-test mutation did not change a value")
    failing = {
        **passing,
        "summary": {**passing["summary"], "mismatched_cases": 1,
                    "mismatched_components": 1, "passed": False},
        "mismatches": [{"id": mutation["case_id"], "components": [{
            "index": mutation["index"], "expected": mutation["expected"],
            "actual": mutation["actual"]}]}],
    }
    check_fail_report(failing, descriptor, commit, mutation)

    cargo_ok = (
        "     Running unittests src/lib.rs (target/debug/deps/lib)\n"
        "running 0 tests\n"
        "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n"
        "     Running tests/cli.rs (target/debug/deps/cli)\n"
        "running 2 tests\n"
        "test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n"
        "     Running tests/library.rs (target/debug/deps/library)\n"
        "running 3 tests\n"
        "test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n"
    )
    cargo_deleted = (
        "     Running unittests src/lib.rs (target/debug/deps/lib)\n"
        "running 0 tests\n"
        "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n"
        "   Doc-tests rwildfire_parity\n"
        "running 0 tests\n"
        "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n"
    )
    cargo_ignored = (
        "     Running tests/cli.rs (target/debug/deps/cli)\n"
        "running 2 tests\n"
        "test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out\n"
        "     Running tests/library.rs (target/debug/deps/library)\n"
        "running 3 tests\n"
        "test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n"
    )
    cargo_empty_cli = (
        "     Running tests/cli.rs (target/debug/deps/cli)\n"
        "running 0 tests\n"
        "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n"
        "     Running tests/library.rs (target/debug/deps/library)\n"
        "running 3 tests\n"
        "test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n"
    )
    python_ok = "test_a ... ok\n\nRan 2 tests in 0.001s\n\nOK\n"
    python_skipped = (
        "test_a ... ok\ntest_b ... skipped 'r3-02'\n\n"
        "Ran 2 tests in 0.001s\n\nOK (skipped=1)\n"
    )
    python_zero = "Ran 0 tests in 0.000s\n\nOK\n"
    if check_cargo_log(cargo_ok, Path("cargo.log")) != 5:
        raise Invalid("self-test cargo log did not count five passing tests")
    if check_python_log(python_ok, Path("python.log")) != 2:
        raise Invalid("self-test Python log did not count two tests")

    counts = {"capture": 0, "report": 0, "logs": 0}

    def rejects(category: str, call, *args) -> None:
        try:
            call(*args)
        except Invalid:
            counts[category] += 1
            return
        raise AssertionError(f"validator accepted a negative control: {call.__name__} {args!r}")

    rejects("capture", check_capture, {**capture, "cases": [capture["cases"][0]]},
            descriptor, commit)
    rejects("capture", check_capture, {**capture, "cases": [
        {"id": "a", "values": [1.0]}, {"id": "b", "values": [3.0]}]}, descriptor, commit)
    rejects("report", check_pass_report, {**passing, "mismatches": failing["mismatches"]},
            descriptor, commit)
    rejects("report", check_pass_report, failing, descriptor, commit)
    rejects("report", check_fail_report, passing, descriptor, commit, mutation)
    rejects("report", check_fail_report, {**failing,
            "summary": {**failing["summary"], "cases": 0}}, descriptor, commit, mutation)
    rejects("report", check_fail_report, {**failing, "mismatches": [{
        "id": "b", "components": failing["mismatches"][0]["components"]}]},
            descriptor, commit, mutation)
    rejects("report", check_fail_report, {**failing, "mismatches": [{"id": mutation["case_id"],
        "components": [{**failing["mismatches"][0]["components"][0],
                        "actual": mutation["expected"]}]}]}, descriptor, commit, mutation)
    rejects("logs", check_cargo_log, cargo_deleted, Path("cargo.log"))
    rejects("logs", check_cargo_log, cargo_ignored, Path("cargo.log"))
    rejects("logs", check_cargo_log, cargo_empty_cli, Path("cargo.log"))
    rejects("logs", check_python_log, python_skipped, Path("python.log"))
    rejects("logs", check_python_log, python_zero, Path("python.log"))
    print(f"verify_oracle self-test: OK ({counts['capture']} capture-scope, "
          f"{counts['report']} report/mutant, {counts['logs']} test-log negatives, "
          "1 mutation control)")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)

    check_desc = commands.add_parser("check-descriptor")
    check_desc.add_argument("path", type=Path)
    check_desc.add_argument("--suite", required=True)

    test_logs = commands.add_parser("check-test-logs")
    test_logs.add_argument("--cargo", type=Path, required=True)
    test_logs.add_argument("--python", type=Path, required=True)

    for name in ("check-capture", "assert-pass-report", "mutate-capture", "assert-fail-report"):
        command = commands.add_parser(name)
        command.add_argument("path" if name != "mutate-capture" else "input", type=Path)
        command.add_argument("--descriptor", type=Path, required=True)
        command.add_argument("--upstream-commit", required=True)
        if name == "mutate-capture":
            command.add_argument("output", type=Path)
            command.add_argument("metadata", type=Path)
        if name == "assert-fail-report":
            command.add_argument("--mutation", type=Path, required=True)

    commands.add_parser("self-test")
    args = parser.parse_args(argv)

    try:
        if args.command == "self-test":
            self_test()
            return 0
        if args.command == "check-test-logs":
            cargo_count = check_cargo_log(load_text(args.cargo), args.cargo)
            python_count = check_python_log(load_text(args.python), args.python)
            print(f"test logs {args.cargo} / {args.python}: cargo_passed={cargo_count} "
                  f"(cli/library nonempty, 0 ignored), python_ran={python_count} (0 skipped)")
            return 0
        if args.command == "check-descriptor":
            descriptor = load_descriptor(args.path, args.suite)
            print(f"descriptor {args.path}: suite={descriptor['suite']} "
                  f"cases={descriptor['case_count']} components={descriptor['component_count']}")
            return 0
        descriptor = load_descriptor(args.descriptor, "primitives-v1")
        if args.command == "check-capture":
            check_capture(load_json(args.path), descriptor, args.upstream_commit)
        elif args.command == "assert-pass-report":
            check_pass_report(load_json(args.path), descriptor, args.upstream_commit)
        elif args.command == "mutate-capture":
            mutated, mutation = mutate_capture(
                load_json(args.input), descriptor, args.upstream_commit
            )
            _write_json(args.output, mutated)
            _write_json(args.metadata, mutation)
            print(f"mutated {args.input} -> {args.output}: case {mutation['case_id']!r} "
                  f"component {mutation['index']}: {mutation['expected']!r} -> "
                  f"{mutation['actual']!r}")
        else:
            mutation = _req(load_json(args.mutation),
                            ("case_id", "index", "expected", "actual"), "mutation metadata")
            check_fail_report(load_json(args.path), descriptor, args.upstream_commit, mutation)
            print(f"report {args.path}: FAIL names exactly mutated case "
                  f"{mutation['case_id']!r} component {mutation['index']}")
        print(f"{args.command}: suite={descriptor['suite']} cases={descriptor['case_count']} "
              f"components={descriptor['component_count']}")
    except Invalid as error:
        print(f"verify_oracle: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
