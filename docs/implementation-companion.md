# WALI Implementation Companion

**Purpose:** translate the development story into concrete code structure,
implementation slices, and stable ownership boundaries.

This document is deliberately closer to code than the
[development story](./development-story.md), but it is still **not a formal
spec**. Its job is to help implementation stay disciplined across multiple work
sessions.

---

## 1. Role of this document

Use this document when answering questions like:

- Which module should own this logic?
- Which entity should hold this state?
- Is this compile-time or runtime behavior?
- Should this be stored globally or per host?
- What is the next safe implementation step?
- Where should this validation happen?

This document assumes the decisions from **WALI Development Story** remain in
force.

---

## 2. The boundary that should shape the codebase

The codebase should be organized around a hard boundary:

```text
Manifest world                Runtime world
---------------------------   -----------------------------
normalized user intent   ->   concrete execution per host
compile-time checks      ->   side effects
expansion/planning       ->   backend operations
ordering                 ->   task execution
secret requirements      ->   secret usage
```

The most important practical rule:

```text
If code reasons about declarations, selectors, or unresolved task references,
it belongs to compile/planning.

If code opens a connection, runs a command, touches files, or streams output,
it belongs to runtime/execution.
```

---

## 3. Suggested top-level crate/module layout

This is a recommended layout, not a required exact tree. The important part is
the separation of responsibility.

```text
src/
├── main.rs
├── cli/
│   ├── mod.rs
│   └── args.rs
├── error/
│   ├── mod.rs
│   └── kinds.rs
├── manifest/
│   ├── mod.rs
│   ├── model.rs          # normalized manifest entities
│   ├── loader.rs
│   ├── normalize.rs
│   └── validate.rs
├── module_catalog/
│   ├── mod.rs
│   ├── model.rs
│   └── resolve.rs
├── compile/
│   ├── mod.rs
│   ├── compiler.rs
│   ├── host_select.rs
│   ├── expand.rs
│   ├── deps.rs
│   ├── order.rs
│   ├── secrets.rs
│   └── model.rs          # CompiledManifest, HostPlan, TaskInstance
├── runtime/
│   ├── mod.rs
│   ├── engine.rs
│   ├── worker.rs
│   ├── host_runtime.rs
│   ├── task_executor.rs
│   ├── event.rs
│   └── result.rs
├── backend/
│   ├── mod.rs
│   ├── local.rs
│   ├── ssh.rs
│   └── traits.rs
├── secrets/
│   ├── mod.rs
│   ├── store.rs
│   └── prompt.rs
├── lua/
│   ├── mod.rs
│   ├── manifest_runtime.rs
│   ├── module_runtime.rs
│   ├── api.rs
│   └── values.rs
└── util/
    ├── mod.rs
    ├── topo.rs
    └── ids.rs
```

### Why this layout fits the project goals

- keeps compile logic out of runtime logic
- makes ownership visible in the filesystem
- keeps SSH isolated from task graph logic
- avoids turning one huge `executor` module into a dumping ground
- allows small focused files and minimal cross-module knowledge

---

## 4. Suggested core entities

---

### 4.1 Manifest-side entities

These represent normalized user intent.

```text
Manifest
├── hosts: Vec<HostSpec>
├── tasks: Vec<TaskSpec>
├── run_as: Vec<RunAsSpec>
└── module_sources: Vec<ModuleSource>
```

```text
TaskSpec
├── id: TaskId
├── module: ModuleName
├── host selector / host ref / nil
├── args
├── depends_on: Vec<TaskId>
├── when / condition
└── run_as: Option<RunAsRef>
```

These entities should stay close to what the manifest author wrote.

They should **not** contain:

- live connections
- execution results
- concrete per-host expansion
- mutable worker state

---

### 4.2 Compile-side entities

These represent the execution plan.

```text
CompiledManifest
├── manifest: Manifest
├── modules: ModuleCatalog
├── host_plans: Vec<HostPlan>
└── secret_requirements: SecretRequirements
```

```text
HostPlan
├── host: HostSpec
├── tasks: Vec<TaskInstance>
└── ordered_task_ids: Vec<TaskInstanceId>
```

