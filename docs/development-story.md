# WALI Development Story

**Purpose:** keep the architecture, execution story, and design constraints
stable while implementation continues.

This document is **not a formal spec**. It is a development guide for future
work so the project stays small, predictable, and internally consistent.

---

## 1. Project intent

WALI is meant to be:

- tiny
- straightforward
- mostly imperative
- agentless
- reliable
- easy to reason about
- easy to extend with user modules
- easy to write manifests for

This means the architecture should prefer:

- explicit data flow over magic
- compile/plan first, execute second
- deterministic ordering
- host-level parallelism only
- local ownership of mutable runtime state
- narrow interfaces
- minimal hidden behavior

It should avoid:

- global mutable runtime state
- async complexity unless absolutely needed
- cross-thread ownership tricks
- implicit task scheduling rules
- ambiguous dependency semantics
- long-lived shared interpreter state

---

## 2. The key boundary

The central architectural boundary is:

```text
user manifest intent
        |
        v
normalized manifest
        |
        v
compiled per-host execution plan
        |
        v
runtime execution
```

The missing piece that unlocks the rest of the system is the layer between
**normalized manifest** and **runtime execution**.

That layer is represented by **CompiledManifest**.

---

## 3. Normalized Manifest vs CompiledManifest

This distinction must stay sharp.

### Normalized Manifest

The **normalized manifest** is still the user's declarative input, just cleaned
up into a stable internal shape.

It answers questions like:

- Which hosts are declared?
- Which tasks exist?
- Which modules are referenced?
- What are the normalized task fields?
- What does `host = nil` mean in internal form?
- What does `run_as` look like after normalization?

The normalized manifest is still close to author intent. It is **not
execution-ready**.

It may still contain ambiguity that has not yet been resolved into concrete
runtime work units.

### CompiledManifest

The **CompiledManifest** is an internal planning artifact produced from the
normalized manifest plus resolved module information.

It answers questions like:

- Which concrete hosts will actually run which tasks?
- What is the exact per-host ordered task list?
- What does task `X` expand to on host `A` vs host `B`?
- Which dependency edges exist after host expansion?
- Which task instances are invalid because dependencies do not resolve on the
  same host?
- Which secrets are required before execution?
- What is the exact execution plan that workers will consume?

### Short version

```text
Normalized Manifest = cleaned user intent
CompiledManifest   = execution-ready internal plan
```

### Practical rule

If a structure still talks in terms of broad manifest declarations, it is part
of the normalized manifest world.

If a structure talks in terms of concrete **task instances on a concrete host**,
it belongs to the compiled manifest world.

---

## 4. Core design decisions

### 4.1 Parallelism model

- Hosts run in parallel.
- Tasks within one host run sequentially.

This preserves a simple imperative mental model while still allowing overall
concurrency.

### 4.2 Runtime ownership model

- The coordinator owns immutable global data and orchestration.
- Each host worker owns that host's mutable runtime state.
- SSH connections are owned by the host runtime inside the host worker.
- `run_as` is task-scoped, not host-scoped.

### 4.3 Execution model

Execution should follow this shape:

```text
compile plan -> collect secrets -> spawn workers -> connect per host -> run tasks in order
```

### 4.4 Dependency model

Dependencies are resolved **after host expansion**.

That means the actual runtime unit is not just `Task`, but:

```text
TaskInstance = one concrete task on one concrete host
```

### 4.5 Scope of dependencies in v1

Dependencies are **host-local**.

A task instance may depend only on task instances that exist for the same host.

Cross-host dependencies are intentionally out of scope for v1.

---

## 5. Architectural story

### 5.1 Phase 1: Load and normalize

The system reads the manifest, validates structure, normalizes defaults, and
builds the stable internal manifest model.

At the end of this phase, the system knows:

- the declared hosts
- the declared tasks
- the normalized task fields
- the normalized run-as declarations
- the referenced modules

But it still does **not** yet know the final execution plan.

### 5.2 Phase 2: Compile

