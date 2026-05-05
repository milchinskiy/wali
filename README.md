# wali

wali is a small agentless automation tool for local and SSH hosts. The engine is
Rust; manifests and modules are Lua. A normal run is intentionally simple:
inspect the plan, check real hosts, apply changes, and clean up only resources
that a previous successful apply recorded as created.

## Status

The current release line is `0.1.x`. The manifest format, module contract, and
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
curl -fsSL https://raw.githubusercontent.com/milchinskiy/wali/master/scripts/install.sh | sh
```

Useful installer overrides:

```sh
WALI_VERSION=v0.1.0 sh scripts/install.sh
WALI_INSTALL_DIR="$HOME/.local/bin" sh scripts/install.sh
WALI_PACKAGE=./wali-linux-x86_64.tar.gz sh scripts/install.sh
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
        m.task("create demo dir")("wali.builtin.dir", {
            path = "/tmp/wali-demo",
            state = "present",
            parents = true,
            mode = "0755",
        }),
        m.task("write message")("wali.builtin.file", {
            path = "/tmp/wali-demo/message.txt",
            content = "managed by wali\n",
            create_parents = true,
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
`cleanup` removes only filesystem entries recorded as `created` in a previous
successful apply state file.

## Builtin modules

Builtin task modules live under the reserved `wali.builtin.*` namespace:

```text
wali.builtin.command
wali.builtin.copy_file
wali.builtin.copy_tree
wali.builtin.dir
wali.builtin.file
wali.builtin.link
wali.builtin.link_tree
wali.builtin.permissions
wali.builtin.pull_file
wali.builtin.push_file
wali.builtin.remove
wali.builtin.template
wali.builtin.touch
```

Target-host filesystem paths are absolute unless a module documents otherwise.
Controller-side paths used by transfer and template modules may be absolute or
relative to manifest `base_path`.

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
  custom Lua modules.
- [`docs/module_contract.lua`](docs/module_contract.lua) is a compact Lua-facing
  contract reference.
- [`docs/development.md`](docs/development.md) covers maintainer checks and the
  release workflow.
