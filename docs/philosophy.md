# Project philosophy

This document records wali's design direction. It is not a frozen specification;
it exists to prevent drift as the implementation grows. Operational
module-authoring details belong in `docs/module-developers.md`. Builtin-specific
behavior belongs in `docs/builtin-modules.md`.

## Goal

wali is a small agentless automation tool written in Rust with Lua manifests and
Lua modules.

It should make host-local automation easy without installing an agent on target
hosts. It should stay understandable for one developer reading the source tree,
prefer explicit contracts over framework magic, and avoid broad subsystems until
the current execution model actually needs them.

The intended scope is:

- local and SSH hosts;
- host-local task execution;
- mostly imperative task order;
- small desired-state modules for common filesystem and command operations;
- custom Lua modules for user-specific work;
- predictable plan, check, and apply phases.

The project should not grow into a distributed scheduler, full configuration
management platform, or async orchestration framework without a concrete need
that cannot be solved by the simpler model.

## Core execution model

The CLI currently has three execution layers plus explicit cleanup:

1. `plan` compiles and prints the per-host task plan without host access.
2. `check` performs host-aware, non-mutating validation.
3. `apply` runs the same validation and then mutates host state.
4. `cleanup` uses a previous successful apply state file to remove filesystem
   entries that were reported as created within the current selected manifest
   scope.

This split is central. A user should be able to inspect task expansion before
connecting to anything, then validate against real hosts before applying
changes.

Hosts may run in parallel. `check --jobs N`, `apply --jobs N`, and
`cleanup --jobs N` cap the number of hosts executing at once; `--jobs 1` runs
hosts serially in manifest order. Tasks within one host run sequentially. This
keeps the per-host mental model imperative while still allowing useful
concurrency.

CLI host and task selectors are plan-level narrowing primitives. They mutate the
compiled plan before rendering, secret collection, module source preparation, or
worker launch. Host ids and host tags form an inclusive host selector. Task ids
and task tags form an inclusive task selector. Host and task dimensions are
intersected. Task selection is dependency-inclusive: selected task instances
bring their transitive same-host dependencies, but not their downstream
dependents. This keeps `plan`, `check`, `apply`, and cleanup scope aligned
around one concrete working set.

## Boundaries

Manifest-side data answers what the user wrote. Plan-side data answers what will
run on which host. Runtime-side data owns live connections, effective backends,
reporting events, and execution results.

The plan is immutable runtime input. Workers consume a plan; they do not rewrite
it.

A task becomes meaningful only after host expansion. Dependency resolution is
host-local: a task instance may depend only on task instances on the same host.
Cross-host dependency ordering is intentionally outside the current design.

## Module source model

Module source selection should be deterministic and simple.

A manifest may mount local or Git module sources, optionally under a namespace.
The namespace is only a manifest-level selector for task module names. It is not
Git cache identity and not a prefix module authors must know for internal
imports.

Each task has exactly one effective module source and runs in a fresh Lua
runtime. Wali adds only that source root to the task runtime's `package.path`,
then loads the source-local module name. This preserves normal Lua imports such
as `require("internal.utils.tool")` while avoiding cross-source collisions.

Git module preparation should use the system `git` executable. The local Git
cache is an implementation detail, not a host state backend and not part of
convergence. Git cache locks must protect checkouts for the full `check` or
`apply` execution, because module files may be loaded after the initial fetch.
Every Git child process must be bounded by timeout, must not read from
interactive stdin, and must not prompt through Git's terminal credential flow.
Output capture must not depend on pipe-reader threads that can be held open by
Git helpers or grandchild processes after the direct Git child is killed.

`plan` remains compile-only: no Git fetches, no host access, no network access.

## Lua phase model

Lua module execution has three contracts:

1. `requires` checks host capabilities.
2. `validate` performs read/probe-only validation.
3. `apply` performs mutation.

`requires` is module-owned and host-focused. It should check capabilities such
as commands, paths, environment variables, OS, architecture, hostname, user, and
group.

`validate` must not mutate host state through the normal context API. If
validation could write files or run arbitrary commands, `check` would become a
weaker form of apply and would not be safe enough to trust.

`apply` receives the full context and may perform mutations.

## Effective backend

Host access is mediated through an effective backend. Task-level `run_as`
binding must affect filesystem operations and command execution consistently.

That is why host filesystem operations are executor-backed operations rather
than direct SFTP or controller-side filesystem calls. If a task is bound to a
user through `run_as`, operations such as `write`, `create_dir`, `chmod`, and
`copy_file` must be executed through that same effective backend.

