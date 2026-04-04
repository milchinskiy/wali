# Wali: strict development plan for a tiny Rust + Lua automation tool

_Date: 2026-04-04_

## 1. Executive summary

Your current top-level flow is directionally correct:

`raw manifest -> normalized manifest -> evaluation plan -> per host plan -> threaded executor -> compiled report`

That shape matches the strongest ideas from mature systems:

- **Puppet** separates **catalog compilation** from **catalog application**.
- **Salt** separates human-friendly state from machine-friendly lowstate.
- **Ansible** exposes multiple execution strategies and shows, by its own design
  and documentation, that host scheduling and privilege escalation are where a
  lot of operational surprises come from.
- **Chef** demonstrates the value of idempotent resource contracts, guards,
  no-op/why-run, notifications, handlers, and single-run locking.

For **Wali**, the right answer is **not** to copy any one of them. The right
answer is to build a smaller system with a narrower contract:

1. **Pure compile pipeline, impure execution pipeline.**
2. **Host-pinned execution by default**: each host progresses independently
   through its own ordered plan.
3. **Strongly typed internal plan model** that is serializable, hashable, and
   stable.
4. **Event-sourced reporting**: reports are derived from structured events,
   never from logs.
5. **`run_as` is deny-by-default, preflight-verified, non-interactive, and
   restricted.**
6. **Lua manifest evaluation is side-effect free.**
7. **Shell execution is an explicit escape hatch, not the primary abstraction.**

That gives you a small, predictable core with room to grow without turning into
an Ansible/Chef/Puppet replacement.

---

## 2. Current skeleton review

## What is already promising

The skeleton already contains a few healthy signals:

- A distinct `manifest` namespace.
- A distinct `runtime` namespace for embedded Lua.
- A distinct `host` namespace for execution concerns.
- A separate `When` model for predicates.
- Per-host selection via `HostSelector`.
- A controller facts abstraction and executor capability traits.
- A plan to keep the controller flow synchronous and offload only host execution
  into threads.

These are the right seams.

## What is currently unstable and should be redesigned now

The codebase is still at the stage where architectural changes are cheap, and
there are several signs that the public contract is not frozen yet:

1. **The example manifest does not match the Rust schema.**
   - `examples/test.lua` uses host/module/task shapes that do not align with
     `src/manifest/*.rs`.
   - Example: `argv` vs `args`, string module references vs structured module
     selectors, SSH shape mismatch, missing auth shape.

2. **The Lua manifest surface is not yet defined.**
   - `lua/manifest.lua` is empty.
   - `lua/modules/api.lua` is empty.

3. **Module loading is not implemented.**
   - `runtime::module_load_by_name` is unfinished.
   - Module validation currently checks only whether a `run` key exists.

4. **Execution is still only a facts skeleton.**
   - There is no actual per-host executor implementation yet.
   - Local and SSH execution models are not yet separated into stable, testable
     units.

5. **The internal data model is not yet frozen.**
   - There is no normalized manifest type distinct from raw manifest.
   - There is no evaluation-plan type.
   - There is no host-plan type.
   - There is no structured event model or report model.

This is exactly the right moment to stop coding features and freeze the
architecture first.

---

## 3. Lessons from mature automation systems

## 3.1 Ansible: learn from scheduling flexibility and `become` pain

### What Ansible gets right

- By default, Ansible runs each task on all hosts before moving to the next
  task, and exposes strategy plugins that explicitly control host/task
  scheduling.[A1]
- Ansible also provides `free` and `host_pinned` strategies, including a mode
  that executes tasks on each host without interruption.[A2][A3]

### What this means for Wali

Your proposed model is closest to **Ansible host-pinned strategy**, not
Ansible’s default linear strategy.

That is good.

For a small automation tool, the cleanest default is:

- compile a single global plan,
- derive one ordered host plan per host,
- run each host plan independently in its own thread,
- do **not** coordinate hosts at task boundaries unless explicitly required
  later.

This prevents one slow host from stalling all others, which is exactly the
benefit Ansible documents for non-linear strategies.[A2][A3]

### What Ansible teaches about privilege escalation failure modes

Ansible’s own documentation shows why `become` is such a sharp edge:

- When becoming another **unprivileged** user, temporary module-file handling
  becomes complicated and can require ACL tricks or even world-readable temp
  files. [A4][A5]
- Ansible explicitly documents that `world_readable_temp` makes temp files
  readable by any user on the system.[A5]
- It documents environment surprises with `pam_systemd`, especially around
  `XDG_RUNTIME_DIR` and user-session semantics.[A6]
- It documents that only one escalation method may be enabled per host and that
  escalation methods cannot be chained.[A7]
- It also notes that privilege escalation must be general rather than
  command-limited, because module execution occurs through changing temp file
  names, not a fixed command path.[A7]

### What Wali should take from this

Do **not** replicate generic `become` semantics.

Wali should instead implement a narrower and safer `run_as` contract:

- no method chaining,
- no interactive password prompting,
- no temp-file sharing tricks between unrelated unprivileged users,
- no reliance on inherited session environments,
- no assumption that arbitrary commands can be whitelisted externally and still
  work.

