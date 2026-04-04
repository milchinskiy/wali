# Tiny Agentless Automation Tool — Revised Development Document

Version: 0.2  
Status: revised architecture  
Primary language: Rust  
Manifest/runtime DSL: embedded Lua  
Execution model: agentless, local + SSH, no mandatory controller service, no remote agent, no required remote interpreter

---

## 1. Why this revision exists

The prior draft allowed too much "inventory thinking" to leak into a Lua-native design: groups, precedence ladders, inherited `run_as`, and multi-scope variable resolution. That is the same family of design that makes many automation tools hard to predict.

This revision replaces that model with a stricter one:

- **no inventory inheritance engine**,
- **no first-class groups**,
- **no host/group/default precedence tree**,
- **no hidden merge rules in the tool**,
- **no semantic difference between "defined here" and "imported from another Lua file"**.

The controller should not interpret layered inventory semantics. Lua should compose data explicitly, and the automation engine should consume only the **final resolved host objects**.

That is the central correction.

---

## 2. Core doctrine

### 2.1. Primary design goal

Build a small, deterministic automation tool that behaves like a planner/executor over explicit data, not like a configuration meta-language with a hidden inheritance engine.

### 2.2. Hard rules

1. The engine receives **flat resolved data**.
2. All composition happens **before planning**, inside Lua evaluation.
3. The engine does **not** merge host defaults, group defaults, and host overrides.
4. `tags` are **labels for selection only**, not inheritance containers.
5. `run_as` has at most **two scopes**:
   - host default,
   - resource override.
6. Variables have at most **two scopes**:
   - host variables,
   - resource-local arguments.
7. There is no concept of "later group wins" or "inventory precedence".
8. A host is a complete object, not a delta from multiple parents.
9. A manifest may use helper functions during evaluation, but the returned value must be plain data.
10. The planner operates on already-expanded resource instances.

### 2.3. Resulting mental model

The user should be able to say:

> "Lua assembled these concrete hosts. The engine selected some of them by name or tag. Then it planned resources against those hosts."

That is the whole model.

---

## 3. What to borrow from existing tools, and what to reject

### 3.1. Keep

- SSH as the default remote transport.
- Local host as a first-class target.
- Probe-first resource planning.
- Plan/apply separation.
- Deterministic DAG execution.
- Bounded concurrency.
- Optional saved plans.
- Stateless controller design.

### 3.2. Reject

- agent/server control planes,
- remote Python/Ruby runtimes,
- hidden compile/converge semantics,
- inventory precedence ladders,
- group inheritance,
- controller-owned authoritative state,
- multi-method escalation chains,
- magic session state between commands.

---

## 4. Architectural thesis

The system should be split into five layers.

### Layer 1: Manifest evaluation layer
Responsibility:
- evaluate Lua in a sandbox,
- allow `require` for local manifest libraries,
- return a plain data model.

Non-responsibility:
- no remote access,
- no probing,
- no side effects,
- no resource execution.

### Layer 2: Validation and normalization layer
Responsibility:
- validate schema,
- assign stable IDs,
- normalize selectors,
- normalize module arguments,
- freeze the evaluated data into an internal Rust model.

Non-responsibility:
- no precedence resolution,
- no group expansion,
- no hidden inheritance.

### Layer 3: Planning layer
Responsibility:
- resolve targets,
- probe host state,
- ask modules for change plans,
- build the resource DAG,
- produce a saved plan if requested.

### Layer 4: Execution layer
Responsibility:
- execute the approved plan,
- stream logs,
- honor concurrency controls,
- preserve deterministic ordering.

### Layer 5: Reporting layer
Responsibility:
- summarize changes,
- report drift or skipped work,
- emit machine-readable results,
- explain failures with exact context.

---

## 5. Primary entities

This section defines the actual domain model.

### 5.1. Manifest
A manifest is a Lua file that returns one table.

It may import helpers from other Lua files, but the engine only sees the final returned table.

### 5.2. Host
A host is the fundamental execution target.

A host is a **fully materialized object** with all fields already decided.

Required identity fields:
- `name`
- `transport`

Typical fields:
- `name`
- `transport = "local" | "ssh"`
- `address`
- `port`
- `connect_as`
- `run_as`
- `tags`
- `vars`
- `ssh`
- `policy`

Important rule:

> A host does not inherit from groups, defaults, roles, environments, or inventories inside the engine.

### 5.3. Tag
A tag is a string label attached to a host.

Examples:
- `web`
- `db`
- `debian`
- `prod`
- `canary`