The system converts the normalized manifest into a compiled execution plan.

This phase:

- resolves modules
- expands tasks onto concrete hosts
- creates task instances
- resolves host-local dependency edges
- checks dependency validity
- stable-toposorts each host plan
- determines which secrets are needed

This is the phase where ambiguity is eliminated.

### 5.3 Phase 3: Preflight input collection

Before any worker threads begin actual execution, the coordinator collects
secrets required for execution.

Examples:

- SSH password
- key passphrase
- `run_as` password when needed

Interactive prompting belongs here, not inside worker threads.

### 5.4 Phase 4: Per-host execution

Each host worker:

- receives one host plan
- creates one host runtime
- establishes one backend for that host
- executes tasks in the compiled order
- sends events back to the coordinator

### 5.5 Phase 5: Reporting and exit

The coordinator aggregates worker events and task results into final output and
process exit status.

---

## 6. Entity model

### 6.1 Top-level entity roles

```text
+--------------------+
| Engine / Coordinator|
+--------------------+
| owns orchestration |
| owns manifest      |
| owns compiled plan |
| owns secrets       |
| owns event rx      |
+---------+----------+
          |
          | spawns
          v
+--------------------+      +--------------------+      +--------------------+
| HostWorker(host A) |      | HostWorker(host B) | ...  | HostWorker(host N) |
+--------------------+      +--------------------+      +--------------------+
| owns host runtime  |      | owns host runtime  |      | owns host runtime  |
| owns backend       |      | owns backend       |      | owns backend       |
| runs tasks in seq  |      | runs tasks in seq  |      | runs tasks in seq  |
+---------+----------+      +---------+----------+      +---------+----------+
          |                           |                           |
          v                           v                           v
   +-------------+              +-------------+             +-------------+
   | HostRuntime |              | HostRuntime |             | HostRuntime |
   +-------------+              +-------------+             +-------------+
   | Local/Ssh   |              | Local/Ssh   |             | Local/Ssh   |
   | facts cache |              | facts cache |             | facts cache |
   +------+------+              +------+------+             +------+------+
          |                            |                           |
          v                            v                           v
   +-------------+              +-------------+             +-------------+
   |TaskExecutor |              |TaskExecutor |             |TaskExecutor |
   +-------------+              +-------------+             +-------------+
   | per task    |              | per task    |             | per task    |
   | task-scoped |              | task-scoped |             | task-scoped |
   | run_as      |              | run_as      |             | run_as      |
   +-------------+              +-------------+             +-------------+
```

### 6.2 Proposed entities

#### Engine / Coordinator

Responsibilities:

- load manifest
- normalize manifest
- resolve modules
- compile host plans
- collect secrets
- spawn workers
- receive events
- aggregate final result

#### CompiledManifest

Responsibilities:

- hold the execution-ready plan
- hold resolved module references/catalog
- hold per-host plans
- serve as the single source of truth for execution order

#### HostPlan

Responsibilities:

- represent the exact task plan for one concrete host
- hold tasks already ordered for execution
- remain immutable once compiled

#### TaskInstance

Responsibilities:

- represent one concrete task bound to one concrete host
- carry module reference, normalized args, dependency info, manifest order, and
  optional `run_as`

#### HostWorker

Responsibilities:

- own execution for one host
- create the host runtime
- establish transport/backend
- execute tasks in order
- emit events

#### HostRuntime

Responsibilities:

- own all mutable host-specific runtime state
- own the backend/connection
- expose primitives needed by modules
- hold reusable per-host caches such as facts

#### TaskExecutor

Responsibilities:

- provide per-task execution context
- apply task-scoped `run_as`
- expose the module contract against the host runtime

---

## 7. Ownership rules

These rules should remain stable.

### 7.1 Coordinator owns

- normalized manifest
- compiled manifest
- module catalog
- secret store
- worker lifecycle
- event receiver
- final result aggregation

### 7.2 Host worker owns

- one concrete host
- one host runtime
- one live backend for that host
- one immutable host plan
- one event sender back to the coordinator