```text
TaskInstance
├── instance_id: TaskInstanceId
├── host_id: HostId
├── task_id: TaskId
├── manifest_order: usize
├── module_name: ModuleName
├── normalized_args
├── depends_on: Vec<TaskInstanceId>
├── run_as: Option<RunAsRef>
└── condition / when
```

These entities should be immutable after compilation.

That is important: workers should consume a plan, not rewrite it.

---

### 4.3 Runtime-side entities

These represent live execution state.

```text
Engine
├── reporter
├── secret_store
└── run(compiled_manifest)
```

```text
HostWorker
├── plan: HostPlan
├── secrets: SecretStoreView
├── tx: Sender<Event>
└── execute()
```

```text
HostRuntime
├── host: HostSpec
├── backend: HostBackend
├── facts_cache
└── maybe host-local temp state
```

```text
TaskExecutor<'a>
├── runtime: &'a mut HostRuntime
├── run_as: Option<&'a RunAsRef>
└── execute(task_instance)
```

These entities should own mutation and side effects.

---

## 5. Ownership rules that should not drift

Keep these rules explicit in code review.

### Rule 1: normalized manifest is not execution state

Do not attach runtime caches, task status, or connections to manifest entities.

### Rule 2: compiled manifest is immutable

Workers may read it, but should not mutate it.

### Rule 3: one host worker owns one host runtime

No shared mutable host runtime between threads.

### Rule 4: host runtime owns the connection/backend

Not the engine, not the module runtime, not global state.

### Rule 5: task executor is ephemeral

It exists to execute one task against one host runtime under one `run_as`
context.

### Rule 6: prompting is coordinator-only

Interactive prompting belongs before worker execution.

### Rule 7: host plans are pre-ordered

Workers should not have to solve dependency graphs at runtime.

### Rule 8: Lua module runtime is task-scoped

Prefer one fresh runtime per task execution for predictability.

---

## 6. Suggested Rust types

These sketches are not exact API requirements. They are shape guidance.

```rust
pub struct CompiledManifest {
    pub manifest: Manifest,
    pub modules: ModuleCatalog,
    pub host_plans: Vec<HostPlan>,
    pub secret_requirements: SecretRequirements,
}

pub struct HostPlan {
    pub host: HostSpec,
    pub tasks: Vec<TaskInstance>,
    pub ordered_task_ids: Vec<TaskInstanceId>,
}

pub struct TaskInstance {
    pub instance_id: TaskInstanceId,
    pub host_id: HostId,
    pub task_id: TaskId,
    pub manifest_order: usize,
    pub module_name: ModuleName,
    pub normalized_args: TaskArgs,
    pub depends_on: Vec<TaskInstanceId>,
    pub run_as: Option<RunAsRef>,
    pub when: Option<WhenExpr>,
}
```

```rust
pub struct Engine {
    reporter: Reporter,
}

pub struct HostWorker {
    plan: HostPlan,
    secrets: SecretStore,
    tx: std::sync::mpsc::Sender<Event>,
}

pub struct HostRuntime {
    host: HostSpec,
    backend: HostBackend,
    facts_cache: FactsCache,
}

pub enum HostBackend {
    Local(LocalBackend),
    Ssh(SshBackend),
}

pub struct TaskExecutor<'a> {
    runtime: &'a mut HostRuntime,
    run_as: Option<&'a RunAsRef>,
}
```

### Notes on these shapes

- `HostPlan` stores tasks plus an explicit ordered list. That makes scheduling
  intent visible.
- `TaskInstance` includes both original `task_id` and concrete `instance_id`.
  This is important for diagnostics.
- `TaskExecutor<'a>` borrows runtime instead of owning it. That prevents
  accidental state duplication.

---

## 7. Compile pipeline in code terms

The compile phase should become a narrow, explicit pipeline.

```text
Normalized Manifest
    |
    +--> resolve module catalog
    |
    +--> expand tasks to concrete hosts
    |
    +--> create task instances
    |
    +--> resolve host-local dependencies
    |
    +--> validate dependency graph
    |
    +--> stable topological ordering per host
    |
    +--> compute secret requirements
    v
CompiledManifest
```