This should be a deliberate security subsystem, not a task option glued onto
command execution.

---

## 3.2 Chef: learn from resource contracts, why-run, and run locking

### What Chef gets right

Chef’s resource model is useful because resources are expected to describe
desired state, define actions, and support guards for idempotence.[C1][C2]

Chef also makes several architectural lessons explicit:

- Common resource functionality includes **guards**, **notifications**, and
  **idempotence-oriented behavior**.[C2][C3]
- Guards (`only_if`, `not_if`) exist specifically to let a resource test desired
  state and avoid unnecessary changes.[C2]
- Notifications can be queued (`:delayed`) or immediate, and are explicit
  relationships between resources.[C4]
- Chef supports **why-run** / no-op mode and documents both its usefulness and
  its limitations.[C5]
- Chef uses a **lock file** so only one run is active at a time on a node.[C5]
- Chef later introduced **Unified Mode** specifically to reduce the complexity
  created by split compile/converge semantics.[C6]

### What Wali should take from this

1. **Idempotence must be a module contract, not a convention.** Each module
   should have a stable interface for:
   - validating args,
   - probing current state,
   - deciding whether change is needed,
   - applying change,
   - reporting whether the system changed.

2. **Dry-run must be a first-class design target.** Wali should support a
   dry-run mode early, but with explicit honesty:
   - predicates may be evaluable,
   - probes may be exact,
   - some modules may be able to predict change precisely,
   - others may return “unknown” or “assumed change”.

3. **Guards must be side-effect free.** Chef’s why-run docs explicitly warn that
   assumptions are made around guards, and that guard commands might still be
   written in unsafe ways.[C5] Wali should not depend on user-authored “guard
   commands” for correctness. Keep user-facing `when` predicates declarative and
   pure.

4. **Single active run per host.** Even though Wali is controller-driven rather
   than agent-driven, you still want Chef’s run-lock idea per host execution
   context.[C5] Never allow two Wali runs to concurrently mutate the same host
   through the same controller identity unless a future strategy deliberately
   introduces that.

5. **Avoid dual semantic worlds.** Chef’s Unified Mode exists because
   compile-time vs converge-time behavior was hard to reason about.[C6] Wali
   should keep exactly two worlds:
   - pure planning,
   - impure execution.

Nothing in user-authored declaration should blur those boundaries.

---

## 3.3 Puppet: learn from catalog compilation, desired-state comparison, and refresh semantics

### What Puppet gets right

Puppet explicitly separates **catalog compilation** from **catalog
application**.[P1]

It also frames automation in a clear desired-state model:

- a resource declaration tells Puppet to manage a resource’s state,
- Puppet applies a compiled catalog by reading actual state and comparing it to
  desired state.[P2]
- Relationships and ordering are part of the catalog model.[P3]
- Puppet can run in no-op mode and report what would have changed.[P4]
- Refresh/notification semantics are explicit and structured.[P3][P5]

### What Wali should take from this

1. **The normalized manifest is not enough.** You want a distinct **compiled
   plan** that resembles a catalog/lowstate:
   - IDs are stable,
   - dependencies are explicit,
   - targets are resolved,
   - conditions are represented as evaluable nodes,
   - execution intent is fixed.

2. **The report should compare actual vs intended outcome.** Every task result
   should explicitly answer:
   - was it eligible?
   - was it evaluated?
   - was it skipped?
   - was it in sync?
   - did it change state?
   - did it fail?
   - what dependent tasks were blocked?

3. **If you later add triggers/notifications, make them explicit.** Do **not**
   infer restart/reload behavior from side effects. Make every cross-task
   reactive behavior explicit in the plan graph.

For v1, the safest choice is to **omit automatic notification/refresh semantics
entirely** and rely only on dependencies plus explicit tasks.

---

## 3.4 Salt: learn from highstate -> lowstate compilation and event-driven reporting

### What Salt gets right

Salt’s documentation is especially aligned with your proposed architecture:

- human-friendly state is compiled into machine-friendly **low state**.[S1]
- the system emphasizes finite, deterministic ordering through requisites and
  lowstate evaluation.[S2][S3]
- Salt has an **event bus** for inter-process and network transport, and
  supports returning/logging events via returners.[S4][S5]
- Salt’s docs also explicitly warn against letting the declaration/rendering
  phase mutate the underlying system, because that breaks dry-run assumptions
  and maintainability.[S1]

### What Wali should take from this

1. **Your “normalized manifest -> evaluation plan” split is correct.** This is
   the same family of idea as Salt’s highstate -> lowstate compilation.[S1]

2. **The event channel is not just logging.** It is a first-class interface
   between executor and controller:
   - structured,
   - typed,
   - stable,
   - replayable,
   - reducible into final reports.

3. **Lua declaration must not mutate state.** Salt’s warning about side effects
   during rendering is directly relevant.[S1] Wali manifest evaluation should be
   pure:
   - no shell execution,
   - no SSH,
   - no filesystem mutation,
   - no network access beyond controlled module resolution if you explicitly
     allow it in a separate resolver stage.

This is non-negotiable if you want reproducibility and trustworthy dry-run
behavior.

---

## 3.5 Nix: learn from plan immutability and graph identity