## Builtin module philosophy

Builtin modules should be desired-state resources whenever possible.

Good builtin modules describe an intended state:

- this directory exists;
- this file has this content;
- this symlink points here;
- this tree is copied here;
- this path is absent.

They should not merely expose syscall-shaped wrappers such as `mkdir`, `rm`, or
`ln`. Low-level host operations are already available to custom modules through
`ctx.host.*` during apply.

A builtin module should normally be:

- idempotent;
- strict about invalid input;
- explicit about destructive behavior;
- conservative around special filesystem entries;
- structured in its execution result;
- covered by integration tests before more modules are added on top of it.

The reserved namespace is `wali.*`. User modules should not use it. Shared
builtin Lua helpers belong in `wali.builtin.lib`.

## Filesystem semantics

Filesystem metadata must be explicit about symlink behavior.

- `stat` follows symlinks.
- `lstat` does not follow symlinks.
- `walk` returns lstat-style metadata for entries.

Modules should choose symlink behavior deliberately. For example, remove and
link reconciliation normally use `lstat` because they operate on the path
itself. Permissions may follow symlinks by default because users often intend to
update the target, but no-follow behavior must be explicit and portable before
it is expanded.

Tree modules are especially sensitive. A tree module should define traversal
order, nesting rules, symlink policy, overwrite policy, special-entry policy,
pruning behavior, partial-failure behavior, and structured changes before it is
implemented.

Current tree modules avoid pruning. `apply --state-file FILE` records the
selected effective plan, explicit resource records, and the final successful
apply report state. `cleanup --state-file FILE` uses the explicit resource
records conservatively: it removes filesystem entries recorded as created
resources in the current selected manifest scope. It does not revert updates,
remove unchanged paths, or perform sync-style pruning. Cleanup does not rewrite
the apply state file; a new successful apply records the next baseline.

## Result contract

Execution results are structured. A successful task returns change records, an
optional human message, and optional machine-readable data.

A change record should say what happened, not only that something happened:
created, updated, removed, or unchanged; subject; path or detail when useful.
Successful apply state converts those task results into explicit resource
records. Cleanup consumes those resource records, not renderer JSON internals.

## Reporting model

Workers send events. Renderers consume state derived from events.

The reporting path should stay separate from execution decisions. A worker
should not know how human, text, or JSON output is rendered. A renderer should
not own task execution logic.

## Safety principles

Prefer refusing unclear operations over guessing.

Examples:

- refuse `/`, `.`, `..`, and parent-escaping paths as directory removal targets;
- refuse existing directory destinations in exact-path rename primitives;
- refuse `/` as a tree destination path;
- refuse source/destination nesting in tree operations;
- refuse source symlinks in `copy_file` instead of silently following them;
- refuse special filesystem entries unless an explicit module option says
  otherwise;
- preflight predictable tree conflicts before mutating;
- keep `check` non-mutating by construction.

These refusals may feel strict, but they keep wali from becoming a collection of
thin wrappers around dangerous shell commands.

## Testing direction

The most valuable tests are local black-box integration tests that run the real
CLI binary against isolated temporary directories.

Important properties to test:

- first apply changes state;
- second apply is unchanged;
- check does not mutate;
- plan does not access hosts or Git remotes;
- unsafe paths are rejected;
- tree conflict preflight happens before mutation;
- when-skip and requires-failure are reported correctly;
- task ordering and dependency errors are deterministic;
- runtime dependency semantics are deterministic: failed or skipped tasks block
  only declared dependents, while independent tasks continue on the same host.

Doc-tests are useful for critical Rust contracts such as schema normalization,
run_as defaults, and execution result serialization. They should be small and
executable. Avoid `rust,ignore` examples unless the code cannot reasonably be
compiled as a doc-test.

## Dependency discipline

wali should keep dependencies modest. A new dependency should have a clear role
that cannot be handled cleanly by the standard library or existing crates.

The implementation may use shell commands where needed for portability across
local and SSH backends, but those shell snippets should be narrow, quoted
carefully, and shared where possible. Internal executor probes should not use
login shells.

## Near-term direction

The current priority order is:

1. keep the worker/report/module contracts tight;
2. keep integration tests ahead of new builtins;
3. document module authoring and current builtin behavior;
4. add new modules only when their safety semantics are clear;
5. keep cleanup conservative; destructive sync-style pruning or revert features
   need stronger ownership semantics first.
