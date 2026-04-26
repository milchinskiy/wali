# Module developer guide

This document describes how to write custom wali Lua modules against the current
module contract.

The contract is still evolving, but the main boundaries are already important:
`requires` checks host capabilities, `validate` is non-mutating, and `apply`
performs changes.

## Module location

A manifest can add local module paths:

```lua
return {
    hosts = {
        { id = "localhost", transport = "local" },
    },

    modules = {
        { path = "./modules" },
    },

    tasks = {
        {
            id = "run custom module",
            module = "example_file",
            args = {
                path = "/tmp/example.txt",
                content = "hello\n",
            },
        },
    },
}
```

A module source can also be mounted under a namespace:

```lua
modules = {
    { namespace = "local_ops", path = "./modules" },
}

tasks = {
    {
        id = "run custom module",
        module = "local_ops.example_file",
        args = {},
    },
}
```

The namespace is selected by the manifest author. Module authors do not need to
know it. Wali resolves `local_ops.example_file` to the `local_ops` source,
creates a fresh Lua runtime for that one task, adds only that source root to
`package.path`, and loads the source-local module name `example_file`.

Internal imports should therefore stay source-local and ordinary:

```lua
local tool = require("internal.utils.tool")
```

This import resolves inside the effective source selected for the task. Two
repositories can contain the same `internal/utils/tool.lua` tree without
colliding, as long as the task modules are addressed through different manifest
namespaces.

A module file should return one Lua table:

```lua
local api = require("wali.api")

return {
    name = "example file",
    description = "writes one file",

    schema = {
        type = "object",
        required = true,
        props = {
            path = { type = "string", required = true },
            content = { type = "string", required = true },
            mode = { type = "string", default = "0644" },
        },
    },

    validate = function(ctx, args)
        if args.path == "/" then
            return api.result.validation():fail("path must not be /"):build()
        end
        return nil
    end,

    apply = function(ctx, args)
        return ctx.host.fs.write(args.path, args.content, {
            create_parents = true,
            mode = tonumber(args.mode, 8),
        })
    end,
}
```

Do not put custom modules under `wali.*`. That namespace is reserved for modules
shipped with wali.


## Git module sources

