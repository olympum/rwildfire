# ADR 0001: Pin the Java reference and build the instrument first

Status: accepted for the initial harness.

## Context

The target is full JWildfire behavior and feature parity in Rust. Similar images do not establish this. The current upstream CPU renderer has randomly initialized, cached RNGs and timing-dependent shared float raster accumulation. GPU backends use different arithmetic and cannot substitute for a CPU oracle. Java scripts, object-serialized side formats, desktop workflows, and third-party dependencies remain part of the programme.

## Decision

Pin `https://github.com/thargor6/JWildfire` at `2a26806eb7487ea60eb3b1a554c8a73120d54343` (release 9.20). Build and execute actual upstream Java classes without changing their implementation. Keep source and generated artifacts in ignored local cache directories. Record runtime identity with captures. Do not silently refresh the revision or reference data.

The first suite is `primitives-v1`: seeded Marsaglia RNG outputs and a small, explicitly enumerated set of deterministic and stochastic variation probes, including preserve-Z behavior. It validates the instrument, not a Rust renderer. Later suites cover the complete registry, XML/model semantics, transform ordering, intermediate rasters, final images, and workflows; they require separately versioned suite identities.

### Trace contract, version 1

A UTF-8 JSON document has `schema_version` (integer 1), `upstream_commit` (the full pinned SHA), `suite` (nonempty string), `seed` (unsigned integer), and `cases` (nonempty array). Each case has a unique nonempty `id` and a nonempty `values` array of finite JSON numbers. Runtime provenance is a sidecar, not numerical case data. Unknown fields are rejected to expose protocol drift.

A comparison validates both documents before comparing them. Schema, upstream revision, suite and seed must agree. Cases are matched by ID, independent of array order; the case sets and vector dimensions must match exactly. Empty, duplicate, missing, extra, malformed, or nonfinite input is an input error, never evidence of parity.

Numerical comparison defaults to exact finite f64 equality (signed zero compares equal). Optional finite nonnegative absolute and relative tolerances use `abs(expected-actual) <= max(abs_tol, rel_tol * max(abs(expected), abs(actual)))`; arithmetic overflow must not manufacture a pass. Reports identify each differing case/component and distinguish input errors from numerical mismatches. Decimal JSON parsing must preserve round-trip f64 values.

### Command contract

`rwildfire-parity compare REFERENCE CANDIDATE [--abs-tol N] [--rel-tol N] [--json]`

Help and version go to stdout and take precedence over other arguments, returning exit 0 without comparing inputs. These informational requests are the explicit exception to the comparison exit contract. Default comparison output is brief human-readable text; `--json` produces a stable structured report. Diagnostics go to stderr. For a comparison, exit 0 means a nonempty comparison passed, 1 means valid inputs differ, and 2 means invalid input, invocation, or I/O failure. Automation must invoke `compare` without help/version flags and validate the structured report, as the gate does. No prompts, telemetry, implicit downloads, or implicit reference replacement occur in the comparator. There is no user configuration or environment-variable precedence in this first version. Shell completions are not part of this developer-only milestone.

## Proof obligations

1. Independently execute the Java oracle twice and compare the captures.
2. Compare a fresh capture with the committed reference. Capture is explicit and reviewable; normal verification never rewrites the golden.
3. Change one numerical output and require a named mismatch with exit 1.
4. Remove cases; introduce duplicates, dimension/metadata mismatches, malformed/nonfinite values, and empty inputs; require failures.
5. Exercise tolerance boundaries and prove that a bypassed comparison fails the harness tests.
6. Run real Java verification in CI. Missing runtime or missing cases must fail, not skip.

A green instrument means it detects differences in this suite. It does not prove Rust rendering parity until independent Rust implementations produce candidate traces. Do not infer whole-render repeatability from isolated seeded probes.

## Consequences and sequencing

The harness precedes all renderer ports. Preserve upstream quirks in evidence before considering improvements. Semantic XML equality is distinct from byte equality, particularly for parent-curve names containing Java object hash codes. Deterministic numerical evidence is distinct from stochastic image/workflow evidence. Keep backend-specific criteria explicit.

Follow-on work includes model/XML, variations, CPU/animation, desktop tools, Java extensions, side formats, GPU, and distribution. No unsupported subsystem may disappear from the full-parity goal merely because the first suite does not exercise it.
