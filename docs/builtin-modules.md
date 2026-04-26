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

## `wali.builtin.link`

Ensures a symbolic link exists or is absent.

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

`expect` may be `"any"`, `"file"`, or `"dir"`. The module refuses symlinks
for now because the executor API currently uses lstat-style metadata while
POSIX chmod/chown symlink-follow behavior is platform-sensitive. Follow/no-follow
semantics should be added explicitly later rather than guessed.

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
})
```

It returns entries with:

```lua
{
    path = "/absolute/or/target/path",
    relative_path = "path/relative/to/root",
    kind = "file" | "dir" | "symlink" | "other",
}
```

The walk does not follow symlinks. This primitive is intentionally separate
from tree modules; `copy_tree`, `link_tree`, and archive-style modules should
be designed on top of this traversal contract instead of shelling out directly
to `cp -a` or `find` inside each module.
