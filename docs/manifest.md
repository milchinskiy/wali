# Manifest guide

A manifest is a Lua file that returns a table. The table describes hosts,
optional custom module sources, variables, and tasks.

```lua
---@type WaliManifestDefinition
return {
    name = "demo",
    base_path = ".",
    vars = {},
    hosts = {},
    modules = {},
    tasks = {},
}
```

Only `tasks` is required by the raw schema. In practice, a useful manifest also
contains at least one host.

## Helper module

During manifest loading, wali exposes a small helper module:

```lua
local m = require("manifest")
```

The helper is optional. It emits the same tables you can write by hand, so raw
tables and helper-generated tables can be mixed freely.

```lua
local m = require("manifest")

return {
    hosts = {
        m.host.localhost("localhost"),
    },

    tasks = {
        m.task("write message")("wali.builtin.file", {
            path = "/tmp/wali-demo/message.txt",
            content = "managed by wali\n",
            create_parents = true,
        }),
    },
}
```

The helper rejects unknown option names and non-table option values. Helper ids
and task module names must be strings without leading/trailing whitespace or
control characters.

LuaLS users can add `types/` to `workspace.library` to get completion for raw
manifest tables (`WaliManifestDefinition`), `require("manifest")`, host helpers,
task helper options, module sources, host selectors, `when` predicates, and
Wali's `null` sentinel. `.luarc.example.json` contains a minimal setup.

## Top-level fields

### `name`

Optional string. If omitted or empty, wali uses the manifest file path as the
manifest name in compiled plans and state files.

### `base_path`

Optional controller-side directory used by modules that read or write
controller-side files, such as `wali.builtin.push_file`,
`wali.builtin.push_tree`, `wali.builtin.pull_file`, `wali.builtin.pull_tree`,
`wali.builtin.template`, and `ctx.controller` helpers.

Rules:

- omitted or empty `base_path` means the manifest directory;
- relative `base_path` is resolved relative to the manifest directory;
- absolute `base_path` is used as-is;
- the resolved path must exist and be a directory.

`base_path` is not a sandbox. Controller-side module APIs may also accept
absolute paths when the module documents that behavior.

### `vars`

Optional table of JSON-like values. Manifest, host, and task variables are
merged shallowly for each task:

```text
manifest vars < host vars < task vars
```

Later levels replace earlier values with the same top-level key. Values keep
their Lua/JSON shape: strings, numbers, booleans, arrays, objects, and explicit
`null`. Variable keys must be non-empty and must not have leading or trailing
whitespace. Plan output shows variable keys, not values; do not treat variables
as a secret store.

Example:

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

The module receives `ctx.vars.app`, `ctx.vars.role`, `ctx.vars.port`, and
`ctx.vars.config_name`.

## Hosts

A host has an `id`, a `transport`, optional `tags`, optional `vars`, optional
`run_as` entries, and an optional `command_timeout`.

### Local host

Helper form:

```lua
m.host.localhost("localhost", {
    tags = { "local" },
    vars = { role = "controller" },
    command_timeout = "30s",
})
```

Raw form:

```lua
{
    id = "localhost",
    transport = "local",
    tags = { "local" },
    command_timeout = "30s",
}
```

`transport = "local"` executes host operations on the controller machine through
the same backend abstraction used for SSH hosts.

### SSH host

Helper form:

```lua
m.host.ssh("web-1", {
    user = "deploy",
    host = "web-1.example.invalid",
    port = 22,
    auth = "agent",
    host_key_policy = { strict = {} },
    connect_timeout = "10s",
    keepalive_interval = "30s",
    tags = { "web" },
})
```

Raw form:

```lua
{
    id = "web-1",
    transport = {
        ssh = {
            user = "deploy",
            host = "web-1.example.invalid",
            port = 22,
            auth = "agent",
            host_key_policy = { strict = {} },
            connect_timeout = "10s",
            keepalive_interval = "30s",
        },
    },
    tags = { "web" },
}
```

SSH fields:

```text
user                required non-empty string
host                required non-empty string
port                optional integer, default 22, must be greater than zero
connect_timeout     optional human duration, must be greater than zero
keepalive_interval  optional human duration, must be greater than zero
```

Authentication:

