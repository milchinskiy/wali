# Changelog

All notable user-facing changes should be recorded here.

This project uses pre-1.0 semantic versioning. Patch releases should remain
compatible within the current pre-1.0 release line where practical, but
manifest, module, and state-file contracts may still evolve before 1.0.

## 0.2.0

### Changed

- Reworked builtin task modules around imperative verbs instead of declarative
  resource-style modules. The public builtin surface is now `touch`, `mkdir`,
  `write`, `link`, `copy`, `push`, `pull`, `remove`, `permissions`, and
  `command`.
- Collapsed file and tree variants into flexible verb modules: `link`, `copy`,
  `push`, `pull`, and `permissions` can now operate on either one path or a
  recursive tree where that operation supports recursion.
- Replaced public `create_parents` builtin options with the shorter, consistent
  `parents` option. Lower-level custom-module APIs still use their existing
  option names.
- `replace = false` in builtin destination-writing modules now prevents
  destructive replacement without hiding satisfied work: matching destinations
  report unchanged, conflicting single-path destinations skip the task, and
  conflicting recursive leaves are skipped while the module continues with
  remaining entries.
- Recursive-only options such as `max_depth` are now ignored when
  `recursive = false`, while their value types and ranges are still validated.
- `wali.builtin.write` now handles both inline text and controller-side source
  files, and renders through MiniJinja automatically when effective variables
  are present.
- `wali.builtin.command` now uses boolean `changed`, and `creates` / `removes`
  may be either a single absolute path or a proper list of absolute paths. Guard
  hits are reported as skipped tasks; map-shaped guard tables are rejected.
- Module apply results may now explicitly skip a task with
  `require("wali.api").result.skip(reason)`.
- Custom modules can now inspect and enforce runtime compatibility with
  `require("wali")`, including `wali.version`, `wali.compatible(requirement)`,
  and `wali.require_version(requirement, label)`.
- LuaLS stubs, examples, builtin docs, manifest docs, module contract docs, and
  the release smoke test were updated for the new builtin contract.
- Recursive `link`, `copy`, `push`, and `pull` now preflight destination kind
  conflicts before mutating when `replace = true`, preserving all-or-error
  behavior for structural conflicts.

### Removed

- Removed the old public builtin modules `dir`, `file`, `template`, `copy_file`,
  `copy_tree`, `link_tree`, `push_file`, `push_tree`, `pull_file`, and
  `pull_tree`.
- Removed declarative builtin `state` options from the public builtin API. Use
  operation-specific modules and `wali.builtin.remove` for deletion.

## 0.1.2

### Added

- `wali.builtin.push_tree` for transferring a controller-side directory tree to
  a target-host directory. Relative controller `src` paths resolve against
  manifest `base_path`; `dest` remains an absolute target-host path.
- `wali.builtin.pull_tree` for transferring a target-host directory tree to a
  controller-side directory. `src` remains an absolute target-host path;
  relative controller `dest` paths resolve against manifest `base_path`.
- Apply-phase `ctx.transfer.push_tree(...)` and `ctx.transfer.pull_tree(...)`
  helpers for custom modules.
- LuaLS stubs for the new tree transfer modules and transfer helper option
  tables.
- `manifest.here(...)` helper for building absolute controller paths relative to
  the manifest directory, useful for localhost-only manifests that need an
  absolute target-host path such as `link_tree.src`.

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
