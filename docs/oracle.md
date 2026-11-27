# Java reference oracle: primitives-v1

This document owns the Java half of the parity harness. The Rust comparator and
its trace/report contract live on the Rust side. The oracle executes the real,
unmodified upstream JWildfire classes; it does not translate formulas into a
second implementation, and no mock variation stands in for upstream code.

## Pinned reference

- Repository: <https://github.com/thargor6/JWildfire>
- Commit: `2a26806eb7487ea60eb3b1a554c8a73120d54343` (release 9.20)
- Suite: `primitives-v1`
- Seed: `123456789`

The checkout is external. Nothing from upstream is vendored into this
repository. `prepare` fetches the pinned commit into an ignored bare cache by
default, or accepts an explicit clean checkout with `--source`.

## Production CPU stills: whole-render-v7

After preparing the pinned source, run `python3 tools/check_whole_render.py`
(or append the path to a clean pinned checkout). The standard gate also runs it.
The probe calls unmodified Java's `FlameRenderer.renderFlame` in production mode
and retains its padded float/int raster. It encodes the output to PNG and
compares decoded RGB, not PNG container bytes.

TINA0010 (linear) and TINA0012 (includes blur3D) run at 160×120, quality 20,
with the source filter radius and oversampling (1). The probe sets the average
axis scale for the requested dimensions and disables legacy density estimation
(which the source flames enable). It uses one worker and seeds the shared
Marsaglia factory entry `MARSAGLIA#0` before constructing the renderer. Every
capture has a fresh JVM and empty preference home. Each scene's two captures
must have identical bins and RGB; a third capture at quality 21 must have more
hits and a different raster. The budget reported here is calculated from the
padded dimensions and requested quality, not a worker instrumentation counter.
For oversample O, round quality × padded raster area / O, then round up to
a complete layer round. The output stays 160×120. The oversample-2 fixture
uses radius 1 with a 326×246 raster; oversample-3 uses radius 0 with a
480×360 raster. The radius-1 oversample-3 fixture uses a 490×370 raster
and exercises rounding up from a fractional sample budget. The 2×
`adaptive-mitchell` fixture also compares with a fixed Mitchell Smooth Java
control: the entire raster must stay identical while reconstructed RGB changes. `tone-controls`
compares the opaque-black output with nondefault contrast, vibrancy, and
low-density brightness. Three Java controls reset one setting at a time;
each must preserve the raw raster and change the decoded image. The
`transparent-alpha` fixture compares decoded RGBA, including clear and
semitransparent pixels. Resetting foreground opacity in Java must leave
the raster and RGB unchanged while changing alpha. The two
`solid-background` scenes compare RGB and RGBA with a colored backdrop.
Black controls must preserve the raster (and RGBA alpha) while changing RGB.

Artifacts, including PNGs, binary captures, per-process logs, and provenance with
input and output hashes, are kept under the reported ignored artifact directory.
Rust must match the Java padded bins and decoded RGB with legacy projection
jitter enabled (amount 0.25, radius 0.5) and disabled (amount 0). The legacy
preview API and CLI leave jitter off. This is controlled single-worker parity
for these fixtures, not reproducibility under Java's default worker and RNG settings.
The same comparisons also cover the repository-owned `chaos-transitions` fixture,
with nonuniform destination weights and an omitted (all-ones) row. The
`post-affine` fixture exercises an asymmetric XY matrix; `final-chain`
exercises ordered stochastic and nonlinear final transforms. `layer-rounds`
uses overlapping weighted layers with different palettes and densities to check
shared RNG, Java’s full-round budget and buffered raster addition order.

## Toolchain

A JDK 8 must be supplied by the caller. The oracle never downloads a JDK and
does not search package managers.

```sh
export JAVA_HOME=/path/to/jdk8
export PATH="$JAVA_HOME/bin:$PATH"
```

`JAVA_HOME` is preferred. If it is unset, `java` and `javac` are resolved from
`PATH`. Both must report version `1.8`; otherwise the command fails with exit
2. The observed runtime and compiler version strings are recorded in the
provenance sidecar.

## Commands