Tags exist only for:
- target selection,
- filtering,
- reporting,
- optional concurrency partitioning.

Tags do **not** carry variables, `run_as`, members, or transport settings.

### 5.4. Resource
A resource describes desired state or an imperative action.

Fields:
- `id`
- `module`
- `targets`
- `args`
- `requires`
- `run_as` (optional override)
- `when` (optional predicate over host vars/facts)

### 5.5. Module
A module is a Rust implementation with a narrow versioned interface.

Kinds:
- resource module,
- action module,
- fact module.

### 5.6. Plan
A plan is the frozen result of planning.

It contains:
- manifest digest,
- module set digest,
- host resolution snapshot,
- probed current state excerpts,
- ordered operations,
- policy snapshot,
- staleness guards.

### 5.7. Execution policy
A policy controls execution, not data inheritance.

Examples:
- `parallelism`
- `fail_fast`
- `host_serial`
- `resource_timeout`
- `continue_on_error`

Policy should be defined:
- globally for the run,
- optionally per host,
- optionally per resource only where the field makes sense.

Policy is not variable inheritance.

---

## 6. Relations between entities

### 6.1. Host ↔ Tag
Many-to-many in practice, but tags are plain labels embedded in the host.

### 6.2. Resource ↔ Host
Resolved by selectors.

The selector system should stay small.

Allowed selectors:
- explicit host name,
- tag selector,
- `all`,
- exclusion forms.

Examples:
- `"host:web-1"`
- `"tag:web"`
- `"all"`
- `"!host:db-2"`
- `"!tag:canary"`

### 6.3. Resource ↔ Resource
Resources may depend on other resources by stable IDs.

Dependencies must be explicit and validated before apply.

### 6.4. Host ↔ Policy
A host may embed a host-default policy.

This affects execution behavior for that host, but does not change any other host.

### 6.5. Host ↔ Variables
A host owns its own `vars` object.

The engine reads it as-is.

There is no internal merge of defaults, groups, environments, or roles.

---

## 7. Manifest model

## 7.1. The only acceptable composition model

Lua may be used to build hosts from reusable pieces, but that composition must happen **in Lua**, explicitly, before the data reaches the engine.

That means this is acceptable:

```lua
local common = require("common")
local mk_host = require("lib.host").mk_host

return {
  hosts = {
    mk_host({
      name = "web-1",
      address = "10.0.0.10",
      tags = { "web", "debian", "prod" },
      vars = common.merge(common.debian_vars(), {
        server_name = "web-1.example",
      }),
      run_as = common.sudo_root(),
    }),
  },
}
```

And this is not acceptable as an engine feature:

- host belongs to groups,
- groups inject vars,
- defaults inject vars,
- host overrides some of them,
- CLI overrides others,
- precedence table explains the final value.

That model must not exist.

## 7.2. Recommended manifest shape

```lua
return {
  meta = {
    name = "example",
    version = 1,
  },

  hosts = {
    {
      name = "local",
      transport = "local",
      tags = { "dev" },
      vars = {
        pkg_backend = "apt",
      },
      policy = {
        parallelism = 1,
      },
    },

    {
      name = "web-1",
      transport = "ssh",
      address = "10.0.0.10",
      port = 22,
      connect_as = "deploy",
      run_as = {
        method = "sudo",
        user = "root",
      },
      tags = { "web", "debian", "prod" },
      vars = {
        pkg_backend = "apt",
        service_name = "nginx",
        server_name = "web-1.example",
      },
      ssh = {
        config = "~/.ssh/config",
        control_master = true,
      },
      policy = {
        parallelism = 4,
        fail_fast = false,
      },
    },
  },

  resources = {
    {
      id = "pkg:nginx",
      module = "package",
      targets = { "tag:web" },
      args = {
        name = "nginx",
        state = "present",
      },
    },

    {
      id = "tpl:nginx",
      module = "template",
      targets = { "tag:web" },
      requires = { "pkg:nginx" },
      args = {
        src = "templates/nginx.conf.tpl",
        dest = "/etc/nginx/nginx.conf",
        mode = "0644",
      },
    },

    {
      id = "svc:nginx",
      module = "service",
      targets = { "tag:web" },
      requires = { "tpl:nginx" },
      args = {
        name = "nginx",
        enabled = true,
        state = "running",
      },
    },
  },
}
```

---

## 8. Variable model

This must stay brutally simple.

## 8.1. Rule

The engine sees only:
- `host.vars`
- `resource.args`

That is all.