Nix is not a remote automation tool, but one design idea is useful: derivations
represent a build-time dependency graph in a machine-friendly form, and Nix can
also perform determinism checks.[N1][N2]

### What Wali should take from this

Use a **content-hashable plan artifact**.

For every run, Wali should be able to emit:

- normalized manifest JSON,
- compiled global plan JSON,
- per-host plan JSON,
- plan hash.

That gives you:

- reproducibility,
- diffability,
- regression testing,
- strong debugging,
- future caching opportunities.

---

## 4. Wali target definition

## Primary goal

A **tiny, explicit, predictable automation tool** for local and SSH-driven host
tasks, with embedded Lua for manifest declaration and module logic.

## Operational model

- Push-based controller.
- Single controller process.
- Synchronous single-threaded planning.
- Per-host threaded execution.
- Event-based aggregation back to the controller.
- Final compiled report.

## Non-goals for v1

Do not build these yet:

- dynamic inventory,
- variable precedence stacks like Ansible,
- implicit templating engine complexity,
- background agents,
- rolling deployment orchestration,
- distributed locks across multiple controllers,
- async jobs on hosts,
- cross-host dependencies,
- automatic handler/refresh semantics,
- secrets management beyond explicit input injection,
- Windows support,
- every privilege escalation backend under the sun.

The system should stay intentionally narrow.

---

## 5. Recommended architecture

## 5.1 Pipeline

### Stage 1: Raw Manifest

Input:

- manifest bytes,
- manifest file path,
- module source declarations.

Output:

- `RawManifest`.

Properties:

- syntax-level only,
- no defaults,
- no path resolution,
- no dependency checks,
- no host selection expansion.

### Stage 2: Normalized Manifest

Input:

- `RawManifest`,
- controller context,
- resolved module sources.

Output:

- `NormalizedManifest`.

Properties:

- all IDs canonicalized,
- all default values applied,
- paths resolved,
- module references canonicalized,
- duplicate IDs rejected,
- schema and semantic validation performed,
- user-facing ambiguities removed.

### Stage 3: Evaluation Plan

Input:

- `NormalizedManifest`.

Output:

- `EvaluationPlan`.

Properties:

- machine-friendly graph,
- task DAG with explicit nodes,
- static dependencies resolved,
- host selectors preserved in compiled form,
- `when` predicates compiled into pure predicate instructions,
- execution contexts attached symbolically,
- no host facts fetched yet.

### Stage 4: Per-Host Plan

Input:

- `EvaluationPlan`,
- host definition.

Output:

- `HostPlan`.

Properties:

- only tasks relevant to that host remain,
- dependency graph pruned for that host,
- `run_as` contexts resolved symbolically,
- preflight requirements attached,
- stable task order finalized,
- host plan is serializable and hashable.

### Stage 5: Executor Run

Input:

- `HostPlan`,
- transport/session,
- module runtime.

Output:

- event stream.

Properties:

- executor never parses raw manifest,
- executor never invents plan changes,
- executor only executes compiled steps and emits structured events.

### Stage 6: Report Compilation

Input:

- global plan snapshot,
- host plan snapshots,
- structured event stream.

Output:

- final report(s).

Properties:

- pure reduction,
- deterministic from inputs,
- renderable to human text and JSON.

---

## 5.2 Default execution strategy

### Recommended default: `host_pinned`

Wali should implement **exactly one** execution strategy in v1:

- plan per host,
- run each host independently,
- preserve strict ordering within each host,
- no host waits for other hosts at task boundaries,
- controller only aggregates events and final states.

This is the best fit for your current design and the smallest mental model.

### Failure semantics

For v1:

- **task failure** blocks downstream dependent tasks on the same host,
- **transport failure** aborts the host,
- **one host failing must not stop unrelated hosts**,
- **controller failure** aborts all runs,
- **Ctrl-C** becomes cooperative cancellation:
  - stop scheduling new steps,
  - let in-flight step finish if possible,
  - close report cleanly.

### Concurrency policy

Only one concurrency axis in v1:

- **across hosts**

No parallelism:

- within a host,
- within a task,
- within a module.

That simplicity is a feature.

---

## 6. Strict responsibility model

## 6.1 Responsibility matrix

| Component          | Owns                                                                    | Must never own                                |
| ------------------ | ----------------------------------------------------------------------- | --------------------------------------------- |
| CLI                | parsing user input, selecting manifest, output mode                     | planning logic, host execution                |
| Manifest Loader    | file loading, Lua evaluation into raw data                              | defaults, host probing, execution             |
| Normalizer         | defaults, canonicalization, semantic validation                         | network/session I/O                           |
| Module Resolver    | locating builtin/path/git modules, cache preparation                    | task execution                                |
| Planner            | DAG construction, dependency validation, host applicability compilation | SSH/local command execution                   |
| Host Plan Compiler | host-specific pruning and ordering                                      | remote mutation                               |
| Transport          | local process exec, SSH channel/SFTP mechanics                          | task semantics, report building               |
| Privilege Engine   | `run_as` policy resolution and preflight                                | task/module business logic                    |
| Module Runtime     | argument schema checks, probe/apply calls                               | scheduling, host selection, report formatting |
| Executor           | host-local step execution, retries within policy, event emission        | parsing manifests, changing plan              |
| Event Collector    | ordered ingestion, buffering, fan-in                                    | execution decisions                           |
| Reporter           | reduce events into summaries and artifacts                              | execution                                     |
| Cache Store        | module cache, plan snapshots, temp paths                                | policy decisions                              |

