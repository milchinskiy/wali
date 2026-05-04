# Project philosophy

This file captures the shape of wali: what belongs in the project, what does
not, and where we draw the line when a feature could grow in several directions.
It is not a second specification. User-facing details live in the README,
`docs/module-developers.md`, and `docs/builtin-modules.md`.

## Goal

wali is a small agentless automation tool for local and SSH hosts. Manifests and
modules are Lua; the engine is Rust.

The project is built around a simple loop:

1. read a manifest;
2. expand tasks per host;
3. validate against real hosts when asked;
4. apply changes when asked;
5. clean up only resources that a previous apply recorded as created.

The source tree should remain readable by one maintainer. Prefer a plain helper,
a small data type, or a direct check over a framework-shaped abstraction. Add a
subsystem only when the current model cannot carry the behavior cleanly.

wali is not trying to be a distributed scheduler, a full configuration
management platform, or a general orchestration framework.

## CLI phases

The main commands have separate responsibilities:

- `plan` compiles the manifest and prints the selected per-host task plan. It
  does not connect to hosts, fetch Git repositories, or validate module input.
- `check` prepares module sources, connects to hosts, evaluates predicates and
  requirements, normalizes arguments, and runs module validation in a read-only
  context.
- `apply` runs the same checks, then calls module `apply` functions with the
  full context.
- `cleanup` reads a successful apply state file and removes only filesystem
  resources recorded as created within the current selected manifest scope.

Host concurrency is a CLI concern. `--jobs N` caps how many hosts run at the
same time; tasks on one host stay sequential. This keeps the per-host execution
model easy to reason about while still allowing useful parallelism.

Selectors narrow the plan before secrets are collected, module sources are
prepared, or workers start. Host selectors are inclusive. Task selectors are
inclusive and bring same-host upstream dependencies, but not downstream tasks.
The selected plan is the same working set for `plan`, `check`, `apply`, and
`cleanup`.

## Boundaries

Keep these layers separate:

- manifest data: what the user wrote;
- plan data: what will run on each host;
- runtime data: live connections, effective backends, events, and results.

A plan is immutable runtime input. Workers consume it; they do not rewrite it.

Dependencies are host-local. Cross-host ordering is outside the current design.
It can be added later only with a concrete use case and clear failure semantics.

## Module sources

A manifest may mount local or Git module sources, with an optional namespace.
The namespace selects a source from the manifest. It is not part of the Git
cache key and module authors do not use it for internal imports.

Each task resolves to one source and runs in a fresh Lua runtime. Wali adds only
that source root to `package.path`, then loads the source-local module name.
Internal imports stay normal Lua:

```lua
local tool = require("internal.utils.tool")
```

Git sources use the system `git`. The cache is an implementation detail, not a
host-state backend. `check` and `apply` hold a cache lock until execution
finishes because Lua files may be loaded after the fetch. Git commands run with
null stdin, terminal prompting disabled, captured output, and a timeout.

`plan` stays offline: no Git fetches, no host access, no network access.

## Lua phases

A Lua module may define three active phases:

1. `requires` checks host capabilities;
2. `validate` checks input and host state without mutation;
3. `apply` mutates host state.

`validate` must remain read-only. If validation can write files or run arbitrary
commands, `check` becomes a weak apply, which defeats the point of the phase.

## Effective backend

All host work goes through an effective backend. Task-level `run_as` must affect
filesystem operations and command execution in the same way. That is why host
filesystem APIs are executor-backed instead of direct SFTP or controller-side
file edits.

## Builtins

Builtins should stay primitive. Good builtins wrap low-level work that is hard
to implement portably or safely in Lua: target-host filesystem operations,
command execution, controller file reads, transfers, JSON, Base64, SHA-256,
template rendering, and bounded tree walks.

Domain policy belongs in custom Lua modules. Service managers, package managers,
databases, containers, and similar high-level resources should live outside the
core unless a clear primitive is missing.

A builtin should be narrow, idempotent where reconciliation is expected,
conservative around destructive actions, strict about invalid input, and tested
through the CLI before other modules build on top of it.

`wali.*` is reserved. User modules must not use that namespace.

## Filesystem rules

Symlink behavior must be explicit:

- `stat` follows symlinks;
- `lstat` does not;
- `walk` returns lstat-style metadata.

Modules choose their own policy. Remove and link reconciliation normally operate
on the path itself, so they use no-follow behavior. Permission changes may
follow symlinks by default because that is often what users expect, but any
no-follow expansion must be portable first.

Tree modules need more care than single-path modules. They must define traversal
order, nesting rules, symlink policy, overwrite policy, special-entry policy,
partial failure behavior, and result records before implementation.

Current tree modules do not prune. Cleanup is also conservative: it removes
resources recorded as created by a successful apply state file. It does not
revert updates, remove unchanged paths, or perform sync-style pruning.

## Results and reporting

A task result is structured: changed/unchanged status, optional message,
optional data, and change records. Change records should say what happened:
created, updated, removed, or unchanged; subject; and path or detail when that
helps.

Apply state is derived from result records, not renderer output. Changed
filesystem resources must report non-empty absolute host paths so cleanup can
reason about them safely.

Workers send events. Reporters render state derived from those events. Execution
logic should not know how human, text, or JSON output is printed.

## Safety rules

Prefer refusing unclear operations over guessing.

Current examples:

- refuse `/`, `.`, `..`, and parent-escaping directory-removal targets;
- refuse existing directory destinations in exact-path rename primitives;
- refuse `/` as a tree destination;
- refuse source/destination nesting in tree operations;
- refuse source symlinks in `copy_file` rather than following them silently;
- refuse special filesystem entries unless a module option explicitly allows
  them;
- preflight predictable tree conflicts before mutating;
- keep `check` non-mutating by construction.

These checks are not ceremony. They keep wali from becoming a thin wrapper around
unsafe shell snippets.

## Tests

The most useful tests run the real CLI against isolated temporary directories.
They should cover:

- first apply changes state;
- second apply is unchanged;
- `plan` and `check` do not mutate;
- `plan` does not touch hosts or Git remotes;
- unsafe paths are rejected;
- tree conflicts are detected before mutation;
- skip/failure reporting is clear;
- dependency ordering is deterministic;
- failed or skipped tasks block only their declared dependents.

Doc-tests are useful for small Rust contracts such as schema normalization,
`run_as` defaults, and result serialization. Avoid `rust,ignore` examples unless
the code truly cannot compile as a doc-test.

## Dependencies

Keep dependencies modest. Add one only when it has a clear job that the standard
library or current crates cannot handle cleanly.

Shell commands are acceptable when they are the portable route across local and
SSH backends, but keep them narrow, quoted, and shared. Internal executor probes
should not use login shells.

## Release checklist

Before tagging:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo package --allow-dirty
```

Use `cargo package --allow-dirty` only for a local release-candidate check. The
final package should come from a clean tree.

For release-facing changes, keep `CHANGELOG.md`, README, docs, and contract
checks in sync.

## Near-term direction

Current priorities:

1. keep worker, reporter, and module boundaries small;
2. keep integration tests ahead of new builtins;
3. keep builtin docs complete;
4. add modules only when their safety rules are clear;
5. keep cleanup conservative until ownership semantics justify stronger pruning
   or revert behavior.
