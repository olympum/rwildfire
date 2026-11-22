# Source provenance and attribution

rwildfire is an independent Rust port. See [LICENSE](LICENSE) for the GNU General Public License, version 3. The original rwildfire source is licensed under GPL-3.0-or-later.

The CPU sampling code in [render.rs](crates/cpu/src/render.rs) adapts JWildfire 9.20 at commit [2a26806eb7487ea60eb3b1a554c8a73120d54343](https://github.com/thargor6/JWildfire/tree/2a26806eb7487ea60eb3b1a554c8a73120d54343): the Marsaglia random generator, transform and variation equations, selection table, iteration, view projection, raster accumulation, log-density, gamma correction, and [fixed spatial filtering](crates/cpu/src/filter.rs). The original Java sources are Copyright © Andreas Maschke as noted in their source headers, and are licensed LGPL-2.1-or-later. The adapted Rust source is distributed under GPL-3.0-or-later under section 3 of the LGPL 2.1. It has been translated and limited to a seeded 2D CPU slice; it is not the original Java implementation.

The Java reference, its sample flames, and all upstream libraries and binaries stay external. Dependencies fetched by Cargo retain their own licenses. No third-party executables or built artifacts are included in this source snapshot.
