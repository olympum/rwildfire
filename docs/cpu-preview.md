# CPU preview

    cargo run --release --locked -p rwildfire-cpu -- /path/to/JWildfire/resources/flames/TINA0010.flame /tmp/tina0010.png

The preview reads original flame XML directly. It supports legacy single-layer
2D scenes with an RGB palette, camera roll, weighted XY transforms, linear3D
and spherical3D. The legacy XML `rotate` value is read in degrees, as in JWildfire.
The output size is twice the source size. It uses the unfiltered, opaque-black
Java density and gamma path with the default color settings. Production
filtering and adaptive antialiasing are not implemented.

The parity gate uses the originals TINA0010, TINA0004, TINA0006 and TINA0008 from the validated
JWildfire 9.20 source export, not copies in this repository. To run the scene
check separately with JDK 8 and a clean checkout of the pinned revision:

    JAVA_HOME=/path/to/jdk8 scripts/check-cpu-scene /path/to/JWildfire

The probe uses Java model decoding, transform tables, point transforms, view
projection and raster accumulation. It compares decoded values, 256 points and
all occupied bins in that window per scene, without filtering or antialiasing.
The same gate also shades a deterministic TINA0010 raster (100 samples per
pixel) with both implementations and compares every decoded 8-bit RGB pixel.
The artifact directory contains the raster, Java and Rust PNGs, absolute RGB
difference PNG, and provenance. A one-channel mutation must produce exactly
one mismatched pixel. This isolates shading; it does not establish production
filtering or sampling-scheduler parity.