## 8.2. Consequence

There is no engine-level variable precedence tree.

If users want reusable variables, they can write Lua helpers:

```lua
local function debian_web_vars(name)
  return {
    pkg_backend = "apt",
    service_name = "nginx",
    server_name = name,
  }
end
```

The helper returns a final table. The engine does not know or care where the data came from.

## 8.3. Allowed merge location

If deep merge exists, it should exist only in a small Lua helper library owned by the project, not in the engine core.

That keeps merge semantics:
- visible,
- testable,
- replaceable,
- local to the manifest author.

## 8.4. CLI overrides

Avoid general-purpose CLI variable override machinery in the first versions.

Reason:
- it reintroduces hidden precedence,
- it weakens reproducibility,
- it complicates saved plans.

If CLI overrides are later added, restrict them to:
- explicit host selection,
- explicit policy changes,
- explicit scalar injection with full audit visibility.

---

## 9. `run_as` model

This also must stay strict.

## 9.1. Structure

```lua
run_as = {
  method = "none" | "sudo" | "su" | "doas",
  user = "root",
  login = false,
  preserve_env = false,
  password_ref = nil,
}
```

## 9.2. Resolution model

Only two scopes exist:

1. host default `run_as`
2. resource override `run_as`

No manifest defaults. No group-level escalation. No escalation chains.

## 9.3. Effective rule

For a resource instance on a host:
- use `resource.run_as` if present,
- else use `host.run_as` if present,
- else run as the connected user.

## 9.4. Forbidden complexity

Do not allow:
- nested escalation,
- fallback from sudo to su,
- merged escalation fields from multiple scopes,
- partial escalation overrides.

A resource override replaces the whole `run_as` object.

## 9.5. Why this matters

Privilege behavior must be explainable without a precedence chart.

---

## 10. Targeting model

## 10.1. Selection only

Targeting should answer only one question:

> Which concrete hosts does this resource instance apply to?

It should not:
- inject variables,
- change policies,
- affect `run_as`,
- mutate host objects.

## 10.2. Minimal selector grammar

Support only:
- `host:<name>`
- `tag:<name>`
- `all`
- `!host:<name>`
- `!tag:<name>`

Do not support:
- nested boolean selector DSLs in the first release,
- selector-defined variables,
- selector-defined inheritance.

## 10.3. Resolution behavior

Selectors resolve after manifest evaluation and before planning.

The result is a concrete ordered host list.

Deterministic ordering rule:
- preserve manifest host order after filtering.

---

## 11. Module system

## 11.1. Module categories

### Resource modules
Stateful and plannable.

Examples:
- `package`
- `service`
- `file`
- `directory`
- `template`
- `user`
- `group`
- `symlink`
- `sysctl`

### Action modules
Imperative, maybe not exactly plannable.

Examples:
- `command`
- `script`
- `reboot`

### Fact modules
Read-only probes.

Examples:
- `os_release`
- `file_stat`
- `service_status`
- `package_status`

## 11.2. Stable contract

Conceptual Rust-side module contract:

- declare schema,
- validate args,
- probe current state,
- compute change object,
- render execution operations,
- verify result if supported.

## 11.3. Strong rule for modules

A module may inspect:
- its own args,
- host vars,
- probed facts,
- execution context.

A module may not:
- create new hosts,
- rewrite manifest data,
- mutate selector results,
- insert surprise dependencies during apply.

## 11.4. Composite modules

If composite modules exist, they must expand into a fixed subgraph during **plan time only**.

Never during apply.

---

## 12. Planning flow

The flow should be explicit and linear.

### Stage 1: Load
- read Lua manifest,
- evaluate sandbox,
- obtain plain data.

### Stage 2: Validate
- schema checks,
- duplicate ID checks,
- selector syntax checks,
- module existence checks.

### Stage 3: Normalize
- assign internal host IDs,
- normalize `run_as`,
- normalize selectors,
- freeze manifest into internal structs.

### Stage 4: Resolve targets
- select concrete hosts for each resource,
- preserve deterministic host ordering.

### Stage 5: Expand instances
- convert each resource into resource instances per target host,
- assign instance IDs such as `web-1::svc:nginx`.

### Stage 6: Probe
- collect facts required by each module,
- cache within the run,
- never treat cache as authoritative across runs unless explicitly configured.

### Stage 7: Plan
- compare desired vs current state,
- produce change objects,
- build resource instance DAG,
- validate cycles,
- compute execution batches.

### Stage 8: Review
- show plan summary,
- emit exact commands where safe,
- show effective host/user context.

