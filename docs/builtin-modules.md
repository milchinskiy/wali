# Builtin modules

Builtin task modules use the reserved `wali.builtin.*` namespace. The builtin
surface is intentionally imperative: module names are verbs, and modules do not
expose declarative `state` switches. To remove something, use
`wali.builtin.remove`.

The current builtin set is:

- touch
- mkdir
- write
- link
- copy
- push
- pull
- remove
- permissions
- command

Common option names are reused across modules:

- `path`: one target-host path.
- `src`: source path or symlink target.
- `dest`: destination path.
- `parents`: create missing parent directories where applicable.
- `replace`: when false, an occupied destination skips the task.
- `recursive`: operate on a directory tree.
- `max_depth`: recursive traversal limit; ignored when `recursive` is false.
- `symlinks`: recursive symlink policy: `preserve`, `skip`, or `error`.
- `skip_special`: skip sockets, devices, FIFOs, and other special entries.
- `mode` / `owner`: metadata for one path or as recursive defaults.
- `dir_mode` / `file_mode`, `dir_owner` / `file_owner`: recursive overrides.

## `wali.builtin.touch`

Creates a regular file if it is absent. Existing regular-file content is never
replaced.

```lua
m.task("touch marker")("wali.builtin.touch", {
    path = "/var/lib/app/initialized",
    parents = true,
    mode = "0644",
})
```

Options: `path`, `parents`, `mode`, `owner`.

## `wali.builtin.mkdir`

Creates a directory. Existing directories are accepted and metadata is
reconciled. Existing non-directories fail.

```lua
m.task("create config dir")("wali.builtin.mkdir", {
    path = "/home/alice/.config/nvim",
    parents = true,
    mode = "0755",
})
```

Options: `path`, `parents`, `mode`, `owner`.

## `wali.builtin.write`

Writes text to a target-host regular file. The source is exactly one of
`content` or controller-side `src`. When task variables or `vars` are present,
the text is rendered through MiniJinja before writing; otherwise it is written
verbatim.

```lua
m.task("write config")("wali.builtin.write", {
    dest = "/etc/app.conf",
    content = "port = {{ port }}\n",
    vars = { port = 8080 },
    parents = true,
    mode = "0644",
})
```

```lua
m.task("write config from template")("wali.builtin.write", {
    src = "templates/app.conf",
    dest = "/etc/app.conf",
    vars = { port = 8080 },
    parents = true,
})
```

Options: `src`, `content`, `dest`, `vars`, `parents`, `replace`, `mode`,
`owner`.

## `wali.builtin.link`

Creates one symlink, or recursively mirrors a source directory as directories
and symlinks.

```lua
m.task("link bashrc")("wali.builtin.link", {
    src = "/home/alice/dotfiles/bashrc",
    dest = "/home/alice/.bashrc",
    parents = true,
    replace = true,
})
```

```lua
m.task("link config tree")("wali.builtin.link", {
    src = "/home/alice/dotfiles/config",
    dest = "/home/alice/.config",
    recursive = true,
    replace = true,
    skip_special = true,
})
```

In non-recursive mode, `src` is symlink target text and does not need to exist.
In recursive mode, `src` must be an absolute target-host directory.

Options: `src`, `dest`, `parents`, `replace`, `recursive`, `skip_special`,
`max_depth`, `dir_mode`, `dir_owner`.

## `wali.builtin.copy`

Copies on the target host. Without `recursive`, the source must be a regular
file. With `recursive = true`, the source must be a directory.

```lua
m.task("copy config")("wali.builtin.copy", {
    src = "/etc/app/default.conf",
    dest = "/etc/app/app.conf",
    parents = true,
    replace = true,
})
```

```lua
m.task("copy skeleton")("wali.builtin.copy", {
    src = "/opt/app/skel",
    dest = "/var/lib/app",
    recursive = true,
    symlinks = "preserve",
    skip_special = true,
})
```

Options: `src`, `dest`, `parents`, `replace`, `recursive`, `preserve_mode`,
`preserve_owner`, `symlinks`, `skip_special`, `max_depth`, `mode`, `owner`,
`dir_mode`, `file_mode`, `dir_owner`, `file_owner`.

## `wali.builtin.push`

Transfers from the controller to the target host. Without `recursive`, the
controller source must be a regular file. With `recursive = true`, the source
must be a controller-side directory.

```lua
m.task("push config")("wali.builtin.push", {
    src = "files/app.conf",
    dest = "/etc/app.conf",
    parents = true,
    replace = true,
})
```

```lua
m.task("push files")("wali.builtin.push", {
    src = "files/app",
    dest = "/opt/app/files",
    recursive = true,
    symlinks = "preserve",
})
```

Options: `src`, `dest`, `parents`, `replace`, `recursive`, `preserve_mode`,
`symlinks`, `skip_special`, `max_depth`, `mode`, `owner`, `dir_mode`,
`file_mode`, `dir_owner`, `file_owner`.

## `wali.builtin.pull`

Transfers from the target host to the controller. Without `recursive`, the
source must be a regular target-host file. With `recursive = true`, the source
must be a target-host directory.

```lua
m.task("pull generated config")("wali.builtin.pull", {
    src = "/etc/app.conf",
    dest = "captured/app.conf",
    parents = true,
})
```

Options: `src`, `dest`, `parents`, `replace`, `recursive`, `preserve_mode`,
`symlinks`, `skip_special`, `max_depth`, `mode`, `dir_mode`, `file_mode`.

## `wali.builtin.remove`

Removes a target-host path. Missing paths are unchanged. Non-empty directories
require `recursive = true`.

```lua
m.task("remove stale tree")("wali.builtin.remove", {
    path = "/tmp/old-app",
    recursive = true,
})
```

Options: `path`, `recursive`.

## `wali.builtin.permissions`

Reconciles mode and/or owner metadata on an existing target-host path or tree.
It does not create or remove filesystem entries.

```lua
m.task("fix permissions")("wali.builtin.permissions", {
    path = "/var/lib/app",
    recursive = true,
    dir_mode = "0755",
    file_mode = "0644",
    owner = { user = "app" },
    symlinks = "skip",
})
```

Options: `path`, `follow`, `expect`, `recursive`, `symlinks`, `skip_special`,
`max_depth`, `mode`, `owner`, `dir_mode`, `file_mode`, `dir_owner`,
`file_owner`.

## `wali.builtin.command`

Runs a guarded command or shell script on the target host.

```lua
m.task("rebuild cache")("wali.builtin.command", {
    script = "appctl rebuild-cache",
    creates = { "/var/lib/app/cache.db", "/var/lib/app/cache.idx" },
    timeout = "30s",
})
```

Exactly one of `program` or `script` is required. `creates` and `removes` may be
a string or list of strings. `creates` skips when all listed paths exist;
`removes` skips when all listed paths are absent. `changed = false` reports a
successful command run as unchanged.

Options: `program`, `args`, `script`, `cwd`, `env`, `stdin`, `timeout`, `pty`,
`creates`, `removes`, `changed`.
