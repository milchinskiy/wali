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
            timeout = "5m",
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
- `timeout` is optional, uses the same human duration syntax as command
  timeouts, must be greater than zero when set, and defaults to `5m`;
- `submodules = true` materializes submodules with
  `git submodule update --init --recursive --force` after checkout.

Git module sources are cached under `$WALI_MODULES_CACHE` when set. Otherwise
wali uses `$XDG_DATA_HOME/wali/modules`, falling back to
`~/.local/share/wali/modules`.

Checkout identity is based on the Git URL, ref, and submodule materialization
mode. The manifest namespace, repository leaf name, `git.path`, `depth`, and
`timeout` are not checkout identity. `git.path` only selects the include root
inside the checkout; `depth` only changes how the requested ref is fetched;
`timeout` only bounds the system `git` processes used during preparation.

`check` and `apply` hold a process-level cache lock for every Git source until
execution finishes. This prevents another wali process from resetting or
cleaning the same checkout while task runtimes are loading module files.

Every system `git` process is run with `GIT_TERMINAL_PROMPT=0`, null stdin, and
a bounded timeout. Git stdout and stderr are captured through temporary files,
not pipe-reader threads, so inherited output handles from helpers or grandchild
processes cannot block timeout return. A timeout kills the Git child process,
waits for it, and fails source preparation with a `Module source error` that
names the timed-out Git command.

Pin a commit for reproducible module code. Branch names are intentionally
mutable because they are resolved by Git at fetch time.

## Minimal module example

A module file returns one Lua table:

```lua
local lib = require("wali.builtin.lib")

return {
    name = "example file",
    description = "writes one file",

    schema = {
        type = "object",
        required = true,
        props = {
            path = { type = "string", required = true },
            content = { type = "string", required = true },
            mode = lib.schema.mode("0644"),
        },
    },

    validate = function(ctx, args)
        if args.path == "/" then
            return lib.validation_error("path must not be /")
        end
        return nil
    end,

    apply = function(ctx, args)
        return ctx.host.fs.write(args.path, args.content, {
            create_parents = true,
            mode = lib.mode_bits(args.mode),
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

## Shared helper library

Custom modules may import `wali.builtin.lib` when they want the same small
helper surface used by the builtin modules:

```lua
local lib = require("wali.builtin.lib")
```

The helper library is intentionally plain Lua. It does not hide host operations;
it provides reusable validation, schema fragments, result builders, and common
filesystem policies. The most useful public helpers are:

- `lib.result.apply()` and `lib.result.validation()` builders, plus
  `lib.validation_ok()` and `lib.validation_error(message)`;
- `lib.schema.mode()` and `lib.schema.owner()` schema fragments for manifest
  fields that later become executor mode/owner option tables;
- `lib.mode_bits("0644")`, `lib.owner(table)`, `lib.validate_mode_owner(args)`,
  `lib.mode_owner_opts(args)`, and
  `lib.apply_mode_owner(ctx, result, path, args)`;
- `lib.validate_absolute_path(ctx, path, field)`,
  `lib.validate_safe_remove_path(ctx, path)`, and
  `lib.validate_tree_roots(ctx, src, dest)` for common path-safety checks;
- `lib.output_text(output)`, `lib.status_text(status)`,
  `lib.command_error(output, detail)`, and
  `lib.assert_command_ok(output, detail)` for command modules;
- `lib.is_file(metadata)`, `lib.is_dir(metadata)`, and
  `lib.is_symlink(metadata)` for readable metadata predicates.

Helpers that mutate host state, such as `apply_mode_owner`, `ensure_dir`, and
`ensure_symlink`, explicitly require `ctx.phase == "apply"`. Validation code
should use only read/probe helpers and should return validation results rather
than changing host state.

Owner values accepted by helper validation are either non-empty names or
non-negative numeric ids:

```lua
owner = { user = "root", group = "root" }
owner = { user = 0, group = 0 }
```

POSIX modes are accepted as octal strings in manifests and converted with
`lib.mode_bits` before passing options to `ctx.host.fs.*`:

```lua
local lib = require("wali.builtin.lib")