### Recommended compile function shape

```rust
pub fn compile_manifest(
    manifest: Manifest,
    module_catalog: ModuleCatalog,
) -> Result<CompiledManifest, Error>
```

Internally, this can break into helpers like:

```rust
resolve_modules(...)
select_hosts_for_task(...)
expand_task_instances(...)
resolve_dependencies(...)
order_host_plan(...)
collect_secret_requirements(...)
```

Each helper should be narrow and testable.

---

## 8. Host expansion semantics

This is one of the most important implementation points.

### Expansion rule

The scheduler does not execute `TaskSpec` directly.

It executes:

```text
TaskSpec + concrete HostSpec -> TaskInstance
```

### Example

```text
TaskSpec(id = "install", host = nil)
Hosts = [local, web-1, web-2]
```

expands to:

```text
install@local
install@web-1
install@web-2
```

### Why this matters

Dependency resolution, ordering, and reporting become concrete and unambiguous
only after expansion.

### Recommended `instance_id` style

Keep it simple and diagnostic-friendly:

```text
{task_id}@{host_id}
```

or an equivalent typed ID internally.

---

## 9. Dependency resolution rules

Use these rules unless there is an intentional architecture change.

### v1 dependency rules

1. `depends_on` references manifest task IDs.
2. Dependency resolution happens **after** host expansion.
3. Dependencies are **host-local**.
4. A task instance may only depend on another task instance on the same host.
5. If a dependency task does not exist for that host, compilation fails.
6. Cycles fail compilation.
7. Original manifest order is used as tie-breaker for stable ordering.

### Example

```text
Task A -> host = ssh-test
Task B -> host = nil, depends_on = [A]
```

Expanded:

```text
A@ssh-test
B@local
B@ssh-test
```

Resolution:

- `B@ssh-test` can depend on `A@ssh-test`
- `B@local` has no `A@local`
- compile error

That is the correct v1 behavior because it is strict and predictable.

---

## 10. Ordering algorithm

Workers should never be handed an unordered graph and told to figure it out
later.

### Required behavior

For each host plan:

1. collect all task instances for the host
2. build edges from host-local dependencies
3. stable-toposort using `manifest_order` as tie-breaker
4. store final order explicitly in the plan

### Why stable ordering matters

Without a stable tie-breaker, equivalent dependency graphs can reorder tasks
between runs. That makes debugging harder and violates predictability.

### Recommended utility

A small local helper is enough:

```text
util::topo::stable_toposort(nodes, edges, tie_breaker)
```

No heavy dependency is necessary.

---

## 11. Runtime model in code terms

The runtime should look like this:

```text
Engine
  -> for each HostPlan
       spawn HostWorker
          -> create HostRuntime
          -> connect backend
          -> iterate ordered tasks
               -> create TaskExecutor
               -> validate/apply module
               -> emit events
```

### Pseudo-graphic relationship map

```text
+-------------------+
| Engine            |
|-------------------|
| reporter          |
| secret store      |
+---------+---------+
          |
          | spawns
          v
+-------------------+       owns        +-------------------+
| HostWorker        |------------------>| HostRuntime       |
|-------------------|                   |-------------------|
| HostPlan          |                   | host              |
| secrets view      |                   | backend           |
| event sender      |                   | facts cache       |
+---------+---------+                   +---------+---------+
          |                                       |
          | creates per task                      |
          v                                       v
+-------------------+                   +-------------------+
| TaskExecutor      |------------------>| HostBackend       |
|-------------------| uses              |-------------------|
| run_as context    |                   | Local or SSH      |
| module ctx        |                   +-------------------+
+-------------------+
```

### Important runtime boundary

`HostWorker` owns the execution loop. `TaskExecutor` should not decide
scheduling policy.

---

## 12. Backend design guidance

Keep backends narrow and capability-oriented.

### Desired shape

You already have capability concepts like:

- facts
- filesystem operations
- command execution
- path semantics

Keep leaning into that.

### Recommended backend layering

```text
HostBackend (enum)
├── LocalBackend
└── SshBackend
```

Each backend should implement the capabilities needed by the task executor.

### Do not do this