## 6.2 Strong ownership rules

### Rule 1: executors are dumb

Executors should receive a compiled host plan and execute it. They should not:

- resolve module sources,
- reinterpret selectors,
- decide new dependencies,
- invent task IDs,
- reinterpret manifest defaults.

### Rule 2: planners are pure

Planning must be deterministic for the same manifest + module set + controller
options.

### Rule 3: reporters are pure

The final report must be derivable from event stream + plan snapshot. Do not
scrape logs to decide outcome.

### Rule 4: privilege logic is centralized

`run_as` logic must live in one subsystem with one policy. Do not let modules or
transports implement their own privilege switching ad hoc.

### Rule 5: module API is capability-scoped

Modules should receive a limited API surface, not raw access to everything.

---

## 7. Data model to freeze now

## 7.1 Manifest layers

### Raw types

Keep them close to user input and Lua deserialization.

### Normalized types

Recommended fields:

```rust
struct NormalizedManifest {
    manifest_id: String,
    name: String,
    root_dir: PathBuf,
    hosts: Vec<HostSpec>,
    modules: ModuleRegistrySpec,
    tasks: Vec<TaskSpec>,
    defaults: Defaults,
}
```

```rust
struct TaskSpec {
    id: String,
    description: Option<String>,
    tags: BTreeSet<String>,
    depends_on: Vec<String>,
    host_selector: HostSelectorExpr,
    when: Option<PredicateExpr>,
    module_ref: ModuleRef,
    args: serde_json::Value,
    exec: ExecContextRef,
    policy: TaskPolicy,
}
```

```rust
struct ExecContextRef {
    run_as: RunAsRequest,
    cwd: Option<HostPathSpec>,
    env: EnvPolicy,
    timeout: Option<Duration>,
}
```

## 7.2 Plan layers

```rust
struct EvaluationPlan {
    plan_id: String,
    manifest_hash: String,
    nodes: Vec<PlanNode>,
    edges: Vec<PlanEdge>,
}
```

```rust
enum PlanNodeKind {
    PreflightFacts,
    PreflightRunAs { context_id: String },
    TaskGate { task_id: String, predicate: PredicateProgram },
    TaskProbe { task_id: String },
    TaskApply { task_id: String },
}
```

```rust
struct HostPlan {
    host_id: String,
    plan_hash: String,
    steps: Vec<HostStep>,
    dependencies: Vec<HostDependency>,
}
```

The exact shapes can differ, but the **layering** should not.

## 7.3 Event model

This should be a stable internal contract:

```rust
enum Event {
    RunStarted { run_id: String, host_count: usize },
    HostStarted { host_id: String },
    HostPreflightStarted { host_id: String },
    HostFactsResolved { host_id: String, facts: HostFactsSnapshot },
    RunAsPreflightPassed { host_id: String, context_id: String, effective_user: String },
    RunAsPreflightFailed { host_id: String, context_id: String, reason: String },
    TaskStarted { host_id: String, task_id: String, step: u32 },
    TaskSkipped { host_id: String, task_id: String, reason: SkipReason },
    TaskProbed { host_id: String, task_id: String, in_sync: bool, details: ProbeDetails },
    TaskChanged { host_id: String, task_id: String, details: ChangeDetails },
    TaskNoChange { host_id: String, task_id: String },
    TaskFailed { host_id: String, task_id: String, error: TaskError },
    OutputChunk { host_id: String, task_id: Option<String>, stream: Stream, chunk: Vec<u8> },
    HostFinished { host_id: String, status: HostStatus },
    RunFinished { summary: RunSummary },
}
```

Requirements:

- append-only,
- versioned,
- serializable,
- safe to persist,
- sequence-numbered.

---

## 8. Manifest and Lua design

## 8.1 Split Lua into two runtimes

### Runtime A: manifest runtime

Purpose:

- load manifest declarations,
- build raw data structures,
- import helper libraries.

Must be:

- side-effect free,
- deterministic,
- capability-restricted.

Do **not** allow:

- shell execution,
- filesystem mutation,
- SSH,
- arbitrary networking,
- environment mutation.

### Runtime B: module runtime

Purpose:

- implement module logic against explicit executor capabilities.

May be allowed:

- host read/write actions **only through the provided API**.

Do **not** expose:

- arbitrary process spawning outside the controlled executor API,
- raw SSH session internals,
- raw filesystem/network APIs by default.

## 8.2 Module ABI

Recommended stable module contract:

```lua
return {
  schema = function()
    return {
      -- optional arg schema / defaults / docs
    }
  end,

  normalize = function(args)
    -- pure, no host access, returns normalized args
    return args
  end,

  probe = function(ctx, args)
    -- read-only host inspection
    -- returns { in_sync = bool, diff = ..., message = ... }
  end,

  apply = function(ctx, args)
    -- performs change if needed
    -- returns { changed = bool, message = ..., outputs = ... }
  end,
}
```

