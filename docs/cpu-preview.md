# CPU preview

    cargo run --release --locked -p rwildfire-cpu -- /path/to/JWildfire/resources/flames/TINA0010.flame /tmp/tina0010.png

The preview reads original flame XML directly. It supports legacy single-layer
2D scenes with an RGB palette, camera roll, weighted XY transforms, linear3D
and spherical3D. The legacy XML `rotate` value is read in degrees, as in JWildfire.
The output size is twice the source size. The opaque-black renderer uses
JWildfire’s default Mitchell Smooth filter for color and Gaussian filter for
intensity at legacy `filter="1.0"`. Radius 0 or a missing `filter` attribute
keeps the prior unfiltered preview behavior; the missing-attribute case does
not reproduce Java’s preference default. Other radii, kernels, oversampling,
and adaptive antialiasing are not supported.

The parity gate uses the originals TINA0010, TINA0004, TINA0006 and TINA0008 from the validated
JWildfire 9.20 source export, not copies in this repository. To run the scene
check separately with JDK 8 and a clean checkout of the pinned revision:

    JAVA_HOME=/path/to/jdk8 scripts/check-cpu-scene /path/to/JWildfire

The probe uses Java model decoding, transform tables, point transforms, view
projection and raster accumulation. It compares decoded values, 256 points and
all occupied bins in that window per scene, without filtering or antialiasing.
The gate shades shared deterministic TINA0010 rasters (100 samples per output
pixel) with both implementations and compares every decoded 8-bit RGB pixel.
It covers both the original radius-1 filter on a padded raster and the unfiltered
path. The unfiltered check rejects a one-channel mutation; the filtered check
rejects bypassing the kernel on the same padded raster. Successful runs retain
the rasters, Java and Rust PNGs, difference PNGs, and provenance. This proves
fixed-filter shading, not sampling-scheduler or adaptive-filter parity.
