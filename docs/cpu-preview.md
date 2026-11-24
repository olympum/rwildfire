# CPU preview

    cargo run --release --locked -p rwildfire-cpu -- /path/to/JWildfire/resources/flames/TINA0010.flame /tmp/tina0010.png

The preview reads original flame XML directly. It supports legacy single-layer
2D scenes with an RGB palette, camera roll, weighted XY transforms, and the
`linear3D`, `spherical3D`, `waves`, `bubble`, `spiral`, `pdj`, `disc`, `hemisphere`, `swirl`, `rectangles`, `curl3D`, `heart`, `horseshoe`, `fan2`, `diamond`, and `popcorn` variations. Fan2 reads `fan2_x` and `fan2_y`, defaulting to `(0.5, 1.2)`. Curl3D reads `curl3D_cx`, `curl3D_cy`, and `curl3D_cz`, defaulting to `(0, 0.05, 0.05)`. Rectangles takes `rectangles_x` and `rectangles_y`, defaulting to `0.3` and `0.2`. The `pdj` preview reads `pdj_a` through `pdj_d`, defaulting to `(1, 2, 3, 4)`. The sampler carries Z between
transforms, but the supported camera still projects only XY. The sine, cosine, and tangent ports use the pinned Java angle reduction. The parity gate compares those functions against Java across small and large arguments, including near tangent poles. Supported variations accumulate in the order JWildfire loads them from
legacy XML. Priority overrides and indexed variation names remain unsupported. The legacy XML `rotate` value is read in degrees, as in JWildfire.
The output size is twice the source size. The opaque-black renderer uses
JWildfire’s default Mitchell Smooth filter for color and Gaussian filter for
intensity at legacy `filter="1.0"`. Radius 0 or a missing `filter` attribute
keeps the prior unfiltered preview behavior; the missing-attribute case does
not reproduce Java’s preference default. Other radii, kernels, oversampling,
and adaptive antialiasing are not supported.

The parity gate uses the originals TINA0010, TINA0004, TINA0006, TINA0008, TINA0011, TINA0021, TINA0009, TINA0018, TINA0013, TINA0016, TINA0003, TINA0001, TINA0007, TINA0005, TINA0015, TINA0023, TINA0025, and TINA0032
from the validated JWildfire 9.20 source export, not copies in this repository. To run the scene
check separately with JDK 8 and a clean checkout of the pinned revision:

    JAVA_HOME=/path/to/jdk8 scripts/check-cpu-scene /path/to/JWildfire

The probe uses Java model decoding, transform tables, point transforms, view
projection and raster accumulation. It compares decoded values, 256 points and
all occupied bins in that window per scene, without filtering or antialiasing.
A synthetic trace checks ordered variation accumulation, explicit PDJ, rectangles, and curl3D parameters, and the horseshoe, fan2 (default and explicit parameters), and diamond transforms.
For every scene listed above, Java and Rust independently generate seeded
padded rasters at the original quality with radius-1 filtering. The gate compares every bin’s
count and float sums bit-for-bit, then compares every decoded RGB pixel.
For each scene it rejects a modified bin and checks that bypassing the filter
changes the image. The unfiltered TINA0010 path still uses a shared raster
and rejects a one-channel mutation. Successful runs retain the rasters,
images, differences, and provenance. The probes use one seeded stream with
sampling antialiasing disabled; they do not test thread or RNG scheduling.
