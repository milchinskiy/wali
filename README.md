# wali

wali is a small agentless automation tool for local and SSH hosts. The engine is
Rust; manifests and modules are Lua. It is built around a simple workflow:
inspect the plan, check against real hosts, apply changes, and clean up only
what a previous successful apply recorded as created.

## Status and compatibility

The current release line is `0.1.x`. The manifest, module, and state-file
formats are ready to use, but they are not a 1.0 compatibility promise yet. When
one of those formats changes, update `CHANGELOG.md` and the matching docs in the
same patch.

## Build and install

Requirements:

- Rust 1.94.0 or newer;
- a C toolchain for native dependencies;
- `pkg-config` and OpenSSL development headers on platforms where the native SSH
  dependency does not find them automatically;
- system `git` when manifests use Git module sources or when running the Git
  module-source tests.

Build from the repository root:

```sh
cargo build --release
```

Install locally from a checkout:

```sh
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
WALI_REPO=your-user/wali sh scripts/install.sh
```

Release assets are published for Linux `x86_64`, Linux `aarch64`, and macOS
universal. The Linux packages are built with the musl target and checked as
static binaries. The release build enables vendored OpenSSL and static zlib for
Linux portability. The macOS package contains one universal binary built from
native Intel and Apple Silicon runners.

For development, the repository includes a Nix shell with the Rust toolchain,
Clippy, rustfmt, Git, Perl, make, pkg-config, and OpenSSL development inputs:

```sh
nix develop -c $SHELL
```

## Basic model

A manifest describes hosts and tasks. A task selects a module and passes module
arguments. Wali expands those tasks per host, evaluates host predicates and
module requirements, validates input, and applies changes when requested.

The CLI has four main commands:

```sh
wali plan manifest.lua
wali check manifest.lua
wali apply manifest.lua
wali cleanup --state-file apply-state.json manifest.lua
```

`plan` compiles the manifest only. It does not connect to hosts, fetch Git
sources, or validate module input.

`check` prepares module sources, resolves modules, connects to hosts, evaluates
host-aware requirements, normalizes arguments, and runs module validation in a
read/probe-only Lua context.

`apply` runs the same checks, then calls module `apply` functions with the full
task context.

`cleanup` reads a previous successful apply state file and removes filesystem
entries recorded as `created` resources inside the current selected manifest
scope. It uses the current manifest for host connection data. It does not remove
paths that were merely updated or unchanged, and it does not rewrite the apply
state file. Run `apply --state-file FILE` again to record a new baseline.

JSON output is available for all commands:

```sh
wali --json plan manifest.lua
wali --json check manifest.lua
wali --json apply manifest.lua
wali --json cleanup --state-file apply-state.json manifest.lua
wali --json-pretty apply manifest.lua
```

`check`, `apply`, and `cleanup` run hosts concurrently by default. Use
`--jobs N` on any of those commands to cap host concurrency without changing
per-host task order:

```sh
wali check --jobs 1 manifest.lua
wali apply --jobs 4 manifest.lua
wali cleanup --jobs 1 --state-file apply-state.json manifest.lua
```

`--jobs 1` runs hosts serially in manifest order. Tasks within one host always
run sequentially.

`apply --state-file FILE` writes an atomic JSON snapshot after a successful
apply. The snapshot contains the selected effective plan, resource records, and
the final apply report state. Failed applies do not overwrite the state file.
Cleanup reads this snapshot.

Use `--host ID`, `--host-tag TAG`, `--task ID`, and `--task-tag TAG` on `plan`,
`check`, `apply`, or `cleanup` to select a smaller working set without changing
the manifest:

```sh
wali plan --host web-1 manifest.lua
wali check --task deploy manifest.lua
wali apply --host web-1 --task deploy manifest.lua
wali cleanup --host-tag web --task-tag deploy --state-file apply-state.json manifest.lua
```

Selectors are exact ids or exact tags and may be repeated. Host id and host tag
selectors select the union of matching hosts. Task id and task tag selectors
select the union of matching tasks. Host and task dimensions are intersected.
Selecting a task by id or tag includes its transitive `depends_on` and
`on_change` source tasks on the same host, but it does not include downstream
dependents. `plan` prints the same selected plan that `check` and `apply` would
execute. For selected plans, module source preparation and validation are
limited to modules required by the selected tasks. For `cleanup`, host id/tag
selectors limit cleanup to previous created entries on selected hosts. Task
id/tag selectors limit cleanup to previous created entries from the selected
task dependency closure.

## Manifest helper module

A manifest file is just Lua that returns a table. While that Lua chunk is being
loaded, wali provides a small helper module:

```lua
local m = require("manifest")
```

The helper is optional. It returns the same tables you could write by hand, so
raw tables and helper-generated tables can be mixed freely.

```lua
hosts = {
    m.host.localhost("localhost", {
        tags = { "local" },
        vars = { role = "controller" },
        command_timeout = "30s",
    }),

    m.host.ssh("web-1", {
        user = "deploy",
        host = "web-1.example.invalid",
        port = 22,
        auth = "agent",
        connect_timeout = "10s",
        keepalive_interval = "30s",
        tags = { "web" },
    }),
}
```

`m.host.localhost(id, opts)` emits a local host. Common host options are `tags`,
`vars`, `run_as`, and `command_timeout`.

`m.host.ssh(id, opts)` emits an SSH host. SSH options are `user`, `host`,
`port`, `host_key_policy`, `auth`, `connect_timeout`, and `keepalive_interval`.
Common host options use the same names as `m.host.localhost`.

`m.task(id)(module, args, opts)` emits a task. If `args` is omitted, it uses an
empty table. Optional task fields are `tags`, `depends_on`, `on_change`, `when`,
`host`, `run_as`, and `vars`.

The helper rejects unknown option names and non-table option values. Helper ids
and task module names must be strings without leading/trailing whitespace or
control characters. `m.host.ssh` requires `user` and `host`; the manifest loader
validates the rest of the SSH configuration.

Task `host` selectors use the manifest selector form: `{ id = "web-1" }`,
`{ tag = "web" }`, `{ all = { ... } }`, `{ any = { ... } }`, or
`{ ["not"] = ... }`.

## Minimal manifest

Hosts may set `command_timeout = "30s"` to provide a default timeout for host
commands, including the initial fact probe performed during connection.
Per-command `timeout` values override the host default.

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

Task dependencies affect execution, not just ordering. A task with `depends_on`
runs only after every listed dependency has completed successfully on the same
host. `on_change` is also a dependency: in `apply`, the gated task runs only
when at least one source task reports a change. In `check`, `on_change` still
orders and validates the gated task because no apply-time result exists yet.

Dependencies must land on the same host. Duplicate dependency ids and duplicate
references between `depends_on` and `on_change` are rejected. If a dependency
fails or is skipped, its dependents are skipped; unrelated later tasks still
run.

Host ids, task ids, tags, and `run_as` ids/users must not be empty, have
leading/trailing whitespace, or contain control characters. Task ids may contain
ordinary internal spaces, as shown above.

## Variables

Manifests, hosts, and tasks may define `vars`. Variables are copied into each
task context after a shallow, deterministic merge:

```text
manifest vars < host vars < task vars
```

Later levels replace earlier values with the same top-level key. Values keep
their Lua/JSON shape: strings, numbers, booleans, lists, objects, and explicit
`null` are passed to modules through `ctx.vars`. Variable keys must not be empty
or have leading/trailing whitespace. Plan output shows variable keys, not
values, so variables are useful for configuration but not for secrets.

```lua
return {
    vars = {
        app = "demo",
        base_dir = "/opt/demo",
    },

    hosts = {
        {
            id = "web-1",
            transport = "local",
            vars = { role = "web", port = 8080 },
        },
    },

    tasks = {
        {
            id = "write config",
            module = "custom.write_config",
            vars = { config_name = "demo.conf" },
            args = {},
        },
    },
}
```

Inside `custom.write_config`, the effective values are available as
`ctx.vars.app`, `ctx.vars.role`, `ctx.vars.port`, and `ctx.vars.config_name`.

Variables are especially useful with `wali.builtin.template`, which renders
either a controller-side MiniJinja template file or inline template content and
writes the result to the target host:

```lua
{
    id = "write app config",
    module = "wali.builtin.template",
    args = {
        src = "templates/app.conf.j2",
        dest = "/etc/demo/app.conf",
        create_parents = true,
        mode = "0644",
    },
}
```

When `src` is used, template source paths use the same controller-side
`base_path` rules as `wali.builtin.push_file`. `content` can be used instead for
inline templates. Exactly one of `src` or `content` must be set. The template
context is `ctx.vars` plus optional `args.vars`, where `args.vars` wins on
duplicate top-level keys.

Tasks may also declare a host-aware `when` predicate. `when` is evaluated after
the host connection is established and before module `requires`, schema
normalization, validation, or apply. A task whose predicate does not match is
reported as skipped and is treated as a skipped dependency for downstream tasks.

