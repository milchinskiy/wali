# Builtin modules

Builtin modules live under the reserved `wali.builtin.*` namespace. User modules
should not use this namespace. Unknown `wali.*` task modules are rejected during
manifest/preflight validation.

This document is the module-specific reference for current builtins: accepted
arguments, behavior, and safety notes. The shared Lua phase and custom module
contracts are described in `module-developers.md`; the design rationale is in
`philosophy.md`.

General builtin expectations are stable across this file: prefer desired state,
be idempotent by default, validate unsafe input before mutation, and return
structured `ExecutionResult` changes.

## Naming note: `link` versus `copy_file`

`wali.builtin.link` manages one symbolic-link path. It is intentionally named
`link`, not `link_file`, because a symlink target may be a file, directory,
missing path, or any other path string; the module owns the link path itself,
not the target kind.

`wali.builtin.link_tree` applies the same idea to a tree: destination
directories are created, while non-directory source entries are represented as
symlinks.

`wali.builtin.copy_file` is explicitly file-scoped because the source must be an
existing regular file. `wali.builtin.copy_tree` composes that file primitive
with deterministic tree walking.

## `wali.builtin.dir`

Ensures a directory exists or is absent.

```lua
{
    id = "create config dir",
    module = "wali.builtin.dir",
    args = {
        path = "/etc/example",
        state = "present",
        parents = true,
        mode = "0755",
        owner = { user = "root", group = "root" },
    },
}
```

For removal:

```lua
{
    id = "remove old dir",
    module = "wali.builtin.dir",
    args = {
        path = "/opt/old-example",
        state = "absent",
        recursive = true,
    },
}
```

## `wali.builtin.file`

Ensures a regular file with literal content exists or is absent.

```lua
{
    id = "write motd",
    module = "wali.builtin.file",
    args = {
        path = "/etc/motd",
        content = "managed by wali\n",
        create_parents = false,
        mode = "0644",
        owner = { user = "root", group = "root" },
    },
}
```

Behavior when `state = "present"`:

- destination directories and special entries are refused;
- an existing identical regular file is unchanged unless requested metadata must
  be updated;
- an existing symlink destination is preserved unchanged when `replace = false`;
- an existing symlink destination is replaced with a regular file when
  `replace = true`, even when the symlink target already has identical content;
- a symlink destination that resolves to a directory is refused.

For removal:

```lua
{
    id = "remove old file",
    module = "wali.builtin.file",
    args = {
        path = "/tmp/old-file",
        state = "absent",
    },
}
```

## `wali.builtin.copy_file`

Copies one regular file on the same target host. The copy is performed by the
executor on the host side; file bytes are not routed through Lua.

```lua
{
    id = "copy config template",
    module = "wali.builtin.copy_file",
    args = {
        src = "/opt/example/default.conf",
        dest = "/etc/example.conf",
        create_parents = true,
        replace = true,
        preserve_mode = true,
        owner = { user = "root", group = "root" },
    },
}
```

Behavior:

- `src` must be an existing regular file;
- source symlinks are refused instead of followed;
- destination directories, including symlinks to directories, and special entries
  are refused;
- an existing identical regular file is unchanged unless requested metadata must
  be updated;
- an existing symlink destination is preserved unchanged when `replace = false`;
- an existing symlink destination is replaced with a regular file when
  `replace = true`, even when the symlink target already has identical content;
- `mode` overrides `preserve_mode` when both are provided.

## `wali.builtin.push_file`

Transfers one regular file from the wali controller to the target host.

```lua
{
    id = "push config",
    module = "wali.builtin.push_file",
    args = {
        src = "./files/example.conf",
        dest = "/etc/example/example.conf",
        create_parents = true,
        replace = true,
        mode = "0644",
        owner = { user = "root", group = "root" },
    },
}
```

Behavior:

