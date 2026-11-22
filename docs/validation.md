# Validation contract

This page describes what the local gate proves, how to run it, and where its
evidence lives. The trace format and reference-maintenance rules remain owned
by [ADR 0001](adr/0001-parity-evidence.md) and the [oracle instructions](oracle.md).

## What a green gate means

`./scripts/check` passes only when all of the following were observed:

1. `cargo fmt --check` is clean.
2. `cargo clippy --workspace --all-targets --locked -- -D warnings` is clean.
3. `cargo test --workspace --locked` passes, its output is retained, and both
   existing integration targets (`tests/cli.rs` and `tests/library.rs`) report at
   least one passed test. No target may report a failed or ignored test.
4. `tools/tests` exists and `python3 -m unittest discover -s tools/tests -v`
   passes with a nonzero collected-test count and no skipped tests, including
   partial skips.
5. The real Java reference is prepared and captured twice in independent
   processes. Each capture covers the reviewed `fixtures/primitives-v1.suite.json`
   descriptor exactly: 11 cases and 46 numeric components for `primitives-v1`.
6. `cargo run --locked -p rwildfire-parity -- compare` passes for
   fresh-capture-to-fresh-capture and for committed-golden-to-fresh-capture,
   with a nonempty reported case and component count.
7. A copied candidate with exactly one changed numeric component makes the
   comparator exit 1, and the structured report names exactly the mutated case,
   component, expected value, and actual value.
8. The committed golden's SHA-256 is unchanged at the end of the gate.

A green gate establishes that the Java-to-Java instrument is repeatable, that
the reviewed golden still reproduces, and that the comparator reports a real
numeric difference rather than accepting everything. It does **not** establish
Rust renderer parity. The primitive harness was established before the CPU preview, and isolated
seeded probes do not establish whole-render or stochastic workflow
repeatability. Do not describe a green gate as renderer parity.

## Commands

Default: prepare the pinned reference in ignored `.cache/`, then capture.

```sh
./scripts/check
```

Use an existing clean checkout of the exact pinned revision instead:

```sh
JWILDFIRE_SOURCE=/path/to/JWildfire ./scripts/check
# or
./scripts/check --source /path/to/JWildfire
```

The oracle, not the shell gate, owns source identity. `prepare --source`
rejects a wrong `HEAD`, staged or untracked changes, and hidden index flags
(`assume-unchanged` / `skip-worktree`). The gate does not duplicate that
validator; it additionally requires each capture to carry the pinned
`upstream_commit`, the descriptor suite and seed, and the exact descriptor
case/dimension set.

`JAVA_HOME` selects the installed JDK 8. If it is unset, `java` and `javac`
must be JDK 8 on `PATH`. The gate fails if it finds anything else. It never
downloads a JDK and the oracle never downloads a runtime.

## Evidence and artifacts

Every gate run creates a new ignored directory
`artifacts/check.XXXXXXXX/` and prints it. Runs do not delete earlier evidence
and concurrent runs do not share capture paths. A failed run keeps its captures,
logs, reports, and mutation metadata for inspection.

The directory contains:

- `fresh-a.json`, `fresh-b.json`: two independent real-Java captures;
- `fresh-a.provenance.json`, `fresh-b.provenance.json`: runtime and source
  provenance written by the oracle;
- `report-fresh-fresh.json`, `report-golden-fresh.json`: passing comparator
  reports;
- `mutant.json`, `mutant.meta.json`, `report-mutant.json`: the explicit
  numeric mutant and the exit-1 report that must name it;
- `rust-tests.log`: the complete cargo test output used for execution checks;
- `python-tests.log`: the collected Python tooling tests.

The committed reference is `fixtures/primitives-v1.json`. Capture always writes
to `artifacts/`; reinstalling a fixture is a separate, reviewed change. Never
repair a failed comparison by replacing the golden. Follow the
[oracle instructions](oracle.md) for reference updates.

## Failure rules

A missing JDK 8, oracle, validator, goldens, suite descriptor, Python test suite,
empty capture, empty report, zero collected tests, an empty `cli` or `library`
integration target, any ignored Rust test, any skipped Python test, unexpected
exit code, or changing golden is a failure. It is never a skip and never a pass.
The gate rejects a numeric mutant that exits 0 (comparator missed a real
difference) or 2 (the mutation produced invalid input rather than a valid
difference).

The small `tools/verify_oracle.py` helper is not a second trace comparator. The
Rust comparator owns schema, finite values, metadata, IDs, and dimensions. The
helper adds only gate-scope assertions: capture/report coverage of the reviewed
descriptor, exact identity, the exact intended mutant, and nonempty passing
test-execution logs. Its `self-test` exercises those assertions with negative
controls before the real gate uses them.

## Comparator bypass proof

`scripts/falsify` is a separate, manual probe for the pass decision itself. It
copies `Cargo.toml`, `Cargo.lock`, and `crates/` into a temporary directory,
requires the unmodified copy's tests to pass with a nonzero count, forces the
comparator's pass decision to `true`, and requires the same test count to fail.
It restores the temporary copy from a byte backup and verifies that the shared
checkout's SHA-256 did not change. The shared source is never mutated, so it is
safe to run while another coder works on the crate.

```sh
./scripts/falsify
```

Its logs are kept in `artifacts/falsify.XXXXXXXX/`. Run it after changing the
comparator's pass logic and record the result in the pull request. It is not
part of `scripts/check` because it builds and tests a second tree.

## Hosted gate

`.github/workflows/ci.yml` runs the same `./scripts/check` on Linux with
Temurin JDK 8 and stable Rust. The initial hosted target is Linux only; other
platforms need their own observed evidence before support is claimed.

The workflow has `contents: read` only. It uses no repository secrets, no
deployment or publishing step, and no dependency cache. A weak cache key can
serve stale sources or toolchains while still reporting a green build, so each
job starts clean and the gate itself uses `--locked`. Official actions are
pinned by full commit SHA resolved from the upstream tags; the workflow records
the corresponding version in a comment.

The hosted runner's JDK is installed by the pinned `setup-java` action. The
oracle still receives `JAVA_HOME` and never fetches a runtime, and the default
gate explicitly prepares the pinned source rather than trusting an ambient
checkout.

## Reporting

Record a pull request's observed result, not an expectation. Include the
`scripts/check: PASS` line with its artifact directory and golden SHA-256, or
the failing command and its first actionable diagnostic. For comparator changes,
also include the `scripts/falsify` baseline and mutant result.
