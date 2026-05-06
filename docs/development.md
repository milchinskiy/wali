# Development notes

This document is for wali maintainers. User-facing behavior belongs in the
README and the focused files under `docs/`.

## Local environment

The repository can be built with the Rust toolchain directly:

```sh
cargo build
cargo test
```

The minimum supported Rust version is declared in `Cargo.toml`.

A Nix development shell is also available:

```sh
nix develop -c $SHELL
```

The shell includes the Rust toolchain, rustfmt, Clippy, Git, Perl, make,
pkg-config, and OpenSSL development inputs.

## Checks before submitting a patch

Run these from the repository root. This is the same check set used by CI:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo metadata --locked --features release-binary --format-version 1 >/dev/null
cargo test --locked
cargo package --locked --allow-dirty --no-verify
```

Use `--no-default-features` or additional feature-specific checks when changing
optional build dependencies.

## Test style

Integration tests should exercise the CLI binary against isolated temporary
sandboxes. Avoid fixed paths outside the sandbox. Tests that require Git should
use local repositories created during the test and should avoid network access.

Prefer one focused test for one behavior boundary. When several cases share a
large manifest or setup, table-drive the cases rather than duplicating long
fixtures.

## Documentation rules

When a user-visible behavior changes, update the matching documentation in the
same patch:

- command behavior: `docs/cli.md`;
- manifest fields or helper behavior: `docs/manifest.md`;
- builtin module inputs/results/corner cases: `docs/builtin-modules.md`;
- Lua module authoring APIs: `docs/module-developers.md` and, when useful,
  `docs/module_contract.lua`;
- release-visible changes: `CHANGELOG.md`.

README should stay small. It is a landing page, not the reference manual.

## Continuous integration scope

The CI workflow is intentionally limited to changes that can affect the Rust
crate, bundled Lua modules, tests, install script, or workflow definitions.
Documentation-only, example-only, changelog-only, license-only, and community
health file changes do not start the full Rust CI job.

The release workflow is different: release builds run only for explicit
`vX.Y.Z` tag pushes or manual `workflow_dispatch` runs. GitHub does not apply
path filters to tag pushes, and a release tag is an explicit publishing action,
so release packaging should not be skipped by source-path filters.

## Release preparation

For a release tag:

1. Run the full local check set.
2. Confirm `Cargo.toml`, `Cargo.lock`, README install examples, and
   `CHANGELOG.md` agree on the release version.
3. Confirm `scripts/install.sh` still matches published asset names.
4. Tag from `master` with a `vX.Y.Z` tag.
5. Let the release workflow build and attach release artifacts. The workflow
   smoke-tests every built binary on its native runner before packaging and
   smoke-tests the macOS universal binary after `lipo` creates it.
6. Download the produced package, install it with `WALI_PACKAGE=...`, and run a
   small `plan`, `check`, and `apply` smoke test locally.

Linux release binaries are built with the `release-binary` feature so vendored
OpenSSL and static zlib are used for portability. macOS release packaging uses a
universal binary.