### Stage 9: Apply
- execute approved operations,
- stream logs,
- track success/failure/skipped.

### Stage 10: Verify
- rerun module verification when supported,
- produce final report.

---

## 13. Execution model

## 13.1. Transport abstraction

Support two transports initially:
- `local`
- `ssh`

Both must expose the same abstract operation set:
- run command,
- upload file,
- download file,
- stat path,
- create temp path,
- remove temp path.

## 13.2. SSH strategy

Prefer using the system `ssh` and `scp` or `sftp` tools rather than embedding a large SSH stack into the core binary.

Benefits:
- smaller dependency footprint,
- native compatibility with user SSH config,
- easier behavior on unusual systems.

## 13.3. Shell state rule

Each operation is self-contained.

Do not assume a previous `cd`, `export`, or shell function still exists.

Every operation carries explicit:
- command,
- environment,
- cwd,
- timeout,
- input handling,
- `run_as` wrapper.

## 13.4. Deterministic concurrency

Concurrency should be bounded and visible.

Recommended model:
- per-run global ceiling,
- per-host serial execution by default,
- optional cross-host parallelism,
- deterministic order inside the same dependency level.

---

## 14. Saved plans

Saved plans are useful, but they must not become Terraform-style global state.

## 14.1. Plan contents

A saved plan should include:
- manifest digest,
- imported Lua file digests,
- module binary/API digest,
- target host snapshot,
- relevant probed fact digests,
- operation list,
- created timestamp,
- planner version.

## 14.2. Invalidation rules

Reject or warn on apply if:
- manifest digest changed,
- imported file digest changed,
- module version changed,
- target list changed,
- plan requires facts that are too stale,
- explicit freshness window expired.

## 14.3. Source of truth

The plan is a proposal snapshot, not the authoritative system state.

The target host remains the source of truth.

---

## 15. Failure model

Failure reporting is central to trust.

Every failure must identify:
- host,
- resource instance ID,
- module,
- stage,
- exact rendered operation,
- effective `connect_as`,
- effective `run_as`,
- exit code or transport error,
- stdout/stderr excerpts,
- whether rollback was attempted,
- whether subsequent work was skipped or continued.

## 15.1. Failure policy

Recommended defaults:
- fail current resource instance immediately,
- continue other independent hosts unless `fail_fast = true`,
- never perform hidden retries except for clearly transient transport setup if explicitly enabled.

---

## 16. Security model

## 16.1. Manifest sandbox

Allowed during Lua evaluation:
- tables,
- strings,
- math,
- local helper functions,
- `require` from approved project paths.

Forbidden:
- spawning processes,
- arbitrary file writes,
- network I/O,
- reading arbitrary environment variables,
- loading native code.

## 16.2. Secret handling

Do not encourage secrets directly in host vars.

Support secret references later through a dedicated mechanism such as:
- environment references,
- command output references,
- external secret backends behind a narrow interface.

But keep this outside the first core design.

## 16.3. Privilege boundary

The planner should show effective `run_as` before apply.

Privilege changes must be visible in both human and machine-readable plan output.

---

## 17. CLI design

Recommended initial CLI:

```text
mytool check   -f manifest.lua
mytool plan    -f manifest.lua
mytool apply   -f manifest.lua
mytool apply   --plan saved.plan
mytool show    --plan saved.plan
mytool graph   -f manifest.lua
mytool facts   -f manifest.lua --host web-1
```

## 17.1. Command semantics

### `check`
- evaluate and validate manifest,
- no probing unless requested.

### `plan`
- perform target resolution,
- probe,
- build change plan,
- optionally save plan.

### `apply`
- apply current manifest or saved plan,
- verify,
- print summary.

### `graph`
- show resource instance DAG.

### `facts`
- inspect known facts for debugging.

---

## 18. Internal Rust modules

Suggested crate/service split inside the codebase:

- `core-model`
  - manifest structs,
  - plan structs,
  - operation structs.

- `lua-eval`
  - sandbox,
  - manifest loader,
  - project `require` resolver.

- `validator`
  - schema checks,
  - selector checks,
  - normalization.

- `transport-local`
  - local execution and file ops.

- `transport-ssh`
  - system SSH adapter.

- `planner`
  - instance expansion,
  - fact gathering,
  - DAG construction,
  - change planning.

- `executor`
  - bounded parallel execution,
  - streaming logs,
  - result collection.

- `modules`
  - built-in resource/action/fact modules.

