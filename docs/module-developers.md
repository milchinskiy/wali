# Module developer guide

This document describes how to write custom wali Lua modules against the current
module contract.

The contract is still evolving, but the main boundaries are already important:
`requires` checks host capabilities, `validate` is non-mutating, and `apply`
performs changes.

## Module location

A manifest can add module search paths:

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

Do not put custom modules under `wali.builtin.*`. That namespace is reserved for
modules shipped with wali.

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

Object schemas reject unknown fields. This is intentional. A typo in a task
argument should fail instead of being ignored.

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
