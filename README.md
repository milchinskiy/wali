# wali

wali is a small agentless automation tool for local and SSH hosts. The engine is
Rust; manifests and modules are Lua. A normal run is intentionally simple:
inspect the plan, check real hosts, apply changes, and clean up only resources
that a previous successful apply recorded as created.

## Status

The current release line is `0.2.x`. The manifest format, module contract, and
state-file format are ready to use, but they are not a 1.0 compatibility promise
yet. Release-visible changes are recorded in [`CHANGELOG.md`](CHANGELOG.md).

## Install

Build from a checkout:

```sh
cargo build --release
cargo install --path .
```

Install the latest release binary on Linux or macOS:

```sh
curl -fsSL https://raw.githubusercontent.com/milchinskiy/wali/master/scripts/install.sh | WALI_INSTALL_DIR="$HOME/.local/bin" sh
```

Useful installer overrides:

```sh
WALI_VERSION=v0.2.0 sh scripts/install.sh
WALI_INSTALL_DIR="$HOME/.local/bin" sh scripts/install.sh
WALI_PACKAGE=./wali-linux-x86_64.tar.gz sh scripts/install.sh
WALI_DATA_DIR="$HOME/.local/share/wali" sh scripts/install.sh
WALI_TYPES_DIR="$HOME/.local/share/wali/types" sh scripts/install.sh
WALI_INSTALL_TYPES=0 sh scripts/install.sh
```

Requirements for building from source are Rust 1.94.0 or newer, a C toolchain,
and the native dependencies required by `ssh2`/`libssh2` on the current
platform. System `git` is required when manifests use Git module sources.

For development, the repository also provides a Nix shell:

```sh
nix develop -c $SHELL
```

## Quick start

```lua
local m = require("manifest")

return {
    hosts = {
        m.host.localhost("localhost"),
    },

    tasks = {
        m.task("create demo dir")("wali.builtin.mkdir", {
            path = "/tmp/wali-demo",
            parents = true,
            mode = "0755",
        }),
        m.task("write message")("wali.builtin.write", {
            dest = "/tmp/wali-demo/message.txt",
            content = "managed by wali\n",
            parents = true,
            mode = "0644",
        }, {
            depends_on = { "create demo dir" },
        }),
    },
}
```

Run it:

```sh
wali plan manifest.lua
wali check manifest.lua
wali apply --state-file apply-state.json manifest.lua
wali cleanup --state-file apply-state.json manifest.lua
```

`plan` compiles the manifest without connecting to hosts. `check` connects,
prepares modules, evaluates host predicates, normalizes arguments, and validates
module input without mutating hosts. `apply` performs the checked changes.
`cleanup` removes only target-host filesystem entries recorded as `created` in a
previous successful apply state file. Controller-side artifacts reported by pull
operations are not removed by host cleanup.

## Builtin modules

Builtin task modules live under the reserved `wali.builtin.*` namespace:

```text
wali.builtin.touch
wali.builtin.mkdir
wali.builtin.write
wali.builtin.link
wali.builtin.copy
wali.builtin.push
wali.builtin.pull
wali.builtin.remove
wali.builtin.permissions
wali.builtin.command
```

Builtin modules are imperative verbs. They do not expose declarative `state`
fields: creation, writing, linking, copying, transferring, removal, permission
changes, and command execution are separate operations. The common option
`parents` creates missing parent directories where applicable. When
`replace = false`, matching destinations report unchanged, conflicting
single-path destinations skip the task, and conflicting recursive leaves are
left in place while the remaining entries continue.

Target-host filesystem paths are absolute unless a module documents otherwise.
Controller-side paths used by `write`, `push`, and `pull` may be absolute or
relative to manifest `base_path`.

For localhost manifests that intentionally need an absolute path next to the
manifest file, use `require("manifest").here(...)`. For example, dotfile
manifests can pass `src = m.here("home")` to `wali.builtin.link` with
`recursive = true`; the resulting path is valid when the target host sees the
same filesystem, normally `localhost`.

## External modules

Wali can load task modules from local directories or Git repositories. The core
repository ships only the `wali.builtin.*` modules. Higher-level operational
modules are kept outside the core so the engine stays small and the module set
can evolve independently.

The official companion module repository is
[`wali-ops`](https://github.com/milchinskiy/wali-ops). It provides small,
command-shaped modules for common host operations such as package managers,
services, users, groups, downloads, and text-file edits.

To use `wali-ops`, add it as a Git module source in your manifest:

```lua
local m = require("manifest")

return {
    modules = {
        {
            namespace = "ops",
            git = {
                url = "https://github.com/milchinskiy/wali-ops.git",
                ref = "v0.2.0",
                path = "modules",
                depth = 1,
            },
        },
    },

    hosts = {
        m.host.localhost("localhost"),
    },

    tasks = {
        m.task("apt update")("ops.pkg.apt.update"),

        m.task("install curl")("ops.pkg.apt.install", {
            packages = { "curl" },
        }, {
            depends_on = { "apt update" },
        }),

        m.task("ensure deploy group")("ops.group.create", {
            name = "deploy",
            system = true,
        }),

        m.task("ensure deploy user")("ops.user.create", {
            name = "deploy",
            group = "deploy",
            system = true,
            create_home = false,
        }, {
            depends_on = { "ensure deploy group" },
        }),
    },
}
```

Pin `ref` to a release tag for reproducible manifests. During development,
`master` may be useful, but released examples should prefer tags.

See the `wali-ops` README for the full external module reference.

## Lua editor support

Wali ships LuaLS definition files under [`types/`](types/). Add that directory
to LuaLS `workspace.library` for completion and diagnostics for raw manifest
tables, the `require("manifest")` helper, custom modules, `ctx`,
`require("wali")`, `wali.api`, and `wali.builtin.lib`. The release installer
copies these stubs to `${XDG_DATA_HOME:-$HOME/.local/share}/wali/types` by
default. Set `WALI_TYPES_DIR` to install them elsewhere, or set
`WALI_INSTALL_TYPES=0` to skip editor stub installation. The repository includes
[`.luarc.example.json`](.luarc.example.json) as a starting point.

## Documentation

- [`docs/philosophy.md`](docs/philosophy.md) describes the project goals,
  boundaries, and design principles.
- [`docs/cli.md`](docs/cli.md) documents commands, output modes, selectors,
  state files, cleanup, and host concurrency.
- [`docs/manifest.md`](docs/manifest.md) is the manifest author guide, including
  raw tables, `require("manifest")`, variables, host selectors, task predicates,
  dependencies, and module sources.
- [`docs/builtin-modules.md`](docs/builtin-modules.md) is the detailed builtin
  module reference.
- [`docs/module-developers.md`](docs/module-developers.md) explains how to write
  custom Lua modules, including LuaLS editor setup.
- [`types/`](types/) contains LuaLS definition files for manifests, module
  contexts, helper libraries, and builtin module argument tables.
- [`docs/module_contract.lua`](docs/module_contract.lua) is a compact Lua-facing
  contract reference.
- [`docs/development.md`](docs/development.md) covers maintainer checks and the
  release workflow.
- [`wali-ops`](https://github.com/milchinskiy/wali-ops) provides the companion
  external module set for package managers, services, users, groups, downloads,
  and text-file edits.

## License

SPDX license expression: `MIT OR Apache-2.0`.

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this project by you shall be licensed as above, without any
additional terms or conditions.