### 7.3 Task executor owns

This should usually be short-lived and task-scoped.

It should own or borrow:

- mutable access to host runtime
- selected `run_as`
- task metadata
- task context visible to Lua

### 7.4 SSH session ownership

A live SSH session belongs to the host runtime inside the host worker.

It should **not** be shared globally. It should **not** be owned by the
coordinator. It should **not** be passed around as a cross-host concurrency
resource.

---

## 8. Scheduling and ordering

### 8.1 Execution unit

The true execution unit is:

```text
TaskInstance(host_id, task_id)
```

Not just `Task`.

### 8.2 Expansion rule

A task is first expanded to the hosts it applies to.

Examples:

- explicit host selector -> concrete matching hosts
- `host = nil` -> all hosts

### 8.3 Dependency rule

Dependencies are resolved after expansion.

That means:

```text
Task B depends on Task A
```

becomes, for each host:

```text
TaskInstance(B@host) depends on TaskInstance(A@host)
```

only if both instances exist for that same host.

### 8.4 Invalid case rule

If a dependency cannot be resolved on the same host, it is a compile-time error
in v1.

Example:

```text
A runs only on host ssh-test
B runs on all hosts
B depends_on A
```

Then:

- `B@ssh-test` may resolve `A@ssh-test`
- `B@local` has no `A@local`

That should be rejected during compile.

### 8.5 Ordering algorithm

For each host:

1. select tasks that apply to the host
2. create task instances in original manifest order
3. add dependency edges
4. perform stable topological sort
5. execute sequentially in that order

Tie-break rule:

- original manifest order wins when dependencies do not decide

This keeps behavior deterministic and easy to explain.

---

## 9. `run_as` model

`run_as` should be **task-scoped**.

It should not mutate host-global scheduler state. It should not define a
separate worker.

Instead:

```text
HostWorker owns base runtime/backend
TaskExecutor applies per-task run_as behavior
```

That means two tasks on the same host may execute with different privileges
without forcing a different host ownership model.

### Why this is the right place for `run_as`

Because `run_as` is not part of scheduling. It is part of how a specific task
executes against the host runtime.

---

## 10. Secret collection and prompting

### Rule

All interactive prompting should happen in the coordinator preflight stage.

### Examples of preflight prompts

- SSH password
- private key passphrase
- privilege escalation password

### What should not happen in preflight

The coordinator should not establish and keep long-lived live SSH sessions for
later worker use.

The correct split is:

```text
coordinator: collect secrets
worker: establish live connection
```

This keeps ownership and failure handling simple.

---

## 11. Backend model

The backend surface should stay narrow and capability-driven.

Current capability ideas already point in the right direction:

- facts
- filesystem operations
- command execution
- path semantics

Recommended shape:

```text
HostRuntime
  -> HostBackend enum
       -> LocalBackend
       -> SshBackend
```

Both local and SSH execution should satisfy the same capability contracts as
much as possible.

### Important principle

Modules should target a stable host execution API, not transport-specific
details.

That preserves portability and keeps modules easy to write.

---

## 12. Lua runtime model

The simplest and safest approach is:

- fresh Lua runtime per task execution
- load module
- validate args
- apply task
- drop runtime

### Why

This avoids:

- hidden global state across tasks
- accidental leakage between modules
- order-sensitive interpreter behavior

The performance cost is acceptable because remote execution and I/O dominate
anyway.

Predictability matters more than micro-optimizing interpreter reuse.

---

## 13. Event model

Workers should send structured events back to the coordinator.

Minimal useful event types:

```text
HostStarted
HostConnectStarted
HostConnectFinished
TaskStarted
TaskSkipped
TaskStdoutChunk
TaskStderrChunk
TaskFinished
HostFinished
```

This event stream should support:

- live user-facing progress output
- final summary
- error propagation
- future logging/reporting improvements

The worker should not own presentation logic. It should emit facts/events. The
coordinator/reporter decides how to print.

---

## 14. Pseudo-graphics: relation model

