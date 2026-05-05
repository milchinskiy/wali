# Contributing

Thanks for your interest in wali.

wali is intentionally small. Contributions should preserve that direction:
simple primitives, predictable behavior, clear errors, and documentation that
matches the implementation.

## Before Opening a Pull Request

Please open an issue first for:

- new builtin modules;
- new manifest fields;
- executor behavior changes;
- state-file or cleanup changes;
- compatibility-breaking changes;
- large documentation rewrites.

Small fixes, typo fixes, test improvements, and clearly isolated bug fixes may
be opened directly as pull requests.

## Development Checks

Before submitting a pull request, run:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo metadata --locked --features release-binary --format-version 1
cargo test --locked
cargo package --locked --allow-dirty --no-verify
```

## Design Expectations

Contributions should follow these rules:

- prefer small, explicit code over broad abstractions;
- keep builtin modules primitive and reusable;
- keep domain-specific policy out of the core project;
- avoid hidden behavior and surprising defaults;
- make failure modes clear;
- update documentation and tests together with behavior changes;
- do not add dependencies unless they solve a real maintenance or correctness
  problem.

## Documentation

Documentation changes should keep the split clear:

- README.md is the short landing page;
- docs/cli.md describes command-line behavior;
- docs/manifest.md describes manifest authoring;
- docs/builtin-modules.md describes builtin modules;
- docs/module-developers.md describes custom module development;
- docs/development.md describes maintainer workflow.

## License

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in wali is licensed as:

- MIT, or
- Apache-2.0,

at your option.
