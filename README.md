# wali

wali is a small agentless automation tool written in Rust. Manifests and modules
are written in embedded Lua. The current implementation focuses on local and SSH
hosts, strict execution flow, host-aware checks, and small primitive
filesystem/data modules.

The project is still in active development. Public contracts may change while
the executor, module API, and builtin module set are being stabilized.

## Basic model

A manifest describes hosts and tasks. Each task selects a module and passes
module arguments. Wali compiles a per-host task plan, connects to each host for
host-aware commands, evaluates task predicates and module requirements,
validates module input, and applies changes when requested.

The CLI has three execution layers plus explicit cleanup:

```sh
wali plan manifest.lua
wali check manifest.lua
wali apply manifest.lua
wali cleanup --state-file apply-state.json manifest.lua
```

`plan` is compile-only: no host access, no Git fetches, no module validation.

`check` prepares module sources, resolves task module names, connects to hosts,
evaluates host-aware requirements, normalizes task arguments, and runs module
validation with a read/probe-only Lua context.

`apply` runs the same checks and then executes module `apply` functions with the
full task context.

`cleanup` reads a previous successful apply state file and removes filesystem
entries recorded as `created` resources within the current selected manifest
scope. Cleanup uses the current manifest for host connection data and does not
remove paths that were merely updated or unchanged. Cleanup does not rewrite the
apply state file; run apply again with `--state-file` to record a new baseline.

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
apply. The snapshot contains the selected effective plan, explicit resource
records, and the final apply report state. Failed applies do not overwrite the
state file. This explicit resource snapshot is the durable state contract used
by cleanup.

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

Task dependencies are execution dependencies, not only ordering hints. A task
with `depends_on` runs only when every declared dependency completed
successfully on the same host. `on_change` is also an execution dependency, but
in `apply` it runs the gated task only when at least one referenced source task
reported a real change. If all `on_change` sources succeeded unchanged, the
gated task is skipped with a clear reason. In `check`, `on_change` still orders
and validates the gated task because no apply-time change result exists yet.
Dependencies must be scheduled for the same host; duplicate dependency ids and
duplicate references between `depends_on` and `on_change` are rejected. If a
dependency fails or is skipped, its dependents are skipped with a
dependency-specific reason, while unrelated later tasks continue to run.

## Variables

Manifests, hosts, and tasks may define `vars`. Variables are copied into each
task context after a shallow, deterministic merge:

```text
manifest vars < host vars < task vars
```

Later levels replace earlier values with the same top-level key. Values preserve
their Lua/JSON shape: strings, numbers, booleans, lists, objects, and explicit
`null` are passed to modules through `ctx.vars`. Variable keys must not be empty
and must not have leading or trailing whitespace. Plan output exposes only
variable keys, not values, so variables are useful for ordinary configuration
but are not a secret-management mechanism.

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
- every system `git` process has a timeout. `git.timeout` defaults to `5m` when
  omitted.

Custom Lua modules receive `ctx.controller` for controller-side path helpers and
read-only filesystem access, including deterministic tree walking. Controller
filesystem paths may be absolute or relative to manifest `base_path`; there is
no project-root sandbox. Domain modules should use this primitive API rather
than relying on duplicated file helpers in `ctx.template` or `ctx.transfer`.
Target-host filesystem reads expose both raw bytes through `ctx.host.fs.read`
and strict UTF-8 text through `ctx.host.fs.read_text`. Modules also receive
`ctx.json` for compact JSON decoding and encoding, `ctx.codec` for
byte-oriented codecs such as Base64, and `ctx.hash` for one-way digests such
as SHA-256, without vendoring Lua parsers or shelling out to external tools.

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

Builtin fields that manage target-host filesystem objects require absolute
host paths unless documented otherwise. Controller-side transfer and template
paths may still be absolute or relative to manifest `base_path`. See
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
