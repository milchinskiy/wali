# Project philosophy

wali is a small automation tool, not a configuration-management platform. It
tries to make host changes predictable without hiding the underlying operating
system behind a large policy framework.

## Goal

The goal is to run explicit, idempotent tasks against local and SSH hosts from a
Lua manifest. Wali should make the safe path easy:

- inspect the effective plan before touching hosts;
- validate module input before mutation;
- run tasks in deterministic per-host order;
- report structured results;
- clean up only resources that wali knows it created.

The project should stay small enough that a user can understand how work moves
from manifest to plan to check to apply.

## Boundaries

Wali intentionally does not try to own every layer of infrastructure management.
It should not grow a large built-in package manager, service manager, user
manager, or distribution policy library. Those belong in external modules built
on top of narrow primitives.

The core should provide:

- manifest loading and validation;
- host connection and effective backend behavior;
- task selection, dependency handling, and execution flow;
- safe filesystem, command, transfer, template, codec, hash, and JSON
  primitives;
- compact builtin modules for common low-level operations;
- structured reporting and state-file capture.

Domain-specific decisions should live in manifests or custom module
repositories.

## Agentless model

Wali does not install a daemon on target hosts. Local and SSH hosts are driven
from the controller process through the same backend abstraction. That keeps the
operational model simple and makes `run_as` behavior explicit: filesystem
changes and command execution must go through the effective backend, not around
it.

## Lua and Rust responsibilities

Lua is used for user-facing manifests and modules because it is compact, easy to
read, and suitable for small policy code. Rust owns the hard edges:

- process control;
- SSH transport;
- filesystem primitives;
- path normalization;
- module-source preparation;
- schema normalization;
- result validation;
- report and state serialization.

When a primitive affects safety or portability, prefer implementing the
primitive in Rust and exposing it to Lua through `ctx`.

## Builtins

Builtin modules should stay primitive and unsurprising. A builtin may create a
file, manage a symlink, copy a tree, run a command, or render a template. It
should not silently encode high-level product policy.

Good builtin behavior is:

- narrow input contract;
- strict validation;
- explicit symlink policy;
- explicit replace policy;
- idempotent reconciliation;
- structured changes.

If a module starts needing distribution-specific decisions, service
orchestration policy, or application-specific defaults, it probably belongs
outside the core.

## Filesystem rules

Target-host filesystem modules require absolute host paths unless a module
explicitly documents otherwise. Controller-side transfer and template paths may
be absolute or relative to manifest `base_path`. Tree transfer modules keep
these namespaces explicit: `push_tree` is controller-to-host, and `pull_tree` is
host-to-controller.

Destructive operations should reject ambiguous or dangerous inputs before
mutation. Tree operations should reject source/destination nesting that would
make traversal unstable. Symlink following must be documented per module.

## Results and cleanup

Modules return structured results. Results must clearly distinguish unchanged,
changed, failed, and skipped work. Resource records are not cosmetic: cleanup
uses them to decide what may be removed later.

Cleanup is deliberately conservative. It removes resources recorded as `created`
by a previous successful apply and inside the currently selected manifest scope.
It does not remove entries that were updated, unchanged, or not recorded.

## Compatibility

Before 1.0, manifest, module, and state-file formats may still change. A change
that affects users should update the relevant docs and `CHANGELOG.md` in the
same patch.

After 1.0, those formats should be treated as compatibility surfaces. New
features should prefer additive changes and explicit migration notes.