### Why this shape

- `normalize` gives you canonical args early.
- `probe` is the idempotence anchor.
- `apply` is the mutation boundary.
- `schema` lets you validate and document modules.

For tiny Wali, this is enough.

## 8.3 Builtin vs Lua modules

Recommended rule:

- **Builtin Rust modules** for:
  - file write,
  - file link,
  - mkdir,
  - package manager wrappers,
  - service actions,
  - user/group primitives,
  - command/shell escape hatch.

- **Lua modules** for:
  - higher-level composition,
  - domain-specific workflows,
  - reusable task logic.

Do not start with Git-downloaded Lua modules as a runtime feature until:

- cache format,
- lock file,
- update policy,
- integrity verification

are defined.

Support `path` modules first. Support `git` modules later with a local cache
lock file.

---

## 9. Conditions, idempotence, and planning semantics

## 9.1 `when` must stay declarative

Your current `When` enum is a good start because it is **typed**.

That should continue.

Do not let `when` become “run an arbitrary command and parse it”. That path
leads to Chef/Ansible-style guard ambiguity.

Recommended classes of predicates:

- static/controller predicates,
- host fact predicates,
- host existence predicates,
- environment predicates,
- simple command/path existence checks.

These should compile into a small predicate program.

## 9.2 Evaluation timing

Not every predicate can be resolved during planning.

Use three evaluation moments:

1. **compile-time**  
   only for purely manifest/controller-local facts.

2. **host preflight-time**  
   for host facts and run-as preflight prerequisites.

3. **task gate-time**  
   for conditions that depend on current host state immediately before task
   execution.

This preserves your pipeline without pretending everything is knowable up front.

## 9.3 Generic command/shell tasks

A tiny tool still needs an escape hatch.

But the rule should be:

- `command` and `shell` exist,
- they are explicitly marked as **imperative** modules,
- they are not treated as strongly idempotent unless paired with explicit probe
  logic,
- dry-run may report “unknown change potential”.

This keeps the system honest.

---

## 10. `run_as`: the safe design

This is the most important subsystem to narrow.

## 10.1 Design principle

`run_as` is not “become but simpler”.

`run_as` should be a **capability-controlled execution context** with strict
policy and deterministic semantics.

## 10.2 v1 supported modes

Recommended for v1:

```text
inherit   -> execute as connection identity
root      -> execute as root via configured method
user:X    -> execute as named user X only if explicitly allowed
```

No UID literals yet. No arbitrary flags. No chained methods. No password
prompts. No TTY-dependent flows.

## 10.3 Host privilege policy

Each host should declare a privilege policy, separate from tasks:

```lua
privilege = {
  method = "sudo",
  allow = { "root", "deploy", "app" },
  require_non_interactive = true,
  env_mode = "reset",
}
```

### Why this matters

A task should not be able to invent a new escalation policy. Tasks request an
execution context; hosts declare whether that context is allowed.

## 10.4 Preflight protocol

Before executing any task that requires `run_as`, the host executor should
verify the context once:

1. verify escalation method exists,
2. verify it works non-interactively,
3. verify target user exists,
4. resolve target UID/GID/HOME/SHELL,
5. verify cwd policy if configured,
6. cache the resolved effective context.

Suggested check example for sudo-based backends:

- `sudo -n -u <user> -- true`
- `sudo -n -u <user> -- sh -c 'id -u; id -g; printf "%s" "$HOME"'`

If preflight fails:

- mark all dependent tasks on that host as blocked/failed according to policy,
- do not attempt per-task fallback improvisation.

## 10.5 Environment policy

Default should be:

- reset environment,
- explicitly reconstruct a minimal environment,
- only pass allowlisted variables.

Recommended minimal env:

- `HOME`
- `USER`
- `LOGNAME`
- `PATH`
- `LANG`
- `LC_ALL`
- `WALI_*`

Do not inherit arbitrary session env from the controller or connection user.
This directly avoids the kind of surprises Ansible documents around
`pam_systemd` and user session variables.[A6]

## 10.6 Working directory policy

Never inherit the controller cwd.

Per task:

- if `cwd` is explicitly set, validate it,
- else default to target user’s home or host-configured safe working dir.

This avoids silent privilege/context drift.

## 10.7 File and temp strategy

This is where Ansible gets burned.

Recommended Wali rules:

1. **Do not upload executable module files for every task if you can avoid it.**
   Prefer:
   - inline command execution,
   - structured RPC-like operations,
   - stdin-fed scripts,
   - stable helper execution paths only when absolutely necessary.

2. **Never make temp files world-readable to satisfy `run_as`.** If safe
   ownership/ACL transfer is not possible, fail.

3. **For arbitrary unprivileged-to-unprivileged switching, require either:**
   - connection user is root, or
   - verified non-interactive sudo to the target user with safe temp/exec
     semantics.

4. **Do not support password prompts in v1.** A tiny automation tool should not
   become an interactive privilege-broker.

## 10.8 Recommended restriction for v1

Strong recommendation:

- fully support `inherit`,
- fully support `root`,
- support `user:<name>` **only** when:
  - host policy allows it,
  - escalation preflight succeeds non-interactively,
  - execution does not require unsafe temp artifact exposure.

