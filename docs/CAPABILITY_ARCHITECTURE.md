# Capability Kernel Architecture

## Purpose

capability kernel is GVYA's deterministic boundary between **what the conversation means** and **what the host may be asked to do**.

It intentionally does not execute capabilities. A game engine, robot service, application, device firmware bridge or other embedding host remains the authority that implements and executes them.

## Module layout

`crates/gvya-kernel/src/capability/`

- `schema.rs` — bounded compiler-produced executable value-schema IR and validators;
- `binding.rs` — deterministic source-to-argument binding and response/behavior/meaning triggers;
- `catalog.rs` — compiled contract/binding/policy catalog, normalization and validation;
- `policy.rs` — deterministic admission predicates and allow/deny/confirmation policy;
- `engine.rs` — trigger scan, binding, schema gate, availability, policy, confirmation, proposal creation, host-result validation;
- `mod.rs` — public capability kernel surface.

Shared language-neutral boundary values are in `crates/gvya-model/src/lib.rs`.

## Capability contract versus executable schema

The domain model retains an opaque self-contained source `SchemaDocument`. capability kernel does **not** define the source compiler yet. Instead it defines `ValueSchema`, the bounded executable shape consumed by the runtime kernel.

compiler/artifact layer owns canonical JSON Schema source parsing/canonicalization into this IR. Runtime code must not resolve network references or interpret arbitrary source schema text.

The capability kernel executable profile currently supports:

- null / boolean / number / integer / string;
- arrays;
- object properties, required fields, additional-property policy and property count bounds;
- enum;
- `oneOf`;
- numeric/string/array/object resource bounds.

The profile is intentionally bounded and dependency-free. Unsupported source features are rejected or compiled away by the canonical compiler, never silently ignored by runtime.

## Binding

A binding has:

- stable binding ID;
- trigger constraints: Meaning, Behavior and/or selected Response;
- target Capability ID;
- explicit argument bindings.

Binding sources:

- semantic slot;
- semantic typed host reference;
- conversation focus reference;
- immutable host context path;
- author-state path;
- literal.

Missing optional data is omitted; the capability schema decides whether the resulting object is valid. GVYA does not fill a required argument from a heuristic guess.

Host references may be projected to an opaque ID string or `{kind,id}` object. A reference from semantic/focus state must still exist in the current visible-reference snapshot.

## Availability and versioning

`ContextSnapshot.available_capabilities` contains exact `CapabilityId + CapabilityVersion` pairs.

Admission fails if:

- no capability with that ID is currently exposed; or
- the host exposes the ID but not the exact compiled contract version.

This prevents a compiled brain from silently invoking a host implementation under a different contract version.

## Policy

Policy reads only explicit namespaces:

- bound arguments;
- immutable host context;
- author state;
- a typed read-only projection of conversation state;
- explicit system facts.

No policy reads ambient time/environment/randomness.

Matching policy ordering is deterministic:

1. higher explicit priority;
2. at equal priority, `Deny` > `RequireConfirmation` > `Allow`;
3. stable policy ID.

This allows explicit higher-priority exceptions while making equal-priority conflicts fail toward the stricter result.

`ConfirmationHint::Conditional` fails closed unless an explicit policy resolves it.

## Confirmation

Confirmation is a host round trip, not a boolean field hidden in conversation state.

An invocation proposal includes:

- proposal ID;
- exact capability ID/version;
- validated argument object;
- deterministic fingerprint of version + canonical arguments;
- trace ID.

A `ConfirmationGrant` must match the proposal ID and exact fingerprint. More than one grant for the same proposal is rejected as ambiguous. A declined grant is rejected. A changed fingerprint is stale.

Proposal identity is deliberately independent of the presence/number of confirmation grants so the same deterministic turn/call can survive a confirmation round trip. The host/SDK confirms by retrying that exact canonical turn/call with a `ConfirmationGrant`; it may refresh the host context before retrying. If refreshed context changes bound arguments, policy, availability or fingerprint, the old grant fails closed instead of authorizing the changed proposal. Studio retains the originating request and exposes this exact retry explicitly.

The capability kernel FNV hash is a deterministic correlation/staleness mechanism, **not a cryptographic authenticator**. The host owns authentication of the confirmation interaction. Artifact/signature cryptography belongs to the compiler/runtime and host trust boundary.

## Host execution and results

GVYA does not contain a host callback or platform API in the capability module.