schema = {
    type = "object",
    required = true,
    props = {
        path = { type = "string", required = true },
        mode = lib.schema.mode(),
        owner = lib.schema.owner(),
    },
}

validate = function(_, args)
    return lib.validate_mode_owner(args)
end

apply = function(ctx, args)
    return ctx.host.fs.write(args.path, "managed\n", {
        create_parents = true,
        mode = lib.mode_bits(args.mode),
        owner = lib.owner(args.owner),
    })
end
```

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

Manifest-facing objects, Lua host API option tables, module result tables, and
module schema definitions reject unknown fields. Module object schemas reject
unknown task arguments. A typo should fail instead of being ignored.

For POSIX modes, prefer strings such as `"0644"` in module arguments and convert
them inside the module or a shared helper. Decimal mode values are hard to read
in manifests.

## Task dependencies and `on_change`

`depends_on` and `on_change` are host-local task references. Both forms order
the current task after the referenced source tasks, and selecting a task by id
or tag includes both normal dependencies and change-gated source tasks.

`depends_on` is the ordinary success gate: the current task runs only when every
listed dependency succeeded. `on_change` is a success-and-change gate during
`apply`: the current task runs only when every listed source succeeded and at
least one listed source reported a changed execution result. If all `on_change`
sources were unchanged, wali reports the gated task as skipped. During `check`,
`on_change` still orders and validates the gated task because no apply-time
change result exists yet.

Do not list the same source in both `depends_on` and `on_change`. Duplicate
references, self-references, unknown task ids, and references to tasks not
scheduled for the same host are rejected.

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

## Task `when` predicates

A task may declare `when` when the decision to run the task depends on host
facts or cheap host probes. Wali evaluates `when` after connecting to the host
and before module `requires`, schema normalization, `validate`, or `apply`.

```lua
{
    id = "install optional config",
    when = {
        all = {
            { os = "linux" },
            { path_dir = "/etc" },
            { command_exist = "systemctl" },
            { ["not"] = { env_set = "WALI_SKIP_SYSTEMD_TASKS" } },
        },
    },
    module = "wali.builtin.file",
    args = { path = "/tmp/example.conf", content = "managed\n" },
}
```

Supported predicates:

```lua
when = { os = "linux" }
when = { arch = "x86_64" }
when = { hostname = "web-1" }
when = { user = "root" }
when = { group = "root" }
when = { env = { "NAME", "value" } }
when = { env_set = "NAME" }
when = { path_exist = "/path" }
when = { path_file = "/path" }
when = { path_dir = "/path" }
when = { path_symlink = "/path" }
when = { command_exist = "tar" }
```

Predicates can be composed with non-empty `all` and `any` lists and a unary
`not` predicate. Because `not` is a Lua keyword, quote it as a table key:

```lua
when = {
    any = {
        { command_exist = "curl" },
        { command_exist = "wget" },
    },
}

when = { ["not"] = { env_set = "DISABLE_TASK" } }
```

`path_file` and `path_dir` follow symlinks, matching ordinary `stat` behavior.
`path_symlink` inspects the path itself and therefore matches symlinks even when
the link target is missing. Empty `all`/`any` lists and empty string predicate
arguments are rejected during manifest validation.

Use task `when` for deployment decisions. Use module `requires` for capabilities
that the module itself needs regardless of who uses it.

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
ctx.controller.path.*
ctx.controller.fs.*
ctx.codec.*
ctx.json.*
ctx.template.*
ctx.transfer.*
```

Validation context does not expose mutation helpers, command execution, random
helpers, or sleep helpers.

`ctx.vars` contains the effective manifest/host/task variables for the current
host task. The merge is shallow and deterministic: manifest variables are the
base, host variables override them, and task variables override both. Modules
should treat `ctx.vars` as read-only configuration data.

`ctx.controller.path` exposes lexical controller path helpers: `resolve`,
`is_absolute`, `join`, `normalize`, `parent`, `basename`, and `strip_prefix`.
`resolve` converts relative paths to manifest `base_path`; the other helpers are
raw lexical path operations, mirroring `ctx.host.path` semantics for module
authors.

