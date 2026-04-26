# wali

wali is a small agentless automation tool written in Rust. Manifests and modules
are written in embedded Lua. The current implementation focuses on local and SSH
hosts, strict execution flow, host-aware checks, and desired-state filesystem
modules.

The project is still in active development. Public contracts may change while
the executor, module API, and builtin module set are being stabilized.

## Basic model

A manifest describes hosts and tasks. Each task selects a module and passes
module arguments. The engine compiles a per-host task plan, connects to each
host, evaluates task predicates, validates module input, and applies changes
when requested.

Execution has three main CLI layers:

```sh
wali plan manifest.lua
wali check manifest.lua
wali apply manifest.lua
```

`plan` is compile-only. It does not connect to hosts and does not run module
validation.

`check` connects to hosts, evaluates `when`, checks module `requires`,
normalizes arguments, and runs module validation with a read-only Lua context.
It must not mutate host state through the normal context API.

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

## Builtin modules

Builtin modules use the reserved `wali.builtin.*` namespace. The current set
includes:

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
wali.builtin.walk
```

Most filesystem modules are desired-state modules. They should report
`unchanged` when the host already matches the requested state.

More detailed builtin module notes are in `docs/builtin-modules.md`.

## Module phases

Modules may define `schema`, `requires`, `validate`, and `apply`.

The intended order is:

```text
when -> requires -> schema normalization -> validate -> apply
```

`validate` receives a read/probe-only context. `apply` receives the full
context, including mutation APIs.

See `docs/module_contract.lua` for the current Lua module contract.

## Development checks

The integration tests exercise the CLI binary against isolated local temporary
directories:

```sh
cargo test
```

The tests avoid fixed system paths and clean up their temporary sandboxes on
drop.
