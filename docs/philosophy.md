# Project philosophy

This document records the current design direction for wali. It is not a frozen
specification. It exists to keep future changes aligned with the same execution
model and safety rules.

## Goal

wali is a small agentless automation tool written in Rust with Lua manifests and
Lua modules.

The project should make it easy to describe and execute host-local automation
without installing an agent on the target host. It should stay understandable
for one developer reading the source tree. It should prefer explicit contracts
over implicit framework behavior.

The intended scope is:

- local and SSH hosts;
- host-local task execution;
- mostly imperative task order;
- small desired-state modules for common filesystem and command operations;
- custom Lua modules for user-specific work;
- predictable plan, check, and apply phases.

The project should not grow into a general distributed scheduler, a full
configuration management platform, or an async orchestration framework unless
there is a concrete need that cannot be solved by the current simpler model.

## Core execution model

The CLI is intentionally split into three layers:

1. plan -> compile and print the per-host task plan, no host access
2. check -> connect to hosts and run non-mutating validation
3. apply -> run the same checks and then mutate host state

This split is important. A user should be able to inspect task expansion before
connecting to anything, then run host-aware validation before applying changes.

The execution order is:

- load manifest
- compile plan
- collect required secrets for check/apply
- spawn one worker per host
- connect per host
- run host tasks sequentially
- report structured events

Hosts may run in parallel. Tasks within one host run sequentially. This keeps
the per-host mental model imperative while still allowing useful concurrency.

## Plan and runtime boundaries

The plan is immutable runtime input. Workers consume a plan; they do not rewrite
it.

Manifest-side data answers what the user wrote. Plan-side data answers what will
run on which host. Runtime-side data owns live connections, effective backends,
reporting events, and execution results.

A task becomes meaningful only after host expansion. Dependency resolution is
host-local: a task instance may depend only on task instances on the same host.
Cross-host dependency ordering is intentionally outside the current design.

## Lua phases

Lua module execution has three separate contracts:

1. requires -> host capability check
2. validate -> read/probe-only validation
3. apply -> mutation

`requires` is module-owned and host-focused. It should check capabilities such
as commands, paths, environment variables, OS, architecture, hostname, user, and
group. It is evaluated before module validation and before apply.

`validate` receives a restricted context. It may inspect facts, paths,
filesystem metadata, file contents, directory listings, symlink targets, and
walk output. It must not mutate host state through the normal context API.

`apply` receives the full context and may perform mutations.

This boundary is what makes `wali check` useful. If validation could write files
or run arbitrary commands, `check` would become a weaker form of apply and would
not be safe enough to trust.

## Effective backend

Host access is mediated through an effective backend. Task-level `run_as`
binding must affect filesystem operations and command execution consistently.

That is why host filesystem operations are executor-backed operations rather
than direct SFTP or controller-side filesystem calls. If a task is bound to a
user through `run_as`, operations such as `write`, `create_dir`, `chmod`, and
`copy_file` must be executed through that same effective backend.

## Module philosophy

Builtin modules should be desired-state resources whenever possible.

Good builtin modules describe an intended state:

- this directory exists
- this file has this content
- this symlink points here
- this tree is copied here
- this path is absent

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

The reserved namespace is `wali.builtin.*`. User modules should not use it.
Shared builtin Lua helpers belong in `wali.builtin.lib`.

## Filesystem semantics

Filesystem metadata must be explicit about symlink behavior.

- stat -> follows symlinks
- lstat -> does not follow symlinks
- walk -> returns lstat-style metadata for entries

Modules should choose symlink behavior deliberately. For example, remove and
link reconciliation normally use `lstat` because they operate on the path
itself. Permissions may follow symlinks by default because users often intend to
update the target, but no-follow behavior must be explicit and portable before
it is expanded.

Tree operations are especially sensitive. A tree module should define:

- traversal order;
- source/destination nesting rules;
- symlink policy;
- overwrite policy;
- special-entry policy;
- whether extra destination entries are pruned;
- whether failures can leave partial state;
- what structured changes are reported.

Current tree modules avoid pruning. More destructive sync-style behavior should
wait for a journal or state-file design.

## Result contract

Execution results are structured. A successful task returns an `ExecutionResult`
with change records, an optional human message, and optional machine-readable
data.

A change record should say what happened, not only that something happened.

- created / updated / removed / unchanged
- fs entry / command / future subjects
- path and detail when useful

This structure supports human output, JSON output, tests, and future state files
for cleanup or partial revert.

## Reporting model

Workers send events. Renderers consume state derived from events.

The reporting path should stay separate from execution decisions. A worker
should not know how human, text, or JSON output is rendered. A renderer should
not own task execution logic.

The event lifecycle should stay stable and small enough that adding a command
such as `plan`, `check`, or `apply` does not require a second reporting system.

## Safety principles

Prefer refusing unclear operations over guessing.

Examples:

- refuse `/` as a remove or tree destination path;
- refuse source/destination nesting in tree operations;
- refuse source symlinks in `copy_file` instead of silently following them;
- refuse special filesystem entries unless an explicit module option says
  otherwise;
- preflight predictable tree conflicts before mutating;
- keep `check` non-mutating by construction.

These refusals may feel strict, but they protect the project from becoming a
collection of thin wrappers around dangerous shell commands.

## Testing direction

The most valuable tests are local black-box integration tests that run the real
CLI binary against isolated temporary directories.

Important properties to test:

- first apply changes state;
- second apply is unchanged;
- check does not mutate;
- plan does not access hosts;
- unsafe paths are rejected;
- tree conflict preflight happens before mutation;
- when-skip and requires-failure are reported correctly;
- task ordering and dependency errors are deterministic.

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
5. design journal/state tracking before destructive sync or revert features.
