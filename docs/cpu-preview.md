# CPU preview

    cargo run --release --locked -p rwildfire-cpu -- /path/to/JWildfire/resources/flames/TINA0010.flame /tmp/tina0010.png

The preview reads legacy flame XML and a subset of JWildfire’s layered `jwf-flame`
format. It supports RGB palettes, camera roll, weighted XY transforms, and the
`linear3D`, `spherical3D`, `waves`, `bubble`, `spiral`, `pdj`, `disc`, `hemisphere`, `swirl`, `rectangles`, `curl3D`, `heart`, `horseshoe`, `fan2`, `diamond`, `popcorn`, `blur`, `blur3D`, and `julia3D` variations. Fan2 reads `fan2_x` and `fan2_y`, defaulting to `(0.5, 1.2)`. Curl3D reads `curl3D_cx`, `curl3D_cy`, and `curl3D_cz`, defaulting to `(0, 0.05, 0.05)`. Rectangles takes `rectangles_x` and `rectangles_y`, defaulting to `0.3` and `0.2`. The `pdj` preview reads `pdj_a` through `pdj_d`, defaulting to `(1, 2, 3, 4)`. The sampler carries Z between
transforms. The camera supports pitch, yaw, roll, and perspective, including near-plane clipping before pixel mapping. Legacy pitch and yaw are radians in XML; `rotate` is in degrees. Camera bank, translation, depth of field, and solid rendering remain unsupported. The sine, cosine, and tangent ports use the pinned Java angle reduction. The parity gate compares those functions against Java across small and large arguments, including near tangent poles. Supported variations accumulate in the order JWildfire loads them from
legacy XML. An explicit zero amount is still evaluated: a zero `blur` consumes
random draws, and a zero `curl3D` at a singularity can produce NaN. The gate
checks the singularity before projection, not as an image. The preview rejects
`pdj_a` through `pdj_d` without a `pdj` variation on the same transform;
JWildfire ignores those orphan attributes. Priority overrides and indexed variation names remain unsupported. The legacy XML `rotate` value is read in degrees, as in JWildfire.
`julia3D` requires a nonzero explicit `julia3D_power`. The original TINA0014,
TINA0019, and TINA0024 omit it, so the gate makes temporary copies pinning
powers 2, −3, and 5 respectively. Java normally initializes omitted power
with `Math.random()`, outside the seeded sampler. The comparisons establish
parity at those explicit powers, not repeatability of unmodified Java loads.
The gate records the original and modified input hashes.

The `chaos` attribute takes one nonnegative multiplier per ordinary transform,
in XML order. The previous transform’s row multiplies the destination weights
before selection. Omitting a row leaves all multipliers at one. An explicit
row must have a finite positive effective total; partial and zero-total rows
are unsupported.

Ordinary transforms may specify six finite XY `post` coefficients in the
same order as `coefs`. They act on the accumulated variation output before
projection and before the point feeds the next iteration; Z is unchanged.
JWildfire skips the entire post stage when every coefficient is within
`1e-8` of identity. Other affine planes are unsupported. Ordered `finalxform`
elements run on a plotting copy after each ordinary sample, never during
warmup or as orbit feedback. They inherit the ordinary point’s RGB color
and use the same RNG stream. This preview does not support explicit final
color modes or chaos on finals.

A `jwf-flame` may contain one to 16 visible layers in XML order, each with its
own palette and ordinary and final transforms. Layer weight scales RGB, while
density below one gates attempts with probability density squared. Both default
to one and must be in `0..=1`. The seeded worker shares its RNG across layers,
completes whole layer rounds, and flushes per-layer batches into a single raster
in Java production order. Hidden layers are not supported. The `Sampler` trace
API traces the first layer in isolation; use a render request for the full scene.

The output size is twice the source size. The opaque-black renderer uses
JWildfire’s default Mitchell Smooth filter for color and Gaussian filter for
intensity at legacy `filter="1.0"`. Radius 0 or a missing `filter` attribute
keeps the prior unfiltered preview behavior; the missing-attribute case does
not reproduce Java’s preference default. Spatial `oversample` accepts integers
1 through 3 (default 1). It changes the subpixel raster, not the output image
size. With filtering disabled, reconstruction sums the tone-scaled subpixels;
with filtering enabled, the kernels operate on the subpixel grid. Other radii,
kernels and filter-indicator overlays are not supported. At radius 1,
`filter_kernel="MITCHELL_SINEPOW"` enables adaptive reconstruction from the same padded
raster. It accepts `filter_type="ADAPTIVE"` (or omitted), `filter_sharpness`
(default 4.0), and `filter_low_density` (default 0.025). It selects SinePow10
for sparse or smooth areas and Mitchell Smooth for detail; empty selector bins
use the primary filter. This does not change projection jitter. The opaque-black path accepts `contrast`
(positive, default 1), `low_density_brightness` (nonnegative, default 0.24),
and `vibrancy` (default 1, quantized and clamped by Java’s 256-step rule).
These controls do not change the raw raster.

The parity gate uses the originals TINA0010, TINA0004, TINA0006, TINA0008, TINA0011, TINA0021, TINA0009, TINA0018, TINA0013, TINA0016, TINA0003, TINA0001, TINA0007, TINA0005, TINA0015, TINA0023, TINA0025, TINA0032, TINA0002, TINA0012, TINA0017, TINA0020, TINA0022, and TINA0026
from the validated JWildfire 9.20 source export, not copies in this repository. To run the scene
check separately with JDK 8 and a clean checkout of the pinned revision:

    JAVA_HOME=/path/to/jdk8 scripts/check-cpu-scene /path/to/JWildfire

The probe uses Java model decoding, transform tables, point transforms, view
projection and raster accumulation. It compares decoded values, 256 points and
all occupied bins in that window per scene, without filtering or antialiasing.
The synthetic trace checks variation order, explicit parameters, and independent rolling random state for two `blur3D` transforms. Each `blur3D` initializes four draws from the seeded stream in xform order, before the initial point is drawn. Matching the seeded blur probes does not establish repeatability of the default Java renderer.
For every scene listed above, Java and Rust independently generate seeded
padded rasters at the original quality with radius-1 filtering. The gate compares every bin’s
count and float sums bit-for-bit, then compares every decoded RGB pixel.
For each scene it rejects a modified bin and checks that bypassing the filter
changes the image. The unfiltered TINA0010 path still uses a shared raster
and rejects a one-channel mutation. Successful runs retain the rasters,
images, differences, and provenance. The probes use one seeded stream with
sampling antialiasing disabled; they do not test thread or RNG scheduling.

## Quality-500 parity

    JAVA_HOME=/path/to/jdk8 scripts/check-cpu-scene --quality-500 /path/to/JWildfire

This opt-in mode replaces the standard image comparisons with TINA0027–0031 and
TINA0033. Each uses 311,688,000 samples at the original quality of 500, with a
936×666 padded raster and 932×662 output. The ordinary preview retains its
200,000,000-iteration cap; the high-budget path is fixed to this layout and quality.
The gate compares every padded bin and RGB pixel and checks that the ordinary cap
rejects the same request. Push and PR CI keep the standard gate.
