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


## Custom module sources

A manifest can load user modules from local directories or from Git repositories.
Local paths are resolved relative to the manifest file:

```lua
modules = {
    { path = "./modules" },
}
```

Module sources may also be mounted under a manifest-local namespace:

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

The namespace is only a public selector used by the manifest. Once a task is
resolved to one effective module source, wali creates a fresh Lua runtime for
that task, adds only that source's include directory to `package.path`, and
loads the source-local module name. Module internals therefore keep normal Lua
imports:

```lua
local tool = require("internal.utils.tool")
```

Git sources are fetched with the system `git` executable before `check` or
`apply`. `plan` remains compile-only and does not fetch Git sources. Git `url`
and `ref` values are strict strings and must not contain surrounding whitespace.

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
        },
    },
}

tasks = {
    {
        id = "run git module",
        module = "ops.file",
        args = {},
    },
}
```

Local module paths must resolve to existing directories and must be safely
representable in Lua `package.path`; paths containing `;` or `?` are rejected.
Namespaces and task module names are strict Lua-style dotted names. Each segment
must match `[A-Za-z_][A-Za-z0-9_]*`. Namespaces must be unique, must not
overlap, and must not use the reserved `wali.*` namespace. Custom sources must
not contain a top-level `wali.lua` or `wali/` tree because that package prefix
is reserved for wali itself. The same repository may be mounted more than once
at different refs only by using different namespaces.

Unnamespaced sources preserve the simple local workflow. If an unnamespaced task
module exists in more than one unnamespaced source, wali fails instead of
choosing by search-path order. Namespaced sources are not exposed globally.
Before `check` or `apply` connects to hosts or asks for secrets, wali prepares
module sources and resolves every task module name.

Git sources are cached under `$WALI_MODULES_CACHE` when it is set, otherwise
under `$XDG_DATA_HOME/wali/modules` or `~/.local/share/wali/modules`. Git
checkouts use short stable source IDs derived from the Git URL, ref, and
submodule materialization mode. The namespace, repository leaf name, and
module `path` are not cache keys. `check` and `apply` hold a process-level cache
lock for every Git source until execution finishes, so another wali process
cannot reset or clean the same checkout while modules are being loaded.

## Builtin modules

Builtin task modules use the reserved `wali.builtin.*` namespace. Unknown
`wali.*` task modules are rejected during manifest/preflight validation. The
current set includes:

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

More detailed builtin module notes are in `docs/builtin-modules.md`. The broader design direction is recorded in `docs/philosophy.md`.

## Module phases

Modules may define `schema`, `requires`, `validate`, and `apply`.

The intended order is:

```text
when -> requires -> schema normalization -> validate -> apply
```

`validate` receives a read/probe-only context. `apply` receives the full
context, including mutation APIs.

See `docs/module_contract.lua` for the compact contract reference and `docs/module-developers.md` for module authoring guidance.

## Development checks

The integration tests exercise the CLI binary against isolated local temporary
directories:

```sh
cargo test
```

The tests avoid fixed system paths and clean up their temporary sandboxes on
drop.
