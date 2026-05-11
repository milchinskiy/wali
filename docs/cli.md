# CLI reference

The CLI is split into four commands:

```sh
wali plan MANIFEST
wali check MANIFEST
wali apply MANIFEST
wali cleanup --state-file FILE MANIFEST
```

Global output options come before the command. Command-specific options come
after the command.

```sh
wali --json plan manifest.lua
wali --json-pretty apply --state-file apply-state.json manifest.lua
```

## Output modes

By default wali prints human-oriented output. Use `--json` for machine-readable
output and `--json-pretty` for formatted JSON:

```sh
wali --json check manifest.lua
wali --json-pretty cleanup --state-file apply-state.json manifest.lua
```

`--json` and `--json-pretty` are mutually exclusive. The short forms are `-j`
and `-J`.

## `plan`

```sh
wali plan [selectors] MANIFEST
```

`plan` loads and validates the manifest, compiles the effective per-host task
plan, applies CLI selectors, and prints the selected plan.

`plan` does not connect to hosts, fetch Git module sources, evaluate task `when`
predicates, check module `requires`, normalize module arguments, or call module
`validate` functions. It is useful for checking the shape of the work before any
host or module-source side effects are possible.

## `check`

```sh
wali check [--jobs N] [selectors] MANIFEST
```

`check` prepares module sources, resolves task modules, connects to selected
hosts, evaluates task `when` predicates, checks module `requires`, normalizes
arguments through module schemas, and calls module `validate` functions.

`check` uses a read/probe-only Lua context. Mutation APIs are unavailable and
transfer operations that would write data are rejected. A successful check means
wali could understand the manifest and validate the selected tasks against the
current host state; it is not a dry-run diff and does not predict every possible
apply-time race.

## `apply`

```sh
wali apply [--jobs N] [--state-file FILE] [selectors] MANIFEST
```

`apply` performs the same preparation and validation as `check`, then calls each
selected module's `apply` function in per-host task order.

Use `--state-file FILE` to update an apply state file:

```sh
wali apply --state-file apply-state.json manifest.lua
```

The state file contains the selected effective plan from the latest run, an
accumulated resource ledger, and the final apply report state. Wali validates an
existing state file before mutating hosts, then writes the updated document
atomically after the run has been reported. This update also happens when one or
more tasks fail: only successful task results contribute resource records, while
failed or skipped tasks do not.

When `FILE` already exists, it must be a valid Wali apply-state document. Wali
updates it instead of replacing the cleanup ledger. Existing `created` resource
records are preserved when a later apply reports the same path as `unchanged` or
`updated`, so repeated applies do not erase cleanup obligations.

## `cleanup`

```sh
wali cleanup --state-file FILE [--jobs N] [selectors] MANIFEST
```

`cleanup` reads an apply state file and removes target-host filesystem entries
recorded as `created` resources inside the current selected manifest scope. It
uses the current manifest for host connection data. Controller-side artifacts
reported by pull operations are not removed by host cleanup.

Cleanup intentionally does less than apply:

- it removes only resources recorded as `created`;
- it does not remove paths that were merely updated or unchanged;
- it does not rewrite the apply state file;
- it still respects host and task selectors.

Run `apply --state-file FILE` whenever you want to extend or refresh the cleanup
ledger. Reusing the same file is intentional: new `created` resources are added,
and previous `created` resources remain eligible for cleanup unless cleanup is
run with selectors that leave them out of scope.

## Selectors

Selectors are available on `plan`, `check`, `apply`, and `cleanup`:

```sh
wali plan --host web-1 manifest.lua
wali check --task deploy manifest.lua
wali apply --host web-1 --task deploy manifest.lua
wali cleanup --host-tag web --task-tag deploy --state-file apply-state.json manifest.lua
```

Supported selectors:

```text
--host ID       select one host id; may be repeated
-H ID           short form of --host
--host-tag TAG  select hosts with an exact tag; may be repeated
--task ID       select one task id and its dependency closure; may be repeated
-T ID           short form of --task
--task-tag TAG  select tagged tasks and their dependency closure; may be repeated
```

Repeated host id and host tag selectors are unioned. Repeated task id and task
tag selectors are unioned. Host selection and task selection are then
intersected.

Selecting a task by id or tag includes its transitive `depends_on` and
`on_change` source tasks on the same host. It does not include downstream
dependents. This makes partial applies predictable: selecting `deploy` includes
what `deploy` needs, not everything that may react to `deploy`.

For selected plans, module source preparation and validation are limited to
module sources required by the selected task modules. Builtin-only selections do
not fetch custom Git sources.

Selector values must be valid UTF-8, non-empty, without leading/trailing
whitespace or control characters.

## Host concurrency

`check`, `apply`, and `cleanup` run hosts concurrently by default. Tasks within
one host always run sequentially in dependency order.

Use `--jobs N` to cap host concurrency:

```sh
wali check --jobs 1 manifest.lua
wali apply --jobs 4 manifest.lua
wali cleanup --jobs 1 --state-file apply-state.json manifest.lua
```

`--jobs 1` runs hosts serially in manifest order. `N` must be a positive integer
greater than zero.

## Exit behavior

CLI parsing and user-facing errors are reported on stderr. JSON output is
reserved for command reports, so scripts should use the process exit status to
distinguish success from failure.