A manifest can also fetch module code from Git:

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
        module = "ops.example_file",
        args = {},
    },
}
```

`url` and `ref` are required and must not contain surrounding whitespace. The
source is fetched with the system `git` executable before `wali check` and
`wali apply`. `wali plan` does not fetch Git sources, because it is compile-only
and must not depend on network access.

Field behavior:

- `namespace` is optional and belongs to the module source wrapper, not to the
  Git source itself. It is unique only inside one manifest and is used only to
  select the effective source for a task.
- `ref` may be a branch, tag, full ref, or commit accepted by `git fetch origin <ref>`.
  It is always fetched before `check` and `apply`; pin a commit for reproducible
  module code.
- `path` adds a module include directory inside the checked-out repository. It
  must be relative and must not contain parent-directory components.
- `depth` performs a shallow fetch when set. It must be greater than zero.
- `submodules = true` runs `git submodule update --init --recursive --force`
  after checkout. Sources with and without submodule materialization use separate
  cache checkouts.

Local module paths must resolve to existing directories and must be safely
representable in Lua `package.path`; paths containing `;` or `?` are rejected.
Namespaces and task module names must be valid Lua-style dotted names such as
`ops`, `repo_1`, `company.ops`, or `repo_1.file_writer`. Every segment must
match `[A-Za-z_][A-Za-z0-9_]*`; names such as `repo-1.writer`, `.writer`, and
`repo..writer` are invalid. Namespaces must be unique, must not overlap (`repo`
and `repo.lib` in one manifest are invalid), and must not use `wali` or
`wali.*`. Custom sources must not contain a top-level `wali.lua` or `wali/`
tree because that package prefix is reserved for wali itself.

Unnamespaced sources preserve the simple local workflow. When multiple
unnamespaced sources contain the same module name, wali fails clearly instead of
letting `package.path` order choose one. Namespaced sources are not exposed
globally; a task must use the namespace prefix. Before `check` or `apply`
connects to hosts or asks for secrets, wali prepares module sources and resolves
every task module name.

Git module sources are cached under `$WALI_MODULES_CACHE` when that environment
variable is set. Otherwise wali uses `$XDG_DATA_HOME/wali/modules`, falling back
to `~/.local/share/wali/modules`. Checkouts live under short stable source IDs
derived from the Git URL, ref, and submodule materialization mode. The namespace,
repository leaf name, and module `path` are not cache keys. `check` and `apply`
hold a process-level cache lock for every Git source until execution finishes,
so another wali process cannot reset or clean the same checkout while modules
are being loaded. HTTP(S) credentials must not be embedded in module URLs; use a
Git credential helper, SSH agent, or another system-Git credential mechanism
instead.

A Git module source is a distribution mechanism only. The module contract is the
same as for local modules.

## Module fields

A module may define:

```lua
return {
    name = "human readable name",
    description = "human readable description",
    requires = nil,
    schema = nil,
    validate = nil,
    apply = function(ctx, args) ... end,
}
```

`apply` is required for a module that is used by `wali apply`. `schema`,
`requires`, and `validate` are optional, but serious modules should normally use
all three.

## Schema

`schema` normalizes untyped Lua task arguments into a predictable shape before
`validate` and `apply` receive them.

Supported schema kinds are:

```text
any
null
string
number
integer
boolean
list
tuple
enum
object
map
```

Use schemas to catch wrong argument types early and to apply simple defaults.
For example:

```lua
schema = {
    type = "object",
    required = true,
    props = {
        path = { type = "string", required = true },
        state = { type = "enum", values = { "present", "absent" }, default = "present" },
        tags = { type = "list", items = { type = "string" }, default = {} },
        owner = {
            type = "object",
            props = {
                user = { type = "string" },
                group = { type = "string" },
            },
        },
    },
}
```

Manifest-facing objects reject unknown fields, and module object schemas reject
unknown task arguments. This is intentional. A typo in a task argument should
fail instead of being ignored.

For POSIX modes, prefer accepting strings such as `"0644"` in module arguments
and convert them inside the module or a shared helper. Decimal mode values are
hard to read in manifests.

## Requires

`requires` describes host capabilities needed by the module. It is checked by
Rust against the effective backend before schema validation and before apply.

Supported forms:

```lua
requires = { command = "tar" }
requires = { path = "/tmp" }
requires = { env = "HOME" }
requires = { os = "linux" }
requires = { arch = "x86_64" }
requires = { hostname = "web-1" }
requires = { user = "root" }
requires = { group = "root" }
```

Requirements can be composed:

```lua
requires = {
    all = {
        { os = "linux" },
        {
            any = {
                { command = "curl" },
                { command = "wget" },
            },
        },
        { not = { command = "busybox" } },
    },
}
```

Use `requires` for host capability checks. Do not use `validate` to run commands
just to find out whether a command exists.

## Validate

`validate(ctx, args)` runs during `wali check` and `wali apply`. It receives a
read/probe-only context.

It may return:

```lua
return nil
return { ok = true }
return { ok = false, message = "explanation" }
```

Using `wali.api` is clearer:

```lua
local api = require("wali.api")

validate = function(ctx, args)
    if args.path == "" then
        return api.result.validation():fail("path must not be empty"):build()
    end
    return nil
end
```

Validation context exposes read/probe helpers only:

```text
ctx.phase
ctx.task
ctx.vars
ctx.run_as
ctx.host.id
ctx.host.transport
ctx.host.facts.*
ctx.host.path.*
ctx.host.fs.metadata
ctx.host.fs.stat
ctx.host.fs.lstat
ctx.host.fs.exists
ctx.host.fs.read
ctx.host.fs.list_dir
ctx.host.fs.walk
ctx.host.fs.read_link
```

Validation context does not expose mutation helpers, command execution, random
helpers, or sleep helpers.

Keep validation deterministic. It should answer whether the task is well-formed
and safe to attempt, not perform the task early.

## Apply

`apply(ctx, args)` runs only during `wali apply`. It receives the full context
and returns an `ExecutionResult`-compatible table.

The simplest approach is to return executor filesystem results directly:

```lua
apply = function(ctx, args)
    return ctx.host.fs.create_dir(args.path, { recursive = true })
end
```

For composed operations, use `wali.api`:

```lua
local api = require("wali.api")