`ctx.controller.fs` exposes read-only controller filesystem helpers:
`metadata`, `stat`, `lstat`, `exists`, `read`, `read_text`, `list_dir`,
`walk`, and `read_link`. `walk` returns the same entry shape as
`ctx.host.fs.walk`, does not follow symlinks, and defaults to deterministic
pre-order.

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

Low-level mutation helpers enforce the same safety invariants for builtin and
custom modules. `remove_dir` refuses empty, root, current-directory,
parent-directory, and parent-escaping lexical targets before shell execution.
`rename` is exact-path semantics: an existing directory destination is refused
instead of being treated like a request to move the source inside that
directory.

Use `lstat` when your module owns the path itself. Use `stat` when your module
intentionally wants the symlink target.

`walk` returns lstat-style metadata. Use `order = "pre"` for parent-before-child
planning and `order = "post"` for child-before-parent planning.

## Controller API

`ctx.controller` is available during validation and apply. It is the single
namespace for controller-side path and read-only filesystem access. Use it when
a module needs to inspect or read files from the machine running wali. The name
is intentional: `local` would be ambiguous when the target host also uses local
transport.

```lua
ctx.controller.path.resolve(path)
ctx.controller.path.is_absolute(path)
ctx.controller.path.join(base, child)
ctx.controller.path.normalize(path)
ctx.controller.path.parent(path)
ctx.controller.path.basename(path)
ctx.controller.path.strip_prefix(base, path)

ctx.controller.fs.metadata(path, opts) -- opts.follow defaults to true
ctx.controller.fs.stat(path)
ctx.controller.fs.lstat(path)
ctx.controller.fs.exists(path)
ctx.controller.fs.read(path)      -- raw bytes as a Lua string
ctx.controller.fs.read_text(path) -- UTF-8 text only
ctx.controller.fs.list_dir(path)
ctx.controller.fs.walk(path, opts)
ctx.controller.fs.read_link(path)
```

Controller paths may be absolute or relative. Relative paths are resolved
against manifest `base_path`; a relative `base_path` is resolved from the
manifest directory, and an omitted `base_path` defaults to the manifest
directory. Empty controller paths are rejected. No project-root boundary is
imposed. wali assumes the manifest author controls which controller files may be
read.

The controller filesystem API is intentionally read-only. Controller-side writes
currently happen only through `wali.builtin.pull_file` /
`ctx.transfer.pull_file`, where the transfer operation itself owns the write
semantics.

`metadata` follows symlinks by default, matching `stat`. Use `lstat` or
`metadata(path, { follow = false })` when the module owns the path itself.
`list_dir` returns entries sorted by name for deterministic module behavior.
`walk` returns the same entry shape as `ctx.host.fs.walk`, uses lstat-style
metadata, does not follow symlinks, and supports `include_root`, `max_depth`,
and `order` options.

## Codec API

`ctx.codec` is available during validation and apply. It contains small pure
byte/string codecs for module authors. The first supported codec is standard
padded Base64. It accepts and returns Lua strings, so binary bytes are
preserved.

```lua
ctx.codec.base64_encode(bytes)
ctx.codec.base64_decode(text)
```

`base64_decode` accepts ASCII whitespace in encoded text and rejects malformed
input with a clear error. It does not perform filesystem access, command
execution, or any mutation.

```lua
local encoded = ctx.codec.base64_encode(ctx.controller.fs.read("payload.bin"))
local bytes = ctx.codec.base64_decode(encoded)
```

## JSON API

`ctx.json` is available during validation and apply. It is the primitive JSON
codec for module authors, backed by wali's existing Serde JSON handling. Use it
when reading structured controller files, parsing JSON command output, or
emitting machine-readable result data.

```lua
ctx.json.decode(text)
ctx.json.encode(value)
ctx.json.encode_pretty(value)
```

`decode` expects a Lua string containing UTF-8 JSON text and returns ordinary
Lua values. JSON `null` is represented by wali's global `null` sentinel.
`encode` returns compact JSON; `encode_pretty` returns indented JSON. Functions,
threads, userdata, and other non-JSON Lua values are rejected with a clear
error.

