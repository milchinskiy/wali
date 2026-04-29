# wali

wali is a small agentless automation tool written in Rust. Manifests and modules
are written in embedded Lua. The current implementation focuses on local and SSH
hosts, strict execution flow, host-aware checks, and desired-state filesystem
modules.

The project is still in active development. Public contracts may change while
the executor, module API, and builtin module set are being stabilized.

## Basic model

A manifest describes hosts and tasks. Each task selects a module and passes
module arguments. Wali compiles a per-host task plan, connects to each host for
host-aware commands, evaluates task predicates and module requirements,
validates module input, and applies changes when requested.

The CLI has three layers:

```sh
wali plan manifest.lua
wali check manifest.lua
wali apply manifest.lua
```

`plan` is compile-only: no host access, no Git fetches, no module validation.

`check` prepares module sources, resolves task module names, connects to hosts,
evaluates host-aware requirements, normalizes task arguments, and runs module
validation with a read/probe-only Lua context.

`apply` runs the same checks and then executes module `apply` functions with the
full task context.

JSON output is available for all commands:

```sh
wali --json plan manifest.lua
wali --json check manifest.lua
wali --json apply manifest.lua
wali --json-pretty apply manifest.lua
```

## Minimal manifest

Hosts may set `command_timeout = "30s"` to provide a default timeout for host
commands, including the initial fact probe performed during connection.
Per-command `timeout` values override the host default.

```lua
return {
    hosts = {
        { id = "localhost", transport = "local" },
    },

    tasks = {
        {
            id = "create demo dir",
            module = "wali.builtin.dir",
            args = {
                path = "/tmp/wali-demo",
                state = "present",
                parents = true,
                mode = "0755",
            },
        },
        {
            id = "write message",
            depends_on = { "create demo dir" },
            module = "wali.builtin.file",
            args = {
                path = "/tmp/wali-demo/message.txt",
                content = "managed by wali\n",
                create_parents = true,
                mode = "0644",
            },
        },
    },
}
```

Task dependencies are execution dependencies, not only ordering hints. A task with
`depends_on` runs only when every declared dependency completed successfully on
the same host. If a dependency fails or is skipped, its dependents are skipped
with a dependency-specific reason, while unrelated later tasks continue to run.
`check` and `apply` use the same dependency semantics.

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

The namespace is a public selector for the manifest, not something module
authors need to know. After a task resolves to one effective module source, wali
creates a fresh Lua runtime for that task, adds only that source root to
`package.path`, and loads the source-local module name. Internal imports remain
ordinary Lua imports:

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
- every system `git` process has a timeout. `git.timeout` defaults to `5m` when omitted.

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
wali.builtin.remove
wali.builtin.touch
```

See [`docs/builtin-modules.md`](docs/builtin-modules.md) for module-specific
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