```sh
# Materialize and verify the pinned source in ignored .cache.
python3 tools/oracle.py prepare [--source /path/to/clean/JWildfire]

# Capture explicit evidence; never point verification at the committed fixture.
python3 tools/oracle.py capture --output artifacts/reference.json [--source /path/to/clean/JWildfire]
```

Exit status is `0` only after a complete, validated capture. Invocation,
toolchain, source, compilation, JVM, malformed-output, and protocol failures
return `2`. An empty probe stdout is a failure, never a pass.

The full local gate is:

```sh
./scripts/check [--source PATH]
```

See the [validation contract](validation.md) for the gate's exact steps,
artifacts, and failure rules. The gate prepares the oracle, captures twice into
a unique ignored `artifacts/check.XXXXXXXX` directory, and uses the Rust
comparator for fresh-to-fresh and committed-golden-to-fresh checks. Default
comparison is exact finite f64 equality per [ADR 0001](adr/0001-parity-evidence.md):
signed zero compares equal, cases match by ID independent of array order, and
the case sets and vector dimensions must match exactly.

A separate manual repeatability check may byte-compare two explicit captures;
this is stronger than the comparator contract and has held for the current
suite, but it is not the gate's acceptance criterion:

```sh
python3 tools/oracle.py prepare --source PATH
python3 tools/oracle.py capture --output artifacts/manual-a.json --source PATH
python3 tools/oracle.py capture --output artifacts/manual-b.json --source PATH
cmp artifacts/manual-a.json artifacts/manual-b.json
```

The committed fixture is not rewritten by verification.

Python orchestration tests:

```sh
python3 -m unittest discover -s tools/tests -v
```

## Source identity

`prepare` and `capture --source PATH` reject a checkout unless all of the
following hold:

- `HEAD` is the full pinned commit;
- `git status --porcelain` is empty, including untracked files (Git-ignored
  files are not source and are not part of the export);
- `git ls-files -v` contains only normal `H` entries, so
  assume-unchanged and skip-worktree flags cannot hide local edits.

After those checks, the source is exported with `git archive` from the commit
object into `.cache/source/<commit>/<digest-prefix>`. Java is compiled from
that export, never from the mutable worktree. The export contents are hashed
into `source_digest_sha256`; the commit tree object is recorded as
`source_tree`. A cached export is reused only when its recomputed digest
matches.

The default cache is an ignored bare repository at
`.cache/upstream/JWildfire.git`. A bare object store has no worktree whose
cleanliness could drift, so its provenance uses `source_check:
git-bare-object-v1`. No implicit source substitution occurs: a fetch failure
stops the command and tells the caller to pass `--source`.

Trust limit: this proves content identity relative to the local Git object
store and rejects ordinary and hidden worktree drift. It is not a signature and
does not defend against a locally tampered Git object store or Git binary.

## Build and execution

`capture` always compiles into a new unique temporary directory under ignored
`.cache/build`, invokes the JVM, and deletes the directory afterwards. No class
files from a previous capture are reused, so class-file freshness does not
depend on a stamp or timestamp.

Compilation uses:

```text
javac -encoding ISO-8859-1 -source 1.8 -target 1.8 -proc:none -nowarn \
      -cp <all pinned lib/*.jar> -sourcepath <export>/src -d <fresh>/classes \
      oracle/PrimitivesV1Probe.java
```

`javac` resolves referenced upstream source classes from the pinned export.
Probe output is a single JSON document on stdout. JVM diagnostics stay on
stderr.

The probe uses the upstream `MathLib` default, `FAST_MATH`. That choice is
recorded in provenance; the harness does not switch to `JAVA_MATH` for
convenience. Values are therefore tied to the recorded JDK 8 runtime and the
upstream fast-math implementation.

## Suite contract

The checked descriptor is `fixtures/primitives-v1.suite.json`. It pins the
probe path and digest, the exact case IDs, each case dimension, and the case
count. Capture refuses to write output when the probe case set is missing,
extra, duplicated, or dimensionally different. A probe change therefore cannot
silently rebaseline the fixture: the descriptor must change deliberately.

The suite intentionally contains 11 cases:

