# Agent authoring end-to-end validation

This directory is the durable end-to-end validation surface for the external-agent/human authoring workflow.

The fixtures are ordinary GVYA source snapshots. They are deliberately not a patch format, session format, or hidden authoring state. Each pair represents what an external AI or human authoring host could leave on disk before calling the canonical CLI.

`run.mjs` has two layers:

1. **Source/runtime proof** (always executed): the shipped Engine v1 WASM validates and builds every valid snapshot, rejects the malformed intermediate candidate, and exercises representative conversation, state/follow-up/topic, capability confirmation/result, mechanic-removal, and repaired-source runtime paths.
2. **Canonical process proof** (when `--gvya PATH` or `GVYA_BIN` is supplied): the runner first creates a fresh empty Bot with `gvya init bot`, proves that scaffold with `gvya check`, authors a first ordinary candidate slice, and then exercises `gvya author-step BASE CANDIDATE --json`. It verifies local incremental selection, direct mechanic-proof rejection/closure, global full-suite escalation, baseline immutability, host-owned promotion, and sequential accepted slices.

The runner does **not** reproduce mechanic detection. It treats `gvya.cli.author-step/1` / embedded `gvya.cli.check-change/1` as the authority and only asserts the expected outcome of each deliberately authored fixture.

Typical connected-environment run:

```text
node validation/authoring-e2e/run.mjs --gvya target/debug/gvya
```

Engine-only proof:

```text
node validation/authoring-e2e/run.mjs --source-proof
```

Fixture chain:

- `00-base`
- `01-meaning-behavior-{missing-proof,accepted}`
- `02-stateful-{missing-proof,accepted}`
- `03-capability-{missing-proof,accepted}`
- `04-removal-{missing-proof,accepted}`
- `05-malformed` -> `05-repaired`
- `06-failing-regression` -> `06-fixed-regression`
- `07-global-change`
- `08-sequential-a` -> `08-sequential-b`

Promotion is intentionally performed only by the validation host in a temporary directory after `promotion_allowed=true`; `author-step` itself must never mutate BASE.