After the host executes an admitted proposal, a `CapabilityResultInput` enters the canonical capability-result interaction. Every admitted proposal is first written to a runtime-owned, bounded `pending_capabilities` ledger in conversation state. Runtime then validates the returned result against:

- an exact, still-pending proposal receipt emitted by the runtime;
- compiled contract version;
- argument schema and deterministic fingerprint;
- success/failure envelope rules;
- optional declared output schema.

An accepted result consumes that receipt before continuation. A fabricated, modified, stale or replayed result therefore cannot trigger a result handler twice when the host round-trips the returned runtime state. The ledger is runtime-owned and bounded; it is not author-addressable state.

A valid result is then routed through the deterministic capability-result behavior catalog. The selected handler may update GVYA author-state, render a continuation response, produce Why evidence, and propose another capability through the ordinary admission path. It cannot execute the host effect itself. Invalid or stale results produce validation/Why output without entering the continuation or mutating GVYA state.

This makes the lifecycle explicit: `admitted proposal -> pending receipt -> host execution -> consume-once validated result -> deterministic continuation`, while host execution authority remains outside GVYA.

## External semantic-resolver boundary

The semantic kernel resolver output cannot directly enter capability kernel admission. The capability kernel consumes `ConversationOutcome`, current context and explicit confirmation grants. There is no resolver callback, resolver override or direct `ResolverProposal.capability` branch in the capability module.

A model-specific adapter may improve semantic interpretation upstream, but the generic resolver contract contains no Capability identity and deterministic GVYA still owns binding, schema, exact availability, policy and confirmation.

## Deliberate non-scope

capability kernel does not define:

- package composition/override rules;
- project source syntax;
- source JSON Schema compiler;
- `.gvya` artifact encoding;
- host implementation registry/callback ABI;
- durable execution queue/retry/idempotency protocols;
- Studio capability editor;
- AI capability authoring workflow.

Those belong to other explicit architecture layers.

## Canonical capability behavior inventory

This inventory is normative for the current GVYA architecture; implementation and executable tests must agree with it.

| Requirement | Canonical capability expression |
|---|---|
| selected answer can cause a host-visible operation | capability binding can trigger on exact selected `ResponseId`; output is a typed `InvocationProposal` |
| behavior/intent-associated host operation | trigger may combine Meaning, Behavior and Response constraints |
| host, not conversation engine, executes the operation | kernel emits proposals only; no callback/platform/hardware execution surface |
| structured operation arguments | deterministic binding into object-shaped compiled schema; missing required values fail schema |
| operation may use extracted semantic values | `MeaningSlot` and typed `MeaningReference` sources |
| operation may use active/focused object | `FocusReference` source, but exact reference must still be visible now |
| operation may use host facts | read-only `ContextPath` binding and policy namespaces |
| operation may use brain-owned state | read-only `AuthorStatePath` binding; mutation remains state-kernel-owned |
| host-side world/player/device state may change | must be represented by a declared host capability; no direct host-state mutation namespace |
| internal memory/stat response effects | conversation kernel author/conversation state effects; capability kernel does not duplicate them |
| old per-stat bounds/no hidden default behavior | preserved as typed author-state behavior; no capability argument/state default guessing |
| required/forbidden contextual conditions | typed deterministic policy predicates over arguments/context/author/conversation/system |
| action is only useful if host can perform it | current `ContextSnapshot` exposes exact `CapabilityId + CapabilityVersion` |
| malformed action object is diagnosed | catalog validation + binding validation + runtime input schema validation |
| action target identity is meaningful | opaque typed `HostReference(kind,id)`; labels are never authority |
| multiple same-kind references must not be guessed | ambiguous binding rejects |
| remembered object may disappear | stale reference rejects with `reference_not_visible` |
| sensitive/destructive actions may need confirmation | contract hint + deterministic policies + exact proposal confirmation grant |
| confirmation must apply to exact call | stable proposal ID + version/argument fingerprint; changed proposal becomes stale |
| policy reasons should be explainable | stable admission/confirmation trace codes + bounded reason details |
| optional neural model can help interpret user | capability kernel has no resolver authority path; it consumes already validated conversation output |
| host result may return data/success/failure | result correlation/version/envelope/output-schema validation |
| host-result validation should mutate host state | validation never mutates host state; host remains authoritative |
| accidental source/load ordering should decide behavior | compiled bindings/policies are normalized into deterministic ID order |