- `cli`
  - argument parsing,
  - TTY output,
  - JSON output.

---

## 19. Recommended project conventions for Lua

Because the engine must remain simple, the project should ship a tiny standard manifest helper library.

Examples:
- `lib.host.mk_host(spec)`
- `lib.merge.deep(a, b)`
- `lib.run_as.sudo_root()`
- `lib.tags.has(host, tag)`
- `lib.select.by_tag(hosts, tag)`

Important:

These helpers are **userland manifest helpers**, not controller semantics.

That means:
- they can be tested separately,
- teams can replace them,
- the engine does not need to know their meaning.

---

## 20. Caveats and tradeoffs

This stricter design buys predictability, but it deliberately gives up some convenience.

### Tradeoff 1: more explicit Lua composition
You write more explicit host construction code.

This is good.

It moves complexity into visible source code instead of hidden precedence rules.

### Tradeoff 2: less CLI mutability
You lose some flexible override patterns.

This is also good.

It preserves reproducibility and saved-plan trust.

### Tradeoff 3: tags are weaker than groups
Correct.

That is intentional. Tags classify; they do not configure.

### Tradeoff 4: some duplication may remain
Also acceptable.

A little repetition is often safer than a powerful inheritance system.

### Tradeoff 5: Lua helpers can still become messy
True.

To control that, keep helper APIs tiny and avoid building a second hidden DSL inside helper libraries.

---

## 21. Staged development plan

## Stage 0: architecture guardrails
Deliverables:
- internal ADRs defining non-goals,
- manifest schema draft,
- `run_as` strictness rules,
- selector grammar.

Success condition:
- no groups,
- no precedence tree,
- no controller-side inheritance.

## Stage 1: manifest engine
Deliverables:
- embedded Lua sandbox,
- `require` support for project-local files,
- plain-data export into Rust structs,
- validation errors with source locations.

Success condition:
- manifests can compose hosts through Lua, but the engine receives only flat data.

## Stage 2: local transport and minimal modules
Deliverables:
- local transport,
- `command`, `file`, `directory`, `template`, `service` modules,
- fact cache for one run,
- check/plan/apply skeleton.

Success condition:
- reliable local planning and application.

## Stage 3: SSH transport
Deliverables:
- system SSH adapter,
- file transfer support,
- host key handling strategy,
- timeout and stderr classification.

Success condition:
- identical planning semantics for local and remote hosts.

## Stage 4: saved plans
Deliverables:
- saved plan format,
- digests and staleness checks,
- `show --plan` output.

Success condition:
- reviewed plans can be applied safely with clear invalidation behavior.

## Stage 5: DAG and concurrency
Deliverables:
- explicit dependency graph,
- cycle detection,
- deterministic batch execution,
- bounded concurrency.

Success condition:
- repeatable ordering under concurrency.

## Stage 6: quality and hardening
Deliverables:
- JSON output mode,
- structured logs,
- error taxonomy,
- integration test harness using containers/VMs,
- NixOS-focused tests.

Success condition:
- transport, privilege, and module behavior are reproducible across minimal systems.

## Stage 7: optional advanced features
Possible later additions:
- secret backends,
- bastion/jump-host topology,
- richer selector expressions,
- reusable module packs,
- remote fact caching.

These must not violate the earlier guardrails.

---

## 22. Design decisions that should remain fixed

1. **No remote interpreter dependency.**
2. **No remote agent.**
3. **No server control plane.**
4. **No inventory inheritance engine.**
5. **No first-class groups.**
6. **Tags are selection labels, not config carriers.**
7. **A host is already final when the planner sees it.**
8. **`run_as` has only host-default and resource-override scopes.**
9. **Variable semantics are host vars plus resource args.**
10. **Planning happens before apply, and graph shape cannot change during apply.**
11. **The target host is the source of truth for current state.**
12. **Saved plans are snapshots, not authoritative state.**

---

## 23. Final recommendation

The right design is not "Ansible without agents" and not "Terraform for servers." It is a **flat-data planner** with Lua used only as an explicit composition language.

The controller should not know what a host inherited from or which abstract inventory object provided a value. That is exactly the ambiguity you want to avoid.

A professional, durable architecture for your tool is therefore:

- Lua for explicit host construction,
- Rust for validation, planning, transport, and execution,
- final host objects only,
- tags for targeting only,
- minimal `run_as` semantics,
- probe-first plannable modules,
- deterministic plan/apply workflow,
- agentless local/SSH execution.

That model is small, explainable, and much harder to turn into a mess.