apply = function(ctx, args)
    local result = api.result.apply()
    result:merge(ctx.host.fs.create_dir(args.dir, { recursive = true }))
    result:merge(ctx.host.fs.write(args.file, args.content, { create_parents = true }))
    return result:build()
end
```

A result has this shape:

```lua
return {
    changes = {
        { kind = "created", subject = "fs_entry", path = "/tmp/example" },
        { kind = "unchanged", subject = "fs_entry", path = "/tmp/example.conf" },
    },
    message = "optional human summary",
    data = { optional = "machine-readable data" },
}
```

Common change kinds are:

```text
created
updated
removed
unchanged
```

Current subjects include:

```text
fs_entry
command
```

## Host filesystem API

Important read/probe helpers:

```lua
ctx.host.fs.stat(path)      -- follows symlinks
ctx.host.fs.lstat(path)     -- does not follow symlinks
ctx.host.fs.metadata(path, { follow = true })
ctx.host.fs.exists(path)
ctx.host.fs.read(path)
ctx.host.fs.read_link(path)
ctx.host.fs.list_dir(path)
ctx.host.fs.walk(path, { include_root = true, order = "pre" })
```

Important mutation helpers available only during apply:

```lua
ctx.host.fs.write(path, content, opts)
ctx.host.fs.copy_file(src, dest, opts)
ctx.host.fs.create_dir(path, opts)
ctx.host.fs.remove_file(path)
ctx.host.fs.remove_dir(path, opts)
ctx.host.fs.chmod(path, mode)
ctx.host.fs.chown(path, owner)
ctx.host.fs.rename(old_path, new_path)
ctx.host.fs.symlink(target, link_path)
ctx.host.fs.mktemp(opts)
```

Use `lstat` when your module owns the path itself. Use `stat` when your module
intentionally wants the symlink target.

`walk` returns lstat-style metadata. Use `order = "pre"` for parent-before-child
planning and `order = "post"` for child-before-parent planning.

## Command execution

Command execution is available during apply:

```lua
ctx.host.cmd.exec({ program = "systemctl", args = { "reload", "nginx" } })
ctx.host.cmd.shell("printf '%s\n' hello")
```

Prefer `exec` with explicit `program` and `args` for user-controlled values. Use
`shell` only when shell features are actually needed.

If a module requires an external command, declare it in `requires`:

```lua
requires = { command = "tar" }
```

## Path handling

Use host path helpers instead of manual string concatenation:

```lua
ctx.host.path.join(root, relative)
ctx.host.path.parent(path)
ctx.host.path.normalize(path)
```

For destructive operations, normalize and reject unsafe paths explicitly. At
minimum, reject empty path, `/`, `.`, and `..` when removing paths or using tree
destinations.

## Idempotence

A desired-state module should report `unchanged` when the host already matches
the requested state.

Avoid modules that report `updated` every time simply because they called a
command. If a module must be imperative, expose guard options such as `creates`
or `removes`, like `wali.builtin.command` does.

Good idempotence rules:

- inspect existing state before mutating;
- skip writes when content already matches;
- skip chmod/chown when metadata already matches;
- compare existing symlink target before replacing it;
- preflight predictable tree conflicts before the first mutation.

## Error style

Use validation failures for bad input and `error(...)` for unexpected apply-time
failures.

Messages should identify the field or path involved:

```lua
return api.result.validation():fail("path must be absolute"):build()
error("destination already exists and replace is false: " .. args.dest)
```

Do not hide host operation errors. Let executor errors propagate unless the
module can safely recover.

## Testing custom modules

Use the CLI layers while developing:

```sh
wali plan manifest.lua
wali check manifest.lua
wali apply manifest.lua
# one more time
wali apply manifest.lua
```

The second apply should usually report unchanged.

For project tests, prefer black-box integration tests using isolated temporary
directories. They catch mistakes across manifest loading, module loading, schema
normalization, Lua phases, executor behavior, and report JSON.

## When to create a builtin module

A module belongs in `wali.builtin.*` only when it is generally useful and has a
clear safety contract.

Before adding a builtin, define:

- exact desired state;
- validation rules;
- idempotence behavior;
- symlink behavior;
- destructive behavior and required opt-ins;
- expected structured changes;
- integration tests for changed and unchanged runs;
- at least one negative test for an unsafe or invalid case.