If these conditions are not met, fail fast.

This is much better than pretending generic `become` is solved.

## 10.9 Auditability

Every task event must record:

- requested `run_as`,
- effective user,
- effective uid/gid,
- method used,
- whether env was reset,
- whether cwd was defaulted or explicit.

Privilege transitions should be obvious in the report.

---

## 11. Transport and execution design

## 11.1 Transport abstraction

Use a single trait family for execution capabilities, but split implementations
clearly:

- `LocalTransport`
- `SshTransport`

Optional future:

- `PersistentSshTransport` with session reuse,
- `MockTransport` for tests.

## 11.2 Command model

Keep the existing distinction and strengthen it:

```rust
enum ExecSpec {
    Shell(String),
    Spec(Command),
}
```

But policy should be:

- `Spec(Command)` is preferred/default,
- `Shell(String)` is explicit and opt-in,
- reports should mark shell usage clearly.

## 11.3 Output handling

Never treat unbounded stdout/stderr as free.

Recommended limits:

- stream in chunks,
- retain bounded buffers in memory,
- preserve full output only when configured,
- redact known secret values before persistence.

## 11.4 Timeouts

Support:

- connect timeout,
- command timeout,
- optional idle timeout later.

Timeout results should be explicit event outcomes, not generic IO failures.

---

## 12. Reporting model

## 12.1 Internal truth = event stream + plan snapshot

Everything user-visible should be derived from:

- normalized manifest snapshot,
- plan snapshot,
- host plans,
- event stream.

## 12.2 Task outcome taxonomy

Use a fixed status set:

- `skipped`
- `blocked`
- `noop`
- `changed`
- `failed`
- `cancelled`

Optional later:

- `assumed_changed`
- `partial`

## 12.3 Recommended final reports

### v1

- human-readable terminal summary,
- machine-readable JSON report.

### later

- markdown report,
- junit/xunit style export,
- timeline view.

## 12.4 Report content

For each host:

- host metadata,
- connection summary,
- preflight result,
- task results in stable order,
- counts by status,
- duration.

For each task:

- task id,
- module,
- requested/effective run context,
- predicate result,
- probe result,
- apply result,
- changed/noop/failure,
- error data,
- bounded output snippets.

---

## 13. Reliability rules to enforce

## 13.1 Determinism rules

For the same:

- manifest,
- module sources,
- CLI flags,
- controller options,

the system should produce the same:

- normalized manifest,
- plan graph,
- host plans,
- plan hashes.

## 13.2 One-way flow

Planning data flows into execution. Execution may emit events and facts.
Execution may **not** mutate the plan.

## 13.3 Stable IDs

Every entity needs a stable ID:

- manifest id,
- host id,
- task id,
- step id,
- run id,
- event sequence number.

## 13.4 Fail fast on ambiguity

Reject:

- duplicate task IDs,
- duplicate host IDs,
- dependency cycles,
- unknown dependency references,
- unknown module references,
- unsupported `run_as` combinations,
- non-deterministic module resolution,
- mixed old/new manifest shapes.

---

## 14. Features to explicitly defer

These features generate complexity disproportionate to their value in a tiny
tool:

1. dynamic inventory,
2. variable precedence and inheritance trees,
3. automatic handlers/subscriptions,
4. async background jobs,
5. retries as a generic task option,
6. cross-host dependencies,
7. interactive prompts,
8. remote daemon mode,
9. partial graph re-entry and checkpoint resume,
10. distributed controller coordination,
11. Windows privilege escalation,
12. embedded templating beyond minimal file rendering.

---

## 15. Development roadmap

## Phase 0 — architectural reset and contract freeze

### Deliverables

- architecture decision record (ADR) set,
- frozen manifest schema v0,
- frozen normalized model v0,
- frozen event schema v0,
- frozen host-plan schema v0.

### Tasks

- remove/replace manifest example that no longer matches schema,
- define manifest Lua API,
- define module Lua API,
- define non-goals list,
- define stable error categories.

### Exit criteria

- no implementation work proceeds without these types being versioned and
  documented.

---

## Phase 1 — manifest loader and normalizer

### Deliverables

- raw manifest loader,
- manifest runtime with restricted capabilities,
- normalizer,
- semantic validation.

### Tasks

- implement `lua/manifest.lua`,
- path resolution rules,
- host and module reference canonicalization,
- duplicate/cycle/reference validation,
- deterministic JSON dump of normalized manifest.

### Tests

- golden manifests,
- malformed manifest rejection,
- duplicate ID rejection,
- schema migration tests.

### Exit criteria

- same input yields byte-identical normalized JSON.

---

## Phase 2 — planner and dry-run skeleton

### Deliverables

- evaluation plan compiler,
- host plan compiler,
- dry-run command that outputs plan JSON and summary.

### Tasks

- build DAG,
- compile predicates into programs,
- attach symbolic exec contexts,
- host pruning,
- blocked/skipped reasoning.

### Tests

- dependency ordering,
- cycle detection,
- host selector pruning,
- deterministic step ordering.

### Exit criteria

- can produce stable plan files with no execution backend.