```lua
{
    id = "write only on Linux with curl",
    when = {
        all = {
            { os = "linux" },
            { command_exist = "curl" },
            { path_dir = "/etc" },
            { ["not"] = { env_set = "SKIP_THIS_TASK" } },
        },
    },
    module = "wali.builtin.file",
    args = { path = "/tmp/wali-demo/message.txt", content = "managed\n" },
}
```

Supported task predicates are `all`, `any`, `not`, `os`, `arch`, `hostname`,
`user`, `group`, `env`, `env_set`, `path_exist`, `path_file`, `path_dir`,
`path_symlink`, and `command_exist`. `all` and `any` must contain at least one
predicate, and string predicate arguments must not be empty.

## Custom modules

A manifest may load custom modules from local directories or Git repositories.
Local module paths are resolved relative to the manifest file:

```lua
modules = {
    { path = "./modules" },
}
```

A module source may also be mounted under a manifest-local namespace:

```lua
modules = {
    { namespace = "local_ops", path = "./modules" },
}

tasks = {
    {
        id = "run namespaced module",
        module = "local_ops.example_file",
        args = {},
    },
}
```

The namespace is chosen by the manifest author. Module authors do not need it
for internal imports. After a task resolves to one source, wali creates a fresh
Lua runtime, adds only that source root to `package.path`, and loads the
source-local module name. Internal imports remain plain Lua:

```lua
local tool = require("internal.utils.tool")
```

Git sources use the system `git` executable before `check` and `apply`:

```lua
modules = {
    {
        namespace = "ops",
        git = {
            url = "https://example.invalid/ops/wali-modules.git",
            ref = "main",
            path = "modules",
            depth = 1,
            submodules = false,
            timeout = "5m",
        },
    },
}
```

Critical source rules:

- module and namespace names are strict dotted Lua-style identifiers;
- custom sources must not expose `wali.lua` or a top-level `wali/` tree;
- namespaced sources are not exposed globally;
- ambiguous unnamespaced module names fail instead of depending on search order;
- `plan` does not fetch Git sources;
- `check` and `apply` prepare and lock Git checkouts until execution finishes;
- every system `git` process has a timeout. `git.timeout` defaults to `5m` when
  omitted.

Custom Lua modules receive `ctx.controller` for controller-side path helpers and
read-only filesystem access, including deterministic tree walking. Controller
filesystem paths may be absolute or relative to manifest `base_path`; there is
no project-root sandbox. Domain modules should use this primitive API instead of
duplicating file helpers in `ctx.template` or `ctx.transfer`. Target-host reads
expose raw bytes through `ctx.host.fs.read` and strict UTF-8 text through
`ctx.host.fs.read_text`. Modules also receive `ctx.json`, `ctx.codec` for
Base64, and `ctx.hash` for SHA-256 without vendoring Lua parsers or shelling
out.

The detailed custom module and Git source contract lives in
[`docs/module-developers.md`](docs/module-developers.md).

## Builtin modules

Builtin task modules use the reserved `wali.builtin.*` namespace. Unknown
`wali.*` task modules are rejected during manifest/preflight validation.

Current builtins:

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

Builtin fields that manage target-host filesystem objects require absolute host
paths unless documented otherwise. Controller-side transfer and template paths
may still be absolute or relative to manifest `base_path`. See
[`docs/builtin-modules.md`](docs/builtin-modules.md) for module-specific
arguments, behavior, and safety notes.

## Module phases

Modules may define `requires`, `schema`, `validate`, and `apply`. The intended
order is:

```text
when -> requires -> schema normalization -> validate -> apply
```

`validate` receives a read/probe-only context. `apply` receives the full
context, including mutation APIs. See
[`docs/module_contract.lua`](docs/module_contract.lua) for the compact API
reference and [`docs/module-developers.md`](docs/module-developers.md) for
authoring guidance.

## Documentation map

- [`docs/philosophy.md`](docs/philosophy.md) records project goals, boundaries,
  and design principles.
- [`docs/module-developers.md`](docs/module-developers.md) is the custom module
  authoring guide.
- [`docs/module_contract.lua`](docs/module_contract.lua) is a compact Lua-facing
  contract reference.
- [`docs/builtin-modules.md`](docs/builtin-modules.md) documents builtin module
  arguments and behavior.
- [`CHANGELOG.md`](CHANGELOG.md) records release-visible changes.

## Development checks

The integration tests exercise the CLI binary against isolated local temporary
directories:

```sh
cargo test
```

The tests avoid fixed system paths and clean up their temporary sandboxes on
drop.

The broader design direction is recorded in
[`docs/philosophy.md`](docs/philosophy.md).
