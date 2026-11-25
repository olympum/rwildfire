# rwildfire

A parity-first Rust port of [JWildfire](https://github.com/thargor6/JWildfire). The goal is full behavioral and feature compatibility, delivered in measured slices. See the [CPU preview](docs/cpu-preview.md) for the first supported scenes.

The initial reference is JWildfire 9.20, commit `2a26806eb7487ea60eb3b1a554c8a73120d54343`. The first milestone is a falsifiable parity harness using real Java reference probes.

## Development

Prerequisites: Rust with Cargo, rustfmt and Clippy; a JDK 8 installation with `java` and `javac`; Python 3; and Git. Set `JAVA_HOME` to the JDK installation if it is not on `PATH`.

Run the complete local gate from the repository root:

```sh
./scripts/check
```

The gate builds the pinned Java reference, captures independent probe runs, compares them with the reviewed reference, and checks that deliberate corruption is rejected. It also runs the Rust formatting, lint, and test checks. Source downloads are confined to the ignored local cache.

For an existing clean checkout of the exact upstream revision:

```sh
JWILDFIRE_SOURCE=/path/to/JWildfire ./scripts/check
```

The comparator can also compare independently produced traces without Java or network access:

```sh
cargo run --locked -p rwildfire-parity -- compare reference.json candidate.json --json
```

A pass validates only the cases supplied. The initial Java-to-Java checks establish a repeatable instrument, not Rust rendering parity. See the [oracle instructions](docs/oracle.md) and [validation contract](docs/validation.md) before changing reference data.

## Roadmap

The [parity contract](docs/adr/0001-parity-evidence.md) describes how we’ll extend the port beyond the first CPU scenes. The [render boundaries ADR](docs/adr/0002-render-boundaries.md) separates still-render settings from the future editable flame document, timeline, and GPU implementation.

## Contributing

See the [contribution guide](CONTRIBUTING.md), [code of conduct](CODE_OF_CONDUCT.md), and [security policy](SECURITY.md).

## License and provenance

Original rwildfire code is licensed under GPL-3.0-or-later. Upstream JWildfire and third-party components retain their own notices and terms; this repository does not relicense them. The Java reference remains an external, pinned checkout rather than vendored source or binaries. See [source attribution and distribution scope](NOTICE.md). Binary or asset bundles need a separate licensing review before publication.