```lua
auth = "agent"
auth = "password"
auth = { key_file = { private_key = "/home/deploy/.ssh/id_ed25519" } }
auth = { key_file = {
    private_key = "/home/deploy/.ssh/id_ed25519",
    public_key = "/home/deploy/.ssh/id_ed25519.pub",
} }
```

Host key policies:

```lua
host_key_policy = "ignore"
host_key_policy = { allow_add = {} }
host_key_policy = { allow_add = { path = "/home/me/.ssh/known_hosts" } }
host_key_policy = { strict = {} }
host_key_policy = { strict = { path = "/home/me/.ssh/known_hosts" } }
```

The default policy is strict checking against the user's default known-hosts
file.

### Host tags

`tags` is an optional list of non-empty strings. Tags are used by manifest host
selectors and CLI `--host-tag` selectors.

### `command_timeout`

`command_timeout` is an optional human duration, such as `30s` or `5m`. It is
the default timeout for host commands, including the initial fact probe. A
per-command timeout overrides it.

### `run_as`

A host may define named privilege-switching profiles. Tasks opt into a profile
with `run_as = "id"`.

```lua
{
    id = "localhost",
    transport = "local",
    run_as = {
        {
            id = "root",
            user = "root",
            via = "sudo",
            env_policy = "clear",
            pty = "auto",
        },
    },
}
```

Supported `via` values are `sudo`, `doas`, and `su`. Supported environment
policies are `clear`, `preserve`, and `{ keep = { "NAME", ... } }`. Supported
PTY modes are `never`, `auto`, and `require`. `extra_flags` and `l10n_prompts`
may be provided as string lists.

`run_as` ids and users must be non-empty, without leading/trailing whitespace or
control characters. Each host's `run_as` ids must be unique.

## Tasks

A task selects a module and passes module arguments:

```lua
{
    id = "write message",
    module = "wali.builtin.file",
    args = {
        path = "/tmp/wali-demo/message.txt",
        content = "managed by wali\n",
    },
}
```

Helper form:

```lua
m.task("write message")("wali.builtin.file", {
    path = "/tmp/wali-demo/message.txt",
    content = "managed by wali\n",
}, {
    tags = { "demo" },
    depends_on = { "prepare" },
})
```

Task fields:

```text
id          required unique string
module      required dotted module name
args        required by raw tables; helper defaults it to {}
tags        optional list of tags
depends_on  optional list of task ids
on_change   optional list of task ids
when        optional host predicate
host        optional host selector
run_as      optional host-local run_as id
vars        optional task variables
```

Task ids, task tags, and task `run_as` values must be non-empty, without
leading/trailing whitespace or control characters. Task ids may contain ordinary
internal spaces.

## Host selectors

A task without `host` runs on every manifest host. A task with `host` runs only
on matching hosts.

```lua
host = { id = "web-1" }
host = { tag = "web" }
host = { all = { { tag = "web" }, { tag = "prod" } } }
host = { any = { { id = "web-1" }, { id = "web-2" } } }
host = { ["not"] = { tag = "disabled" } }
```

`all` and `any` must contain at least one selector. `id` selectors are checked
against known host ids during manifest validation.

## Dependencies and `on_change`

`depends_on` and `on_change` are host-local task references. Both forms order
the current task after the referenced source tasks, and selecting a task by id
or tag includes both normal dependencies and change-gated source tasks.

`depends_on` is the ordinary success gate: the current task runs only when every
listed dependency succeeded.

`on_change` is a success-and-change gate during `apply`: the current task runs
only when every listed source succeeded and at least one listed source reported
a changed execution result. If all `on_change` sources were unchanged, wali
reports the gated task as skipped. During `check`, `on_change` still orders and
validates the gated task because no apply-time change result exists yet.

```lua
{
    id = "render nginx config",
    module = "wali.builtin.template",
    args = { src = "nginx.conf.j2", dest = "/etc/nginx/nginx.conf" },
}

{
    id = "reload nginx",
    on_change = { "render nginx config" },
    module = "wali.builtin.command",
    args = { program = "systemctl", args = { "reload", "nginx" } },
}
```

Duplicate references, self-references, unknown task ids, and references to tasks
not scheduled for the same host are rejected. Do not list the same source in
both `depends_on` and `on_change`.

## `when` predicates

A task may declare `when` when the decision to run the task depends on host
facts or cheap host probes. Wali evaluates `when` after connecting to the host
and before module `requires`, schema normalization, `validate`, or `apply`.

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

