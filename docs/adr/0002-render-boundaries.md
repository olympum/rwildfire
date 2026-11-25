# ADR 0002: Separate flame documents, render requests, and render jobs

Status: accepted for the CPU still-render boundary.

## Decision

- **Document and evaluation.** Keep the editable flame separate from a renderable scene. A future document reader/writer should retain ordered layers, transforms and variations, motion curves, and unsupported fields it can safely round-trip. Evaluation at an explicit frame or subframe should resolve defaults and report unsupported render features without mutating the document or depending on a previous frame. Match Java's curve evaluation and temporal sampling before claiming animation parity.
- **Request and job.** Frontends pass explicit settings to resolution. The current CPU slice takes dimensions, quality and seed, and reports the derived raster dimensions, border, sample budget, effective pixels per unit and filter radius. Each render starts fresh worker state, including RNG, variation state and accumulators. Encoding stays in the caller. Future desktop work should add cancellation, progress and preview delivery at coarse job boundaries, not per-sample UI callbacks.
- **Backend capabilities.** Share variation names, parameter semantics and ordering, not implementation-specific layouts. CPU and GPU may prepare different representations. Report unsupported features and make fallback observable to the caller; don't make the GPU consume the CPU sampler or its lookup tables.
- **Evidence.** Preserve exact seeded CPU tests. Compare a future animation evaluator at keyframes and between them before comparing output pixels. GPU and threaded execution need their own reference paths and repeatability measurements; CPU evidence cannot set their numerical tolerances.

## What ships now

The CPU library resolves an explicit still request against an immutable preview scene and returns row-major RGB8. The legacy CLI remains an adapter: it doubles the source size, uses its existing default quality and preview seed, and writes PNG. The ordinary budget cap and fixed quality-500 gate remain separate. The CPU request and resolved settings are not a persisted schema, a universal backend interface, or the editable document model.

The current XML parser remains the strict preview loader; no future round-trip, animation, job-control, or GPU feature is implemented by this change. The next proof is a reproducible whole render through pinned Java's production renderer ([#46](https://github.com/olympum/rwildfire/issues/46)). Before choosing UI and GPU stacks, test cancellation while scrubbing in a desktop preview and measure precision, accumulation and readback on the intended GPU hardware.
