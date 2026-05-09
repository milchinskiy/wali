# Changelog

All notable user-facing changes should be recorded here.

This project uses pre-1.0 semantic versioning. Patch releases should remain
compatible within the documented 0.1 contract where practical, but manifest,
module, and state-file contracts may still evolve before 1.0.

## 0.1.2

### Added

- `wali.builtin.push_tree` for transferring a controller-side directory tree to
  a target-host directory. Relative controller `src` paths resolve against
  manifest `base_path`; `dest` remains an absolute target-host path.
- `wali.builtin.pull_tree` for transferring a target-host directory tree to a
  controller-side directory. `src` remains an absolute target-host path; relative
  controller `dest` paths resolve against manifest `base_path`.
- Apply-phase `ctx.transfer.push_tree(...)` and `ctx.transfer.pull_tree(...)`
  helpers for custom modules.
- LuaLS stubs for the new tree transfer modules and transfer helper option
  tables.

### Changed

- Builtin documentation now explicitly separates controller-side transfer paths
  from target-host filesystem paths for tree operations.
- `wali.builtin.pull_tree` validation now matches `pull_file`: `check` validates
  path shape and controller destination conflicts, while target source existence
  and kind are verified during apply so a preceding task can create the source
  tree.
- Controller-side writes from pull transfers are reported as
  `controller_fs_entry` changes instead of target-host `fs_entry` changes, so
  state cleanup cannot accidentally remove same-named paths on remote hosts.

## 0.1.1

### Added

- LuaLS definition files under `types/` for manifest authoring, custom module
  development, builtin module argument tables, `wali.api`, and
  `wali.builtin.lib`.
- LuaLS module stubs for builtin task modules, so direct imports such as
  `require("wali.builtin.file")` have a typed `WaliModule<...>` shape in
  editors.
- Builtin-aware `manifest.task(...)` typing so editor completion can suggest
  builtin module argument shapes.
- `.luarc.example.json` with a minimal LuaLS configuration for the repository
  stubs and Wali's global `null` sentinel.

### Changed

- Release archives and Cargo packages now include the LuaLS stubs and example
  LuaLS configuration.
- `scripts/install.sh` installs LuaLS stubs to
  `${XDG_DATA_HOME:-$HOME/.local/share}/wali/types` by default. Use
  `WALI_TYPES_DIR` to choose a different destination or `WALI_INSTALL_TYPES=0`
  to install only the binary.
- CI and release packaging checks now cover the shipped LuaLS contract files.

## 0.1.0

Initial public release.

### Added

- Local and SSH host execution.
- Lua manifests with per-host task expansion.
- Optional `manifest` Lua helper module for compact host and task definitions.
- Strict manifest label validation for host ids, task ids, tags, and `run_as`
  entries.
- `plan`, `check`, `apply`, and explicit state-file based `cleanup` commands.
- Host and task selectors: `--host`, `--host-tag`, `--task`, and `--task-tag`.
- Per-host concurrency control with `--jobs`.
- Optional atomic apply state snapshots with `apply --state-file FILE`.
- Host-aware `when` predicates and module `requires` checks.
- Task dependencies through `depends_on` and change-gated `on_change`.
- Manifest, host, and task variables exposed through `ctx.vars`.
- Custom Lua modules loaded from local directories or Git repositories.
- Namespaced module sources and deterministic effective module selection.
- System-`git` based module source fetch with cache locking and timeouts.
- Read/probe validation context and full apply context for custom modules.
- Builtin modules: `command`, `copy_file`, `copy_tree`, `dir`, `file`, `link`,
  `link_tree`, `permissions`, `pull_file`, `push_file`, `remove`, `template`,
  and `touch`.
- Public Lua helper APIs for controller filesystem reads, host filesystem
  operations, command execution, path handling, JSON, Base64, SHA-256, MiniJinja
  template rendering, and controller/host file transfer.
- Human, plain-text, and JSON output modes.
