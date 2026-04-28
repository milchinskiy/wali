# Module developer guide

This is the canonical guide for custom Lua modules and custom module sources.
The short README examples intentionally omit most edge cases; this document
keeps the detailed authoring and source-loading contract in one place.

## Module source contract

A manifest may add one or more module sources. A source is either a local path
or a Git repository; it may also have an optional manifest-local namespace.

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
        },
    },
}
```

Each source exposes Lua files below its include root. For a local source the
include root is the resolved `path`. For a Git source the include root is the
checkout root plus optional `git.path`.

Module filenames map to dotted Lua module names:

```text
file.lua                 -> file
file/init.lua            -> file
internal/utils/tool.lua  -> internal.utils.tool
```

`file.lua` and `file/init.lua` in the same source are ambiguous and are
rejected.

## Namespaces and task module names

A namespace is a public selector chosen by the manifest author. It is not a Git
cache key and it is not part of the module author's internal import paths.

Given this source:

```lua
modules = {
    { namespace = "repo_1", path = "./modules" },
}
```

this task:

```lua
{
    id = "run custom module",
    module = "repo_1.example_file",
    args = {},
}
```

resolves to source `repo_1` and loads the source-local Lua module
`example_file`.

Every task gets a fresh one-shot Lua runtime. Wali adds only the effective
source root to that runtime's `package.path`, then loads the source-local module
name. Internal imports therefore stay ordinary and source-local:

```lua
local tool = require("internal.utils.tool")
```

Two repositories may contain the same internal tree, including the same
`internal/utils/tool.lua`, as long as tasks select them through different
namespaces.

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
- unnamespaced sources are allowed for simple local workflows, but if more than
  one unnamespaced source provides the requested module name, wali fails instead
  of choosing by `package.path` order.

Custom source roots must not contain a top-level `wali.lua` or `wali/` tree.
That package prefix is reserved for wali's own APIs and builtins.

## Local source rules

Local paths are resolved relative to the manifest file unless they are absolute.
They must resolve to existing directories during manifest loading.

Because wali intentionally uses native Lua `package.path` for the selected
source, local source paths must be representable as Lua package-path templates.
Paths containing `;` or `?` are rejected.

## Git source rules

Git sources are fetched with the system `git` executable before `wali check` and
`wali apply`. `wali plan` does not fetch Git sources.

Git fields:

- `url` is required and must not be empty, start with `-`, contain control
  characters, contain surrounding whitespace, or embed HTTP(S) credentials;
- `ref` is required and must be a branch, tag, full ref, or commit accepted by
  `git fetch origin <ref>`;
- `path` is optional, relative to the checkout root, and must not contain parent
  directory components;
- `depth` is optional and must be greater than zero when set;
- `submodules = true` materializes submodules with
  `git submodule update --init --recursive --force` after checkout.

Git module sources are cached under `$WALI_MODULES_CACHE` when set. Otherwise
wali uses `$XDG_DATA_HOME/wali/modules`, falling back to
`~/.local/share/wali/modules`.

Checkout identity is based on the Git URL, ref, and submodule materialization
mode. The manifest namespace, repository leaf name, `git.path`, and `depth` are
not checkout identity. `git.path` only selects the include root inside the
checkout; `depth` only changes how the requested ref is fetched.

`check` and `apply` hold a process-level cache lock for every Git source until
execution finishes. This prevents another wali process from resetting or
cleaning the same checkout while task runtimes are loading module files.

Pin a commit for reproducible module code. Branch names are intentionally
mutable because they are resolved by Git at fetch time.

## Minimal module example

A module file returns one Lua table:

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

`apply` is required for a module used by `wali apply`. `requires`, `schema`, and
`validate` are optional, but serious modules should normally use all three.

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

Use schemas to catch wrong argument types early and to apply simple defaults:

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

Manifest-facing objects, Lua host API option tables, and module result tables
reject unknown fields. Module object schemas reject unknown task arguments. A
typo should fail instead of being ignored.

For POSIX modes, prefer strings such as `"0644"` in module arguments and convert
them inside the module or a shared helper. Decimal mode values are hard to read
in manifests.

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

- created
- updated
- removed
- unchanged

Current subjects include:

- fs_entry
- command

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
ctx.host.cmd.exec({
    program = "systemctl",
    args = { "reload", "nginx" },
    env = { FOO = "bar" },
    timeout = "10s",
})
ctx.host.cmd.shell("printf '%s\n' hello")
ctx.host.cmd.shell({ script = "printf '%s\n' hello", timeout = "10s" })
```

Prefer `exec` with explicit `program` and `args` for user-controlled values. Use
`shell` only when shell features are actually needed. Command request tables
reject unknown fields. Environment variables are passed as a string map and
names must match `[A-Za-z_][A-Za-z0-9_]*`. Empty programs, empty shell scripts,
and zero-duration timeouts are rejected.

Command output uses split streams by default:

```lua
local out = ctx.host.cmd.exec({ program = "sh", args = { "-c", "printf out; printf err >&2" } })
-- out.stdout == "out"
-- out.stderr == "err"
-- out.output == nil
```

When PTY mode is required, stdout and stderr are merged by the terminal and Wali
returns a single combined output field:

```lua
local out = ctx.host.cmd.shell({ script = "printf combined", pty = "require" })
-- out.output == "combined"
-- out.stdout == nil
-- out.stderr == nil
```

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
# run apply again; it should usually be unchanged
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