```text
┌──────────────────────────────┐
│        NormalizedManifest    │
│------------------------------│
│ cleaned declarative input    │
│ hosts, tasks, run_as, vars   │
│ still close to user intent   │
└──────────────┬───────────────┘
               │ compile
               v
┌──────────────────────────────┐
│        CompiledManifest      │
│------------------------------│
│ resolved modules             │
│ host-expanded task instances │
│ per-host ordered plans       │
│ secret requirements          │
└──────────────┬───────────────┘
               │ contains
               v
      ┌────────────────────┐
      │      HostPlan      │
      │--------------------│
      │ host = concrete H  │
      │ ordered tasks      │
      └─────────┬──────────┘
                │ contains
                v
      ┌────────────────────┐
      │    TaskInstance    │
      │--------------------│
      │ one task on one    │
      │ concrete host      │
      │ deps already bound │
      └────────────────────┘
```

---

## 15. Pseudo-graphics: execution flow

```text
 CLI
  |
  v
 Load manifest
  |
  v
 Normalize manifest
  |
  v
 Resolve modules
  |
  v
 Compile host plans
  |
  +--> expand tasks onto hosts
  +--> validate host-local deps
  +--> stable-toposort per host
  +--> determine required secrets
  |
  v
 Collect secrets (interactive)
  |
  v
 Spawn one worker per host
  |
  +--> worker builds HostRuntime
  +--> worker creates LocalBackend or SshBackend
  +--> worker executes tasks sequentially
  +--> worker emits events
  |
  v
 Coordinator aggregates results
  |
  v
 Final exit status
```

---

## 16. Pseudo-graphics: ownership map

```text
Coordinator
├── NormalizedManifest
├── CompiledManifest
├── ModuleCatalog
├── SecretStore
├── Reporter
└── WorkerHandles

HostWorker(host-X)
├── HostPlan(host-X)
├── HostRuntime(host-X)
│   ├── HostBackend
│   │   ├── LocalBackend OR
│   │   └── SshBackend(session, channels, etc.)
│   └── FactsCache
└── EventSender

Per task execution
└── TaskExecutor
    ├── &mut HostRuntime
    ├── TaskInstance
    └── optional run_as
```

---

## 17. Invariants

These invariants should always hold.

### Compile-time invariants

- every module referenced by a task resolves successfully
- every task instance belongs to exactly one host plan
- every dependency edge in a host plan points to another task instance in the
  same host plan
- every host plan is already execution-ordered before runtime begins
- manifest order is preserved as a stable tie-breaker

### Runtime invariants

- each host worker owns exactly one host runtime
- each host runtime owns at most one live backend connection for its host
- tasks on the same host never execute concurrently
- cross-host task execution may happen concurrently
- worker threads never prompt the user interactively

### Safety invariants

- module code does not receive transport internals directly
- `run_as` is resolved per task, not as hidden mutable global state
- connection ownership never crosses host boundaries

---

## 18. Anti-goals for now

The following should remain out of scope until the basic architecture is
complete and proven stable.

- cross-host dependency semantics
- a global task scheduler across all hosts
- task-level parallelism within one host
- async runtime migration
- shared SSH connection pools
- long-lived shared Lua runtime across many tasks
- transport-specific module APIs

These can be reconsidered later, but they should not distort v1.

---

## 19. Suggested Rust shape

This is illustrative, not frozen API.