- `src` is a controller-side path;
- absolute controller paths are used as-is;
- relative controller paths are resolved against manifest `base_path`; a relative
  `base_path` is resolved from the manifest directory, and an omitted
  `base_path` defaults to the manifest directory; `base_path` must resolve to
  an existing directory;
- `src` must resolve to a regular file; `wali check` validates this
  controller-side source before apply;
- `dest` is a target-host path and is written through the effective host
  backend, including `run_as` when configured;
- `create_parents`, `replace`, `mode`, and `owner` match
  `wali.builtin.file` write semantics.

## `wali.builtin.pull_file`

Transfers one regular file from the target host to the wali controller.

```lua
{
    id = "pull log snapshot",
    module = "wali.builtin.pull_file",
    args = {
        src = "/var/log/example/current.log",
        dest = "./logs/current.log",
        create_parents = true,
        replace = true,
        mode = "0600",
    },
}
```

Behavior:

- `src` is a target-host path and is read through the effective host backend;
- `dest` is a controller-side path;
- absolute controller paths are used as-is;
- relative controller paths are resolved against manifest `base_path`; a relative
  `base_path` is resolved from the manifest directory, and an omitted
  `base_path` defaults to the manifest directory; `base_path` must resolve to
  an existing directory;
- an existing identical local regular file is unchanged unless requested mode
  bits must be updated;
- `replace = false` preserves any existing local file or symlink destination and
  reports unchanged;
- `replace = true` replaces an existing local symlink destination with a regular
  file, even when the symlink target already has identical content;
- local destination directories, including symlinks to directories, and special
  entries are refused;
- `owner` is intentionally not supported for local controller writes.

## `wali.builtin.link`

Ensures a symbolic link path exists or is absent.

```lua
{
    id = "link config",
    module = "wali.builtin.link",
    args = {
        path = "/etc/example.conf",
        target = "/opt/example/example.conf",
        replace = false,
    },
}
```

`replace = true` may replace files and symlinks, but it refuses to replace
directories.

## `wali.builtin.remove`

Ensures any filesystem path is absent. Use this when the existing path kind is
not important, or when cleanup code should remove either a file, symlink, or
directory. It is idempotent: an already absent path is reported as unchanged.

```lua
{
    id = "remove stale path",
    module = "wali.builtin.remove",
    args = {
        path = "/tmp/old-example",
        recursive = true,
    },
}
```

Safety rules:

- empty path, `/`, `.`, and `..` are rejected after host path normalization;
- directories require `recursive = true` when they are non-empty;
- special filesystem entries are rejected unless `allow_special = true`;
- symlinks are removed as links, not followed.

## `wali.builtin.touch`

Ensures a regular file exists without replacing existing content. This is useful
for marker files, lock/state files, and files whose content is managed by
another command.

```lua
{
    id = "create marker",
    module = "wali.builtin.touch",
    args = {
        path = "/var/lib/example/initialized",
        create_parents = true,
        mode = "0644",
        owner = { user = "root", group = "root" },
    },
}
```

Behavior:

- absent path creates an empty file;
- existing regular file is left intact;
- existing non-file path is rejected;
- `mode` and `owner` are enforced when provided.

## `wali.builtin.permissions`

Ensures mode and/or owner metadata on an existing file or directory.

```lua
{
    id = "secure config",
    module = "wali.builtin.permissions",
    args = {
        path = "/etc/example.conf",
        expect = "file",
        mode = "0600",
        owner = { user = "root", group = "root" },
    },
}
```

`expect` may be `"any"`, `"file"`, or `"dir"`.

By default, `follow = true`, so a symlink to a file or directory is resolved and
`chmod` / `chown` affect the target, matching normal POSIX command behavior. Set
`follow = false` only when you want to inspect the path itself; the module will
then refuse symlinks because portable no-follow chmod/chown semantics are not
available through the current executor contract.

## `wali.builtin.link_tree`

