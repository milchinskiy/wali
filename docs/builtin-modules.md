# Builtin modules

Builtin modules live under the reserved `wali.builtin.*` namespace. User modules
should not use this namespace.

The builtin module philosophy is:

- builtin modules describe desired state whenever possible;
- low-level host operations remain available through `ctx.host.*`, but builtin
  modules should expose stable resources rather than syscall-shaped wrappers;
- each builtin module must be idempotent by default;
- each builtin module must return a structured `ExecutionResult` with concrete
  changes;
- shared Lua behavior belongs in `wali.builtin.lib`, not duplicated across
  modules.

## Naming note: `link` versus `copy_file`

`wali.builtin.link` manages one symbolic-link path. It is intentionally named
`link`, not `link_file`, because a symlink target may be a file, directory,
missing path, or any other path string; the module owns the link path itself,
not the target kind.

`wali.builtin.link_tree` applies the same idea to a tree: destination
directories are created, while non-directory source entries are represented as
symlinks.

`wali.builtin.copy_file` is explicitly file-scoped because the source must be
an existing regular file. `wali.builtin.copy_tree` composes that file primitive
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
- destination directories and special entries are refused;
- an existing identical regular file is unchanged unless requested metadata must
  be updated;
- `mode` overrides `preserve_mode` when both are provided.

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

## `wali.builtin.walk`

Inspects a filesystem tree and returns deterministic `ctx.host.fs.walk(...)`
output as structured result data. This module does not mutate the host; it is
intended for validating traversal behavior before implementing tree mutation
modules.

```lua
{
    id = "inspect demo tree",
    module = "wali.builtin.walk",
    args = {
        path = "/tmp/wali-demo",
        include_root = true,
        order = "pre",
    },
}
```

`order` may be `"pre"`, `"post"`, or `"native"`. The task result is always
unchanged and includes `data.entries` in JSON reports.

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
- destination file/symlink paths may be replaced only when `replace = true`;
- extra destination entries are not pruned.

`dir_mode` / `file_mode` override source modes. Without overrides,
`preserve_mode = true` preserves mode bits from the source entries.
`preserve_owner = true` applies numeric source uid/gid to destination entries and
therefore usually requires suitable privileges.

## `wali.builtin.command`

Runs an explicitly imperative command or shell script. Use `creates` or
`removes` guards when the command can be made idempotent.

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

`changed = "never"` can be used for read-only commands.

## Executor tree walk foundation

The host filesystem API now exposes:

```lua
ctx.host.fs.walk(path, {
    include_root = false,
    max_depth = nil,
    order = "pre",
})
```

`order` may be `"pre"`, `"post"`, or `"native"`. The default is `"pre"`,
which returns deterministic parent-before-child order. Use `"post"` when a
caller needs child-before-parent order, for example deletion planning. Use
`"native"` only when debugging backend traversal behavior.

It returns entries with lstat-style metadata:

```lua
{
    path = "/absolute/or/target/path",
    relative_path = "path/relative/to/root",
    depth = 1,
    kind = "file" | "dir" | "symlink" | "other",
    link_target = nil,
    metadata = {
        kind = "file" | "dir" | "symlink" | "other",
        size = 123,
        link_target = nil,
        uid = 0,
        gid = 0,
        mode = 420,
        accessed_at = 1710000000.0,
        modified_at = 1710000000.0,
        changed_at = 1710000000.0,
        created_at = nil,
    },
}
```

`ctx.host.fs.stat(path)` follows symlinks. `ctx.host.fs.lstat(path)` and
`ctx.host.fs.walk(...)` inspect the path itself and never follow symlinks.

The walk primitive is intentionally separate from tree modules; `copy_tree`,
`link_tree`, and archive-style modules should be designed on top of this
traversal contract instead of shelling out directly to `cp -a` or `find` inside
each module.