```rust
pub struct Engine {
    reporter: Reporter,
}

pub struct CompiledManifest {
    pub modules: ModuleCatalog,
    pub host_plans: Vec<HostPlan>,
    pub secret_requirements: SecretRequirements,
}

pub struct HostPlan {
    pub host: HostSpec,
    pub tasks: Vec<TaskInstance>,
}

pub struct TaskInstance {
    pub host_id: String,
    pub task_id: String,
    pub manifest_order: usize,
    pub depends_on: Vec<TaskInstanceId>,
    pub module_name: String,
    pub normalized_args: serde_json::Value,
    pub run_as_id: Option<String>,
}

pub struct HostWorker {
    pub plan: HostPlan,
    pub secrets: SecretStore,
    pub tx: std::sync::mpsc::Sender<Event>,
}

pub struct HostRuntime {
    pub host: HostSpec,
    pub backend: HostBackend,
    pub facts_cache: FactsCache,
}

pub enum HostBackend {
    Local(LocalBackend),
    Ssh(SshBackend),
}

pub struct TaskExecutor<'a> {
    pub runtime: &'a mut HostRuntime,
    pub run_as: Option<&'a RunAsRef>,
}
```

---

## 20. Implementation sequence

This order should minimize drift and rework.

### Step 1: Freeze semantic rules

Write down and keep stable:

- `host = nil` means all hosts
- dependencies are host-local after expansion
- tie-break ordering is original manifest order
- one worker per host
- one backend per host runtime
- one task executor per task
- prompting only in coordinator preflight

### Step 2: Add compile-layer types

Introduce:

- `CompiledManifest`
- `HostPlan`
- `TaskInstance`

This is the most important missing architectural piece.

### Step 3: Build host-plan compiler

Implement:

- task-to-host expansion
- module resolution checks
- host-local dependency resolution
- stable topological sort
- compile-time rejection of unresolved per-host dependencies

### Step 4: Add plan introspection command

Add a command that prints the compiled plan without executing.

For example:

```text
wali plan manifest.lua
```

or equivalent.

This is extremely useful for debugging, user trust, and architectural
discipline.

### Step 5: Make local execution work end to end

Before finishing SSH complexity, ensure:

- compile works
- worker model works
- events work
- local backend works

### Step 6: Add SSH backend

Then implement:

- connection establishment
- auth
- command execution
- file operations
- facts

### Step 7: Layer in task-scoped `run_as`

Implement elevation as a per-task execution concern, not a scheduling concern.

### Step 8: Keep reporting structured

Ensure workers emit structured events; let coordinator/reporting own output
formatting.

---

## 21. Practical test scenarios to protect the architecture

### Scenario A: simple single-host linear plan

- one host
- three tasks
- no dependencies except manifest order

Expected:

- tasks execute in original order

### Scenario B: multi-host expansion

- two hosts
- one task with `host = nil`

Expected:

- one task instance per host
- parallel host execution

### Scenario C: host-local dependency success

- task A on host X
- task B on host X depends on A

Expected:

- valid compile
- A before B on host X

### Scenario D: host-local dependency failure

- task A only on host X
- task B on all hosts depends on A

Expected:

- compile-time error because dependency cannot resolve for all expanded
  instances of B

### Scenario E: mixed `run_as`

- same host
- task A runs default user
- task B runs elevated user

Expected:

- same host runtime
- different per-task execution behavior
- no extra worker required

### Scenario F: SSH auth prompt

- password-based SSH host

Expected:

- prompt during preflight
- not inside worker thread

---

## 22. Working mental model

When implementation gets confusing, use this mental model:

```text
Manifest says what user wants.
Compiler turns that into exact per-host work.
Workers execute that work.
Modules act through a narrow host runtime API.
```

Or even shorter:

```text
normalize -> compile -> preflight -> execute -> report
```

If a new feature does not fit cleanly into that flow, it should be treated with
suspicion.

---

## 23. Final guidance for future changes

When adding features later, ask these questions:

1. Is this a manifest concern, a compile concern, or a runtime concern?
2. Does this preserve deterministic per-host ordering?
3. Does this keep connection ownership local to a host worker?
4. Does this avoid hidden global mutable state?
5. Does this make manifest behavior easier to explain, or harder?
6. Does this preserve tiny-tool simplicity?

If the answer to several of these is "no", the design likely needs adjustment.

---

## 24. One-sentence architecture summary

**WALI should compile a normalized declarative manifest into immutable per-host
execution plans, then run those plans in parallel across hosts and sequentially
within each host using host-owned runtimes and task-scoped execution context.**
