# CPU preview

    cargo run --release --locked -p rwildfire-cpu -- /path/to/JWildfire/resources/flames/TINA0010.flame /tmp/tina0010.png

The preview reads original flame XML directly. It supports legacy single-layer
2D scenes with an RGB palette, camera roll, weighted XY transforms, and the
`linear3D`, `spherical3D`, and `waves` variations. For `waves` angles above
about 12,739 radians in magnitude, Rust uses its standard sine instead of
Java’s large-angle reduction. Small or zero affine translations can reach
that range. Mixed transforms with three active variations use a fixed Rust
accumulation order; parity for those combinations is unverified. The legacy XML `rotate` value is read in degrees, as in JWildfire.
The output size is twice the source size. The opaque-black renderer uses
JWildfire’s default Mitchell Smooth filter for color and Gaussian filter for
intensity at legacy `filter="1.0"`. Radius 0 or a missing `filter` attribute
keeps the prior unfiltered preview behavior; the missing-attribute case does
not reproduce Java’s preference default. Other radii, kernels, oversampling,
and adaptive antialiasing are not supported.

The parity gate uses the originals TINA0010, TINA0004, TINA0006, TINA0008, TINA0011 and TINA0021
from the validated JWildfire 9.20 source export, not copies in this repository. To run the scene
check separately with JDK 8 and a clean checkout of the pinned revision:

    JAVA_HOME=/path/to/jdk8 scripts/check-cpu-scene /path/to/JWildfire

The probe uses Java model decoding, transform tables, point transforms, view
projection and raster accumulation. It compares decoded values, 256 points and
all occupied bins in that window per scene, without filtering or antialiasing.
For radius-1 TINA0010, Java and Rust independently generate a seeded
1018×672 padded raster for a 1014×668 output image. Quality 100 sets a budget
of 68,409,600 iterations (100 per padded bin). The gate compares every bin’s
count and float sums bit-for-bit, then compares every decoded RGB pixel. It
rejects a modified bin and checks that bypassing the filter changes the image.
The unfiltered path still uses a shared raster and rejects a one-channel
mutation. Successful runs retain the rasters, images, differences and
provenance. This does not test the production thread scheduler or adaptive filtering.