- do not let Lua modules talk directly to `ssh2::Session`
- do not let task graph code care whether execution is local or SSH
- do not hide scheduling logic inside the backend

### SSH ownership rule

The SSH session lives inside `SshBackend`, which lives inside `HostRuntime`,
which is owned by one `HostWorker`.

That should remain true even if future optimizations are added.

---

## 13. `run_as` placement

`run_as` should stay task-scoped.

### Correct layering

```text
HostWorker owns base runtime
TaskExecutor applies task-specific run_as rules
Backend performs command/file actions under that context
```

### Why not host-scoped

A host may run:

- some tasks as default user
- some tasks with sudo
- some tasks under another account

Making `run_as` host-scoped would either:

- force multiple runtimes per host
- or add awkward mutable switching logic in the worker

Both are worse than task-scoped execution context.

---

## 14. Secrets and prompting

Keep a two-stage model.

### Stage 1: discover requirements

During compile, determine what kinds of secrets are needed.

Example categories:

- SSH password for host X
- SSH key passphrase for host Y
- elevation password for run_as Z on host X

### Stage 2: collect before execution

The engine or coordinator prompts once, stores results in a secret store, then
workers consume them.

### Suggested shapes

```rust
pub enum SecretRequirement {
    SshPassword { host_id: HostId },
    SshKeyPassphrase { host_id: HostId },
    RunAsPassword { host_id: HostId, run_as: String },
}

pub struct SecretRequirements {
    pub items: Vec<SecretRequirement>,
}
```

```rust
pub struct SecretStore {
    // internal secure-ish storage policy chosen by project needs
}
```

### Important negative rule

Workers should not prompt interactively.

They may fail because a required secret is unavailable, but they should not
become interactive UI actors.

---

## 15. Lua runtime guidance

You already separate manifest and module runtimes. Keep that split.

### Recommended execution model per task

```text
for each task:
    create fresh module Lua runtime
    expose module API
    load module code
    call validate if present
    call apply
    drop runtime
```

### Why this is the safer default

- no hidden state leakage between tasks
- easier reproducibility
- simpler debugging
- less surprising module behavior

### Tradeoff

This may be slightly less efficient than reusing runtimes, but remote automation
is dominated by IO and process execution anyway.

For this project, determinism beats clever reuse.

---

## 16. Event model guidance

Keep events small and streaming-friendly.

### Suggested event categories

```text
EngineStarted
HostStarted
TaskStarted
TaskStdout
TaskStderr
TaskSkipped
TaskSucceeded
TaskFailed
HostFinished
EngineFinished
```

### Event payload principles

- include host identity
- include task instance identity when relevant
- keep payloads serializable and log-friendly
- avoid putting giant runtime objects in events

### Example shape

```rust
pub enum Event {
    HostStarted { host_id: HostId },
    TaskStarted { host_id: HostId, task: TaskInstanceId },
    TaskStdout { host_id: HostId, task: TaskInstanceId, chunk: String },
    TaskStderr { host_id: HostId, task: TaskInstanceId, chunk: String },
    TaskSucceeded { host_id: HostId, task: TaskInstanceId },
    TaskFailed { host_id: HostId, task: TaskInstanceId, error: String },
    HostFinished { host_id: HostId, success: bool },
}
```

---

## 17. Error placement guidance

Use your project error enum, but keep error origin visible.

### Recommended error buckets

```text
ManifestError   -> load/normalize/validate failures
CompileError    -> expansion/dependency/order failures
SecretError     -> prompt/store/retrieve failures
BackendError    -> local/ssh execution failures
ModuleError     -> lua/module contract failures
RuntimeError    -> worker/engine orchestration failures
```

### Why bucket by phase

When something fails, you want to know whether the issue is:

- bad user input
- impossible plan
- missing secret
- transport failure
- module logic failure
- orchestration bug

That separation improves diagnostics without requiring a complex hierarchy.

---

## 18. The first implementation slices

Build vertical slices, not all abstractions at once.

### Slice 1: compile-only dry-run plan

Goal:

```text
load -> normalize -> compile -> print per-host ordered plan
```

No real execution yet.

This slice should prove:

- task expansion works
- host-local dependencies work
- stable ordering works
- compile errors are understandable

### Slice 2: local-only execution

Goal:

```text
compile -> execute ordered tasks on local backend
```

No SSH yet.

This slice should prove:

- host worker model works
- task executor boundary works
- event reporting works
- module runtime lifecycle works

### Slice 3: SSH backend execution

Goal:

```text
one HostWorker -> one SshBackend -> ordered remote tasks
```

This slice should prove:

- secret preflight works
- backend ownership is correct
- remote command/file operations fit existing traits

### Slice 4: `run_as`

Goal:

```text
task-scoped elevation on top of local/ssh backend
```

This slice should prove:

- `run_as` stays out of scheduler logic
- task-scoped context works
- prompting remains coordinator-only

### Slice 5: richer reporting / UX

Goal:

improve output, summaries, skipped-task diagnostics, and failure rendering.

---

## 19. Development checkpoints

At the end of each checkpoint, WALI should remain internally coherent.

### Checkpoint A

- `CompiledManifest` exists
- `HostPlan` exists
- `TaskInstance` exists
- dry-run plan output exists

### Checkpoint B

- workers execute ordered local tasks
- events are emitted
- task instance IDs appear in logs/errors

### Checkpoint C

- SSH backend is owned by host runtime
- secrets are collected before worker start
- remote execution works without changing compile logic

### Checkpoint D

- `run_as` works per task
- no scheduling code knows transport/elevation details

---

## 20. Testing priorities

Keep tests aligned with risk.

### Highest-value tests first

#### Compile tests

- `host = nil` expands correctly
- host selector expansion is correct
- missing same-host dependency fails compile
- cycle detection works
- stable ordering preserves manifest order where possible

#### Runtime tests

- host workers execute tasks in compiled order
- one host failure is reported correctly
- multiple hosts run independently

#### Backend tests

- local backend command execution works
- SSH backend command execution works
- path semantics stay consistent

#### `run_as` tests

- context is applied per task
- mixed default and elevated tasks on same host behave correctly

### Good testing principle

Most scheduling and dependency tests should not require real SSH or real Lua.

Keep compile tests fast and isolated.

---

## 21. What not to add prematurely

To preserve project goals, avoid introducing these before they are clearly
needed:

- async runtime
- global connection pools
- task-level concurrency within a host
- cross-host dependency semantics
- persistent shared Lua state
- dynamic scheduler rewrites during execution
- backend-aware dependency logic

These all increase complexity faster than they increase value for the current
project shape.

---

## 22. Pseudo-graphic summary for future self

```text
                    +----------------------+
                    |   NormalizedManifest |
                    |----------------------|
                    | cleaned user intent  |
                    | still declarative    |
                    +----------+-----------+
                               |
                               | compile
                               v
                    +----------------------+
                    |   CompiledManifest   |
                    |----------------------|
                    | concrete host plans  |
                    | ordered task graph   |
                    | secret requirements  |
                    +-----+-----------+----+
                          |           |
                host plan |           | host plan
                          v           v
                 +--------------+  +--------------+
                 | HostWorker A |  | HostWorker B |
                 +------+-------+  +------+-------+
                        |                 |
                        v                 v
                 +--------------+  +--------------+
                 | HostRuntime  |  | HostRuntime  |
                 | Local / SSH  |  | Local / SSH  |
                 +------+-------+  +------+-------+
                        |                 |
                        +--------+--------+
                                 |
                           per-task executor
                                 |
                                 v
                           Lua module apply
```

---

## 23. Immediate next step recommendation

The next concrete coding move should be:

```text
Implement CompiledManifest + HostPlan + TaskInstance,
then add a dry-run command that prints the final per-host execution plan.
```

That one step will validate the architectural boundary and remove ambiguity
before more runtime complexity is introduced.

---

## 24. Final implementation mantra

Keep repeating this during development:

```text
Normalize first.
Compile to concrete host plans.
Prompt once.
Run one worker per host.
Own the backend inside the host runtime.
Execute tasks sequentially per host.
Keep scheduling out of runtime.
Keep transport out of planning.
Prefer explicitness over cleverness.
```