| Case | Values | Purpose |
| --- | --- | --- |
| `rng/marsaglia/seed-123456789/first-16` | 16 | Seeded upstream Marsaglia sequence |
| `variation/linear/preserve-z-false` | 3 | Deterministic, Z not preserved |
| `variation/linear/preserve-z-true` | 3 | Deterministic, Z preserved |
| `variation/sinusoidal/preserve-z-false` | 3 | Deterministic, transcendental math |
| `variation/sinusoidal/preserve-z-true` | 3 | Same, Z preserved |
| `variation/spherical/preserve-z-false` | 3 | Deterministic radial math |
| `variation/spherical/preserve-z-true` | 3 | Same, Z preserved |
| `variation/swirl/preserve-z-false` | 3 | Deterministic trig math |
| `variation/swirl/preserve-z-true` | 3 | Same, Z preserved |
| `variation/blur/preserve-z-false` | 3 | Stochastic, two context RNG draws |
| `variation/blur/preserve-z-true` | 3 | Same, Z preserved |

Every variation case starts a fresh `MarsagliaRandomGenerator` seeded with
`123456789`, creates a real `FlameTransformationContext(null, rng, 0, 0)`,
`Layer`, and `XForm`, calls `initOnce` and then `init`, and transforms the
fixed point:

```text
x = 0.3, y = -0.7, z = 0.45, amount = 1.25
```

Each case records the resulting `x`, `y`, `z`. The stochastic case consumes
RNG values through the context; a fresh seeded generator makes it repeatable.
No variation picks up ambient `java.lang.Math.random`.

The direct class list is deliberate: linear, sinusoidal, spherical, swirl, and
blur. The probe does not initialize the full variation registry, renderer, XML
model, or UI.

## Captures and provenance

Committed evidence:

- `fixtures/primitives-v1.json` — the numerical reference capture.
- `fixtures/primitives-v1.suite.json` — the checked case/dimension contract.
- `fixtures/primitives-v1.provenance.json` — runtime and source identity for
  the committed capture.

For output `artifacts/reference.json`, the sidecar is
`artifacts/reference.provenance.json`. The sidecar contains:

- full upstream commit, source URL, suite, seed;
- source cleanliness, check method, Git tree object, source export digest;
- checked probe path and SHA-256, checked descriptor path and SHA-256;
- Java version and full `java -version` lines, `javac` version;
- `math_library: FAST_MATH`, oracle protocol, build mode;
- `reference_sha256`, binding the sidecar to the exact capture bytes.

Tracked provenance contains no host absolute paths. The provenance is not a
signature; it is a reviewable record of what the local toolchain observed.

## Repeatability and rebaselining

Independent captures of the same suite, source, probe, descriptor, and JDK
have produced identical JSON bytes in the observed JDK 8 runs. The gate's
contract is the exact finite-f64 comparison described by ADR 0001, not byte
identity. `fixtures/primitives-v1.provenance.json` records the byte hash of the
committed fixture and the runtime that produced it.

To change the oracle deliberately:

1. Change the probe and/or checked descriptor in a reviewed commit.
2. Run `capture` to a scratch path, never over the fixture.
3. Inspect the complete numerical diff and the new provenance.
4. Copy the reviewed capture and sidecar into `fixtures/` in a separate,
   explicit change.
5. Re-run independent captures and the full gate.

Never rebaseline merely because a candidate differs. A mismatch is evidence
until source, suite, probe, and runtime identity are all accounted for.

## Known limitations

- Isolated probes validate the instrument; they do not establish whole-render,
  image, workflow, or feature parity.
- Five directly instantiated variations from one fixed point and one seed do
  not cover the full registry, parameter ranges, or transform ordering.
- Stochastic coverage is one blur probe per preserve-Z state. It proves seeded
  context RNG consumption for those calls, not statistical behavior.
- No XML/model semantics, parent curves, animation, desktop workflow, GPU
  backend, serialization, or third-party extension behavior is exercised.
- The probe does not run the renderer's XForm affine pipeline or thread
  scheduling; it calls the variation lifecycle directly with real upstream
  objects.
- Numerical values can depend on JDK and upstream `FAST_MATH`; provenance
  records the observed toolchain rather than promising cross-JDK identity.
- The source check verifies a clean Git worktree and hidden index flags, then
  archives the commit. It does not prove the Git binary or local object store
  is uncompromised.
- Symlinks and hardlinks in the upstream archive are rejected rather than
  extracted; the pinned tree currently contains neither.