```lua
local cfg = ctx.json.decode(ctx.controller.fs.read_text("config.json"))
local text = ctx.json.encode({ name = cfg.name, optional = null })
```

## Template API

`ctx.template` is available during validation and apply. It is a pure MiniJinja
rendering helper. Controller-side template files should be read through
`ctx.controller.fs.read_text(...)`; this avoids a second controller-file access
contract in the template namespace.

```lua
ctx.template.render(source, vars)
```

`vars` must be an object/table. Rendering is strict: referencing an undefined
variable is an error. A trailing newline in the template source is preserved.
The environment is intentionally minimal: standard Jinja control syntax and
Serde-backed collections are available, but extra MiniJinja builtins, filters,
loaders, macros, and debug features are not part of the wali contract.

## Transfer API

`ctx.transfer` is available during validation and apply. During validation it is
present but exposes no duplicated controller-file validation helpers. During
apply it moves bytes between the wali controller process and the effective
target host backend. Use it when a module needs controller-to-host or
host-to-controller file transfer; use `ctx.host.fs.copy_file(...)` for same-host
copies.

```lua
ctx.transfer.push_file(src, dest, opts)  -- apply phase only
ctx.transfer.pull_file(src, dest, opts)  -- apply phase only
```

`push_file` reads `src` from the controller and writes `dest` on the target
host. `pull_file` reads `src` from the target host and writes `dest` on the
controller.

Controller-side paths may be absolute or relative. Relative controller paths are
resolved against manifest `base_path`. A relative `base_path` is resolved from
the manifest directory, and an omitted `base_path` defaults to the manifest
directory. `base_path` must resolve to an existing directory. No project-root
boundary is imposed: wali assumes the manifest author controls which local files
may be read or written.

`push_file` accepts the same write options as `ctx.host.fs.write(...)`:

```lua
{
    create_parents = true,
    replace = true,
    mode = 420 -- 0644,
    owner = { user = "root", group = "root" },
}
```

`pull_file` accepts local write options only:

```lua
{
    create_parents = true,
    replace = true,
    mode = 384 -- 0600,
}
```

`owner` is not supported for controller-side writes. `pull_file` treats the
controller-side destination path itself as the managed object: with
`replace = true`, an existing local symlink is replaced by a regular file; with
`replace = false`, an existing local symlink is preserved unchanged. Symlinks to
directories are refused.

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

If a command request omits `timeout`, Wali uses the host-level `command_timeout`
default when it is configured. The same host default bounds the initial fact
probe performed during connection. An explicit request timeout always overrides
the host default.

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

Use host path helpers instead of manual string concatenation or fragile prefix
checks:

```lua
ctx.host.path.join(root, relative)
ctx.host.path.normalize(path)
ctx.host.path.parent(path)
ctx.host.path.is_absolute(path)
ctx.host.path.basename(path)
ctx.host.path.strip_prefix(base, path)
```

`strip_prefix(base, path)` is lexical, normalized, and segment-aware. It returns
the relative suffix when `path` is exactly `base` or below `base`, and returns
`nil` otherwise:

```lua
ctx.host.path.strip_prefix("/tmp/app", "/tmp/app/file")  -- "file"
ctx.host.path.strip_prefix("/tmp/app", "/tmp/app")       -- "."
ctx.host.path.strip_prefix("/tmp/app", "/tmp/app2/file") -- nil
```

That makes containment checks a one-liner without unsafe string-prefix logic:

```lua
local inside = ctx.host.path.strip_prefix(parent, candidate) ~= nil
```

For destructive operations, normalize and reject unsafe paths explicitly. At
minimum, reject empty path, `/`, `.`, and `..` when removing paths or using tree
destinations.

## Idempotence

A module that reconciles filesystem content should report `unchanged` when the
host already matches the requested input.

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
wali check --jobs 1 manifest.lua   # serialize hosts while debugging
wali apply --jobs 4 manifest.lua   # cap host concurrency
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

- exact primitive contract;
- validation rules;
- idempotence behavior;
- symlink behavior;
- destructive behavior and required opt-ins;
- expected structured changes;
- integration tests for changed and unchanged runs;
- at least one negative test for an unsafe or invalid case.