---

## Phase 3 — local executor and event system

### Deliverables

- local transport,
- executor state machine,
- event collector,
- JSON report compiler.

### Tasks

- event schema implementation,
- bounded output capture,
- timeout handling,
- local task execution,
- report reducer.

### Tests

- noop/changed/failed reporting,
- output truncation,
- cancellation behavior,
- report determinism.

### Exit criteria

- local runs can complete end-to-end and produce trustworthy reports.

---

## Phase 4 — builtin module set v1

### Minimum builtin modules

- `file.write`
- `file.link`
- `dir.create`
- `command.run`
- `shell.run`
- `service.run` (only if you can support it cleanly)
- `archive.extract` (optional)
- `package.*` only if platform scope is tightly defined

### Tasks

- freeze module ABI,
- implement probe/apply contract,
- ensure dry-run honesty.

### Exit criteria

- modules return stable probe/apply results and integrate with reports.

---

## Phase 5 — SSH transport

### Deliverables

- SSH transport,
- host key policy handling,
- connection lifecycle,
- remote exec with bounded output.

### Tasks

- strict host key checking as default,
- safe “allow add” development mode,
- no silent ignore mode except explicit opt-in,
- timeout mapping,
- per-host session lifecycle.

### Tests

- integration tests against containerized sshd,
- host key mismatch handling,
- reconnect failure handling,
- remote cwd/env behavior.

### Exit criteria

- local and SSH transports produce equivalent event semantics.

---

## Phase 6 — `run_as` v1

### Deliverables

- privilege policy model,
- preflight verifier,
- safe root execution,
- restricted named-user execution.

### Tasks

- host privilege declarations,
- context cache per host,
- env reset rules,
- cwd policy,
- non-interactive only,
- clear report/audit fields.

### Tests

- inherit/root/user modes,
- denied user,
- failed preflight,
- missing sudo/doas,
- temp artifact safety checks,
- session env correctness.

### Exit criteria

- no unsafe fallback paths remain.

---

## Phase 7 — module source resolution and caching

### Deliverables

- local path modules,
- cache layout,
- module lock file,
- optional git module resolver.

### Tasks

- deterministic cache path scheme,
- update policy,
- offline behavior,
- integrity metadata.

### Exit criteria

- module source resolution no longer affects planner determinism unexpectedly.

---

## Phase 8 — hardening, docs, and usability

### Deliverables

- architecture docs,
- manifest reference,
- module author guide,
- troubleshooting guide,
- stable markdown/JSON reporting.

### Tasks

- secret redaction,
- performance profiling,
- plan/report snapshot regression tests,
- CLI ergonomics,
- error message cleanup.

### Exit criteria

- v1 release candidate.

---

## 16. Testing strategy

## 16.1 Unit tests

- predicate compilation and evaluation,
- selector matching,
- normalization rules,
- dependency graph rules,
- report reduction.

## 16.2 Golden tests

Store fixtures for:

- raw manifest,
- normalized manifest JSON,
- evaluation plan JSON,
- host plan JSON,
- final report JSON.

These should be stable regression anchors.

## 16.3 Integration tests

### Local

- temp dir based filesystem changes,
- user/env/cwd behavior,
- timeout behavior.

### SSH

- containerized sshd,
- host key policy,
- remote command execution,
- output handling.

### Privilege

- controlled sudo policy in test containers,
- root and named-user contexts,
- denied escalation,
- env reset validation.

## 16.4 Property tests

Good candidates:

- topological ordering,
- dependency pruning,
- event reduction invariants,
- plan determinism.

## 16.5 Chaos/failure tests

Inject:

- connection drop,
- timeout,
- broken stdout stream,
- permission denied,
- missing target path,
- cancelled run.

You want the report to remain structurally correct even when execution is
chaotic.

---

## 17. Preferred strong decisions

Make these decisions now.

1. **Side-effect free manifest evaluation.**
2. **Host-pinned execution only in v1.**
3. **One plan artifact format, JSON-serializable and hashable.**
4. **One event schema, versioned.**
5. **`run_as` deny-by-default, non-interactive only.**
6. **Command argv form preferred; shell explicit.**
7. **Executors do not interpret manifests.**
8. **Dry-run support early, but honest about uncertainty.**
9. **No implicit retries.**
10. **No automatic notification/handler system in v1.**
11. **No variable precedence maze.**
12. **No shared mutable Lua runtime across executor threads.**

---

## 18. Weak decisions to avoid

Avoid these even if they seem convenient:

1. letting manifest Lua run commands or mutate files,
2. using shell strings as the default execution model,
3. letting `when` execute arbitrary command snippets,
4. allowing modules to bypass transport/privilege APIs,
5. making logs the source of truth for reports,
6. supporting every privilege backend immediately,
7. storing only human-readable output and no structured events,
8. mixing module download/update logic into task execution,
9. allowing tasks to redefine privilege policy,
10. adding retries before error taxonomy is stable,
11. adding cross-host orchestration before host-local correctness is solid.

---

## 19. Suggested repository structure

A useful crate/module layout would be:

```text
src/
  cli/
  manifest/
    raw.rs
    normalized.rs
    normalize.rs
    selector.rs
    predicate.rs
  module/
    registry.rs
    resolver.rs
    abi.rs
    builtin/
  plan/
    graph.rs
    compile.rs
    host_plan.rs
  transport/
    local.rs
    ssh.rs
    session.rs
  privilege/
    policy.rs
    context.rs
    preflight.rs
  executor/
    runner.rs
    state.rs
    events.rs
  report/
    reduce.rs
    render_json.rs
    render_text.rs
  runtime/
    manifest_lua.rs
    module_lua.rs
  cache/
  common/
```

This keeps responsibility boundaries visible.

---

## 20. Immediate actions for the current codebase

These are the first concrete actions I recommend, in order:

1. **Delete or quarantine the current example manifest** until it matches the
   real schema.
2. **Write the manifest schema document before writing more Rust code.**
3. **Implement `NormalizedManifest` as a distinct type.**
4. **Implement `EvaluationPlan` and `HostPlan` as distinct serializable types.**
5. **Design the event schema before implementing executors.**
6. **Write the `run_as` policy document before writing any escalation code.**
7. **Freeze module ABI (`schema`, `normalize`, `probe`, `apply`).**
8. **Implement local executor before SSH executor.**
9. **Implement dry-run before complex modules.**
10. **Treat git module sources as phase-2/phase-3, not day-1 runtime behavior.**

---

## 21. Final recommendation

The best version of Wali is:

- **smaller than Ansible**
- **stricter than Chef**
- **more explicit than Salt**
- **less ambitious than Puppet**
- **inspired by Nix-style plan identity**

In practice that means:

- pure declaration,
- compiled machine plan,
- host-pinned execution,
- event-sourced reporting,
- narrow privilege model,
- small module ABI,
- strong non-goals.

If you preserve those boundaries, Wali can stay tiny and still be reliable.

---

## 22. Source notes

### Ansible

- [A1] Ansible strategies: default behavior and strategy control  
  https://docs.ansible.com/projects/ansible/latest/playbook_guide/playbooks_strategies.html
- [A2] Ansible `free` strategy  
  https://docs.ansible.com/projects/ansible/latest/collections/ansible/builtin/free_strategy.html
- [A3] Ansible `host_pinned` strategy  
  https://docs.ansible.com/projects/ansible/latest/collections/ansible/builtin/host_pinned_strategy.html
- [A4] Ansible privilege escalation docs  
  https://docs.ansible.com/projects/ansible/latest/playbook_guide/playbooks_privilege_escalation.html
- [A5] Ansible shell plugin temp-file behavior  
  https://docs.ansible.com/projects/ansible/latest/collections/ansible/builtin/sh_shell.html
- [A6] `pam_systemd` / environment caveats in Ansible become docs  
  https://docs.ansible.com/projects/ansible/latest/playbook_guide/playbooks_privilege_escalation.html
- [A7] “Only one method may be enabled per host” / “Privilege escalation must be
  general”  
  https://docs.ansible.com/projects/ansible/latest/playbook_guide/playbooks_privilege_escalation.html

### Chef

- [C1] Chef resources overview  
  https://docs.chef.io/client/19.1/resources/
- [C2] Chef common resource functionality / guards  
  https://docs.chef.io/resource_common/
- [C3] Chef resources reference  
  https://docs.chef.io/resources/
- [C4] Chef notifications and subscriptions  
  https://docs.chef.io/resources/chocolatey_installer/
- [C5] Chef client `why-run`, lock file, and local mode  
  https://docs.chef.io/ctl_chef_client/
- [C6] Chef Unified Mode  
  https://docs.chef.io/unified_mode/

### Puppet

- [P1] Puppet catalog compilation  
  https://help.puppet.com/core/8/content/puppetcore/subsystem_catalog_compilation.htm
- [P2] Puppet resources and desired-state comparison  
  https://help.puppet.com/core/8/Content/PuppetCore/lang_resources.htm
- [P3] Puppet relationships and ordering  
  https://help.puppet.com/core/8/Content/PuppetCore/lang_relationships.htm
- [P4] Puppet noop / no-op behavior  
  https://help.puppet.com/core/current/Content/PuppetCore/metaparameter.htm
- [P5] Puppet notification glossary  
  https://help.puppet.com/core/8/content/puppetcore/glossary.htm

### Salt

- [S1] Salt formulas and highstate -> lowstate compilation guidance  
  https://docs.saltproject.io/en/3006/topics/development/conventions/formulas.html
- [S2] Salt state system layers  
  https://docs.saltproject.io/en/3006/ref/states/layers.html
- [S3] Salt compiler ordering / requisites  
  https://docs.saltproject.io/en/3007/ref/states/compiler_ordering.html
- [S4] Salt event system  
  https://docs.saltproject.io/en/3007/topics/event/events.html
- [S5] Salt `event_return` configuration  
  https://docs.saltproject.io/en/3006/ref/configuration/master.html

### Nix

- [N1] Nix derivation graph representation  
  https://nix.dev/manual/nix/2.20/command-ref/new-cli/nix3-derivation-show
- [N2] Nix determinism / reproducibility checking  
  https://nix.dev/manual/nix/2.25/advanced-topics/diff-hook