Supported predicates:

```lua
when = { os = "linux" }
when = { arch = "x86_64" }
when = { hostname = "web-1" }
when = { user = "root" }
when = { user = 0 }
when = { group = "root" }
when = { group = 0 }
when = { env = { "NAME", "value" } }
when = { env_set = "NAME" }
when = { path_exist = "/path" }
when = { path_file = "/path" }
when = { path_dir = "/path" }
when = { path_symlink = "/path" }
when = { command_exist = "tar" }
```

Predicates can be composed with non-empty `all` and `any` lists and unary `not`.
Because `not` is a Lua keyword, quote it as a table key:

```lua
when = {
    any = {
        { command_exist = "curl" },
        { command_exist = "wget" },
    },
}

when = { ["not"] = { env_set = "DISABLE_TASK" } }
```

`path_file` and `path_dir` follow symlinks. `path_symlink` inspects the path
itself and therefore matches symlinks even when the link target is missing.
Empty `all`/`any` lists and empty string predicate arguments are rejected during
manifest validation.

Use task `when` for deployment decisions. Use module `requires` for capabilities
that the module itself needs regardless of who uses it.

## Module sources

A manifest may add custom module sources from local directories or Git
repositories.

Local source:

```lua
modules = {
    { path = "./modules" },
}
```

Namespaced local source:

```lua
modules = {
    { namespace = "local_ops", path = "./modules" },
}

tasks = {
    { id = "run custom module", module = "local_ops.example_file", args = {} },
}
```

Git source:

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

Each source must define exactly one of `path` or `git`.

### Local source rules

Local module paths are resolved relative to the manifest file, canonicalized,
and must point to an existing directory. The include path must be safe for Lua
`package.path`: it must not contain `;` or `?`, and it must not expose
`wali.lua` or a top-level `wali/` tree.

### Git source rules

`check` and `apply` prepare Git sources using the system `git` executable.
`plan` does not fetch Git sources.

Git fields:

```text
url         required non-empty URL/path passed to git, without surrounding whitespace
ref         required Git ref, without unsafe ref syntax
path        optional repository-relative include directory
depth       optional positive integer used as --depth
submodules  optional boolean, default false
timeout     optional human duration, default 5m, must be greater than zero
```

HTTP(S) URLs must not embed credentials. Use Git credential helpers or SSH URLs
for private repositories.

Git sources are cached under `WALI_MODULES_CACHE` when that environment variable
is set, otherwise under `$XDG_DATA_HOME/wali/modules`, otherwise under
`~/.local/share/wali/modules`. The cache key includes URL, ref, and submodule
mode. Wali locks each checkout while preparing it, updates the origin URL,
fetches the requested ref, checks out `FETCH_HEAD`, cleans the worktree, and
updates or deinitializes submodules according to `submodules`.

### Module name mapping

A source exposes Lua files below its include root:

```text
file.lua                 -> file
file/init.lua            -> file
internal/utils/tool.lua  -> internal.utils.tool
```

`file.lua` and `file/init.lua` in the same source are ambiguous and are
rejected.

A namespace is chosen by the manifest author. It is not part of the module
author's internal import paths. After a task resolves to one source, wali
creates a fresh Lua runtime, adds only that source root to `package.path`, and
loads the source-local module name. Internal imports remain plain Lua:

```lua
local tool = require("internal.utils.tool")
```

Name rules:

- task module names and namespaces are dotted Lua-style identifiers;
- each segment must match `[A-Za-z_][A-Za-z0-9_]*`;
- empty segments, surrounding whitespace, path separators, dashes, and
  shell-like punctuation are invalid;
- `wali` and `wali.*` are reserved for wali itself;
- namespaces must be unique in one manifest;
- namespace prefixes must not overlap, so `repo` and `repo.lib` cannot both be
  mounted in one manifest;
- namespaced sources are not exposed globally;
- ambiguous unnamespaced module names fail instead of depending on search order.

## Template variables

`wali.builtin.template` renders either a controller-side MiniJinja template file
or inline template content and writes the result to the target host:

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

When `src` is used, template source paths follow `base_path` rules. `content`
can be used instead for inline templates. Exactly one of `src` or `content` must
be set. The template context is `ctx.vars` plus optional `args.vars`, where
`args.vars` wins on duplicate top-level keys.
