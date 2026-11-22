# CPU preview

    cargo run --release --locked -p rwildfire-cpu -- /path/to/JWildfire/resources/flames/TINA0010.flame /tmp/tina0010.png

The preview reads original flame XML directly. It supports legacy single-layer
2D scenes with an RGB palette, weighted XY transforms, linear3D and spherical3D.
It omits Java's production filter and adaptive antialiasing, and its output
shading is approximate. The output size is twice the source size.

The parity gate uses the original TINA0010 and TINA0004 from the validated
JWildfire 9.20 source export, not copies in this repository. To run the scene
check separately with JDK 8 and a clean checkout of the pinned revision:

    JAVA_HOME=/path/to/jdk8 scripts/check-cpu-scene /path/to/JWildfire

The probe uses Java model decoding, transform tables, point transforms, view
projection and raster accumulation. It compares decoded values, 256 points and
all occupied bins in that window per scene, without filtering or antialiasing.
Fresh traces and provenance are retained under ignored artifacts. The production
PNGs are visual comparisons, not goldens.