Mirrors a source directory tree into a destination directory by creating
destination directories and symlinks to source files. This module does not copy
file bytes and does not follow source symlinks; a source symlink is mirrored as
a destination symlink pointing at the source symlink path itself.

```lua
{
    id = "link plugin tree",
    module = "wali.builtin.link_tree",
    args = {
        src = "/opt/example/releases/current/plugins",
        dest = "/var/lib/example/plugins",
        replace = false,
        dir_mode = "0755",
    },
}
```

Safety rules:

Before mutating the destination, the module preflights destination conflicts for
the whole walked source tree. This catches predictable type conflicts before any
directory or symlink is created.

- `src` and `dest` must be absolute paths;
- `/` is refused as either source or destination;
- source and destination must not be nested inside each other;
- destination directories are created or verified;
- destination file/symlink conflicts are refused unless `replace = true`;
- destination directory conflicts are never replaced by links;
- source `other` entries are refused unless `allow_special = true`;
- extra destination entries are not pruned.

## `wali.builtin.copy_tree`

Copies a source directory tree into a destination directory on the same target
host. It is built on deterministic `ctx.host.fs.walk(...)` output plus the
host-side `ctx.host.fs.copy_file(...)` primitive.

```lua
{
    id = "copy plugin tree",
    module = "wali.builtin.copy_tree",
    args = {
        src = "/opt/example/releases/current/plugins",
        dest = "/var/lib/example/plugins",
        replace = true,
        preserve_mode = true,
        preserve_owner = false,
        symlinks = "preserve",
    },
}
```

Safety rules:

Before mutating the destination, the module preflights destination conflicts for
the whole walked source tree. This catches predictable type conflicts before any
directory or symlink is created.

- `src` and `dest` must be absolute paths;
- `/` is refused as either source or destination;
- source and destination must not be nested inside each other;
- source symlinks are not followed;
- `symlinks = "preserve"` recreates the same link text at the destination;
- `symlinks = "skip"` leaves destination symlink paths untouched;
- `symlinks = "error"` refuses source symlinks;
- special source entries are refused unless `skip_special = true`;
- destination directories are created or verified;
- destination directories are never replaced by files or links;
- destination special entries are refused for copied files;
- destination symlinks that resolve to directories or special entries are refused
  during preflight where copied files are expected;
- destination file/symlink paths may be replaced only when `replace = true`;
- extra destination entries are not pruned.

`dir_mode` / `file_mode` override source modes. Without overrides,
`preserve_mode = true` preserves mode bits from the source entries.
`preserve_owner = true` applies numeric source uid/gid to destination entries
and therefore usually requires suitable privileges.

## `wali.builtin.command`

Runs an explicitly imperative command or shell script. Use `creates` or
`removes` guards when the command can be made idempotent. If a command creates
or removes its guard path during a successful run, that filesystem transition is
reported explicitly so state-based cleanup can reason about it.

```lua
{
    id = "initialize database",
    module = "wali.builtin.command",
    args = {
        program = "example-init",
        args = { "--quiet" },
        creates = "/var/lib/example/initialized",
    },
}
```

Shell form:

```lua
{
    id = "run shell script",
    module = "wali.builtin.command",
    args = {
        script = "printf hello > /tmp/example",
        creates = "/tmp/example",
    },
}
```

`timeout` is a human-readable string such as `"10s"` or `"2m"`. When omitted,
the host-level `command_timeout` default is used if configured. `env` is a
string map, for example `{ FOO = "bar" }`. `changed = "never"` can be used for
read-only commands.

## Tree traversal primitive

`wali.builtin.copy_tree` and `wali.builtin.link_tree` are built on
`ctx.host.fs.walk(...)`, the host filesystem traversal primitive exposed to
custom modules. Wali does not provide a separate `wali.builtin.walk` task
module; tree inspection is a module-authoring concern rather than desired state
by itself.

For the full `ctx.host.fs.walk(...)` API contract, see `module-developers.md`.
