# GVYA Capability-First Domain Model — Foundation v1

This is the language-neutral conceptual model for GVYA. It defines ownership and vocabulary, not the final source-file syntax or `.gvya` binary format.

## 1. Project

A **Project** is an authoring workspace from which one or more Brains may be compiled. In Studio, standalone Matcher Profile documents define the Project's paired available languages; there is no separate global or Project language list. The Project owns Standard and Fallback Project Packages that are invisible outside that Project and one or more Bot/Brain definitions. Shared Packages remain global reusable sources; Projects neither attach nor override them. Each Brain ultimately resolves its selected Standard Packages plus at most one Shared/Project Fallback Package into an ordinary GVYA compile target.

A project may contain:

- Project-local Standard and Fallback Packages;
- Brain definitions (shown as Bots in the human Studio UI);
- Capability contracts;
- reusable schemas/types;
- policies;
- authoring tests and scenarios;
- assets;
- Project-owned paired Language/Matcher Profiles and explicitly language-tagged conversation material.

Project metadata is authoring-time information and is not automatically shipped in a `.gvya` artifact.

## 2. Package

A **Package** is a reusable source module. It may contribute one or more of:

- semantic vocabulary/rules;
- Conversation Behaviors;
- response resources;
- capability contracts or requirements;
- policies;
- state/schema fragments;
- tests/scenarios;
- assets and explicitly language-tagged conversation text.

Packages have explicit identity and explicit dependency/export boundaries. They do not gain precedence from filesystem order or accidental load order.

A Shared or Project Package selects an authoring language from its owning scope's complete Language/Matcher Profile pairs so human forms open predictably. This preference is not semantic data and is absent from canonical compiler source and artifacts. A Bot Package derives the preference from its Bot default instead of owning another setting. Package content may use several explicit languages; if any used language lacks an owning-scope Matcher Profile, Studio preserves the Package but marks it Disabled and excludes it from Bot selection.

Exact composition/override/specialization rules are intentionally deferred to package/audit/test layer.

## 3. Brain

A **Brain** is a compile target: the declared composition of packages, capability surface, policies, conversation configuration, state definitions, explicitly language-tagged conversation resources and authoring choices that become one executable brain. Studio may label a Brain as a **Bot** for human authoring. Every Bot owns exactly one structural Standard Bot Package that is created/deleted with that Bot and cannot be detached or shared with another Bot, selects a non-empty enabled runtime-language subset of its Project Language/Matcher Profile pairs plus a default inside it, may add Project-installed Standard Packages, and may select at most one Project-installed Fallback Package. Bot-local content and all Standard Package replacements live in that one Bot Package; Fallback Packages are selected whole and never overridden. If a Bot-enabled or resolved-Package language lacks a Project Language/Matcher Profile pair, Studio preserves the Bot as Disabled and exposes only its diagnostic Overview until repaired. Project-level override does not exist. These authoring scopes resolve away before compiler source is emitted.

A Brain is source. A `.gvya` file is its compiled artifact; the two are not interchangeable.

A brain may be suitable for an NPC, robot, appliance, vehicle, application, kiosk or other host. No game-specific ontology is built into the term.

## 4. Interaction

An **Interaction** is one deterministic evaluation boundary.

Conceptually it consumes:

- an `InteractionInput`;
- a `ContextSnapshot`;
- prior `GvyaState`;
- explicit environment inputs such as locale/time/seed when enabled.

It produces an `InteractionResult` containing:

- interpreted Meaning or explicit ambiguity/failure;
- Response Plan;
- zero or more admitted Invocation Proposals;
- next GVYA state or state transition;
- structured Trace.

The runtime may expose stateful ergonomic APIs later, but the semantics are defined as if the transition were explicit.

## 5. Interaction Input

Foundation input kinds are deliberately general:

- **Utterance** — human text and language metadata;
- **Host Signal** — a declared host event visible to the brain;
- **Capability Result** — outcome of a host-executed invocation proposal;
- **Confirmation Decision** — explicit user/host confirmation response when a capability requires it.

Later implementation layers may refine the event model. An utterance is never required to map to a capability.

## 6. Meaning

A **Meaning** is GVYA's structured interpretation of an interaction. It is not synonymous with a tool call.

A Meaning may represent:

- conversational/social behavior;
- a question;
- clarification or repair;
- a state/context observation request;
- a capability request;
- a compound/continuation meaning when supported later.

A Meaning has a stable author-facing ID and may carry typed Slots and References.

A Meaning may also be **partial**: the selected Meaning plus its already-bound typed values plus the authored-order list of its still-missing required declarations. A partial Meaning is a first-class outcome distinct from ambiguous and unresolved — a confidently selected Meaning that simply lacks a required value is never collapsed into a generic non-answer. Conversation, not semantics, owns what happens next; see `SEMANTIC_ARCHITECTURE.md` and `CONVERSATION_ARCHITECTURE.md`.

Resolvers propose meaning candidates; the deterministic semantic/conversation kernel decides how they participate in the final result.

## 7. Conversation Behavior

A **Conversation Behavior** is an authored rule/resource describing how a brain can react to meaning and context.

It can define:

- semantic evidence/examples/rules;
- required/forbidden context or state;
- response choices;
- conversation transitions;
- GVYA-owned state transitions;
- zero or more capability bindings;
- clarification/repair behavior;
- trace-facing author metadata.

A common behavior must remain simple to author. Advanced matching, policy and effect details are optional progressive layers rather than required boilerplate.

The final behavior schema is not frozen in domain-model layer; semantic/conversation/capability layers define executable semantics.

## 8. Slot

A **Slot** is a named typed value needed to represent a Meaning or bind a capability argument.

A slot has:

- stable name;
- declared type/schema;
- required/optional status;
- the authored localized elicitation prompts used while a required value is missing;
- provenance when resolved;
- validation status;
- ambiguity/candidate information where relevant.

A slot holds one value. There is no cardinality system.

Slot values may originate from utterance extraction, conversation state, context, a host reference resolver or an optional external structured semantic resolver. Provenance must remain inspectable.

## 9. Entity

An **Entity** is a typed semantic value recognized from input or context, such as a number, date, duration, color, enum value, free text span or domain-specific value.

Entities are values. They are not automatically host objects.

Domain-specific entity recognizers are extensible; the built-in set is not intended to be a closed list.

## 10. Host Reference

A **Host Reference** identifies an object owned by the embedding host: for example a room, device, game character, file, inventory item or account.

A reference consists conceptually of:

- `kind` — declared reference namespace/type;
- `id` — opaque stable host identity;
- optional safe display/search metadata supplied in the Context Snapshot.

GVYA does not infer authority from a display name. The host defines what references exist and which are visible/usable in a given interaction.

## 11. Context Snapshot

A **Context Snapshot** is immutable host-supplied information for one interaction.

It may contain:

- visible host facts;
- active/focused references;
- available reference candidates;
- capability availability facts;
- player/user/device/application context;
- locale/time/environment values explicitly exposed by the host.

The compiler/runtime must know the schema of context paths it depends on. Arbitrary ambient host access is forbidden.

## 12. GVYA State

**GVYA State** is runtime-owned conversational/brain state. It is separate from host state.

The conceptual state is divided into namespaces:

- **author state** — state deliberately addressable by authored behaviors;
- **conversation state** — runtime-managed continuity such as recent meaning/reference/follow-up/repair data, plus the single bounded active value collection for a partial Meaning;
- **runtime metadata** — implementation/version bookkeeping not author-addressable.

Owning runtime/persistence layers define exact mutation and persistence rules. Author rules must not be able to mutate runtime-internal metadata through arbitrary paths.

## 13. Response Plan

A **Response Plan** is what GVYA intends to communicate, independent of host rendering.

It may contain ordered response items such as:

- text variants/localized text;
- structured presentation hints;
- asset references;
- links;
- additional/conditional messages;
- host-renderable semantic payloads added by future extensions.

A Response Plan may exist with no capability proposal. It is not a side-effect channel.

## 14. Capability Contract

A **Capability Contract** declares an effectful operation the host may implement.

Foundation fields include:

- stable `CapabilityId`;
- human title/description for authoring and AI assistance;
- input schema;
- optional output schema;
- declared reference kinds used by inputs/outputs;
- effect/risk metadata;
- confirmation policy hints;
- version/compatibility identity within GVYA's own contract system.

Capability input/output shape uses a constrained self-contained JSON Schema 2020-12 source profile. Input arguments are object-shaped. External network `$ref` resolution is forbidden. capability kernel executes a bounded compiler-produced schema IR; source-schema parsing/canonicalization belongs to the compiler layer.

Capability identity is `CapabilityId + CapabilityVersion`. The host exposes exact available contract versions for each interaction. A capability contract declares *what may be requested*. It does not contain an executable host implementation.

## 15. Capability Binding

A **Capability Binding** maps resolved conversation state into arguments for one declared Capability Contract.

A binding may be triggered by a Meaning, selected Conversation Behavior, selected Response, or a conjunction of those constraints. This preserves the fact that an exact selected response can carry behavior without reducing conversation to `Meaning -> tool`.

capability kernel binding sources include resolved Slots, typed semantic/focus Host References, immutable Context paths, author-state paths and literals. Bindings are typed and validated. Missing required inputs, stale/ambiguous references or out-of-range values are not repaired by guessing.

## 16. Policy

A **Policy** is a deterministic rule that constrains whether a capability proposal may proceed in the present state/context.

Policies may address:

- context/state requirements;
- role/clearance/ownership facts supplied by the host;
- risk class;
- confirmation requirements;
- rate/sequence constraints that can be evaluated from declared inputs;
- domain-specific admission conditions.

Policy does not execute the capability.

## 17. Admission

**Admission** is the deterministic decision pipeline between a proposed capability binding and a host-visible Invocation Proposal.

capability kernel fixes the deterministic checks in this order:

1. capability is declared by the compiled brain;
2. arguments bind without unresolved required ambiguity or invalid reference;
3. bound arguments conform to the compiled schema;
4. the host snapshot exposes the exact `CapabilityId + CapabilityVersion`;
5. deterministic policy permits, denies or requires confirmation;
6. required confirmation is matched to the exact proposal/fingerprint.

Semantic ambiguity is resolved upstream and remains separately traceable. A remembered Host Reference is not invocation authority unless the current host snapshot still exposes that exact identity.

Foundation outcomes:

- `Admitted`;
- `NeedsConfirmation`;
- `Rejected`.

Clarification due to semantic ambiguity occurs before or alongside admission and remains separately traceable.

## 18. Invocation Proposal

An **Invocation Proposal** is an admitted, typed request for the host to execute a declared capability.

It contains at minimum:

- stable proposal ID for correlation/confirmation round trip;
- exact Capability ID and contract version;
- validated arguments;
- reference identities where applicable;
- deterministic version+arguments fingerprint for stale-confirmation detection;
- trace correlation ID.

The proposal fingerprint is not a cryptographic authorization token. Host security and authentication remain host-owned.

It is not evidence that execution occurred.

The host returns a Capability Result if the brain needs to observe success/failure/output.

## 19. Resolver Proposal

A **Resolver Proposal** is output from an optional semantic resolver, including a local neural model or LLM adapter.

It may contain candidate:

- Meaning ID;
- Slots;
- Host References selected only from allowed candidates or explicit unresolved reference text;
- Capability ID selected only from the declared candidate surface;
- confidence/evidence metadata.

Resolver Proposals are untrusted inputs to deterministic GVYA validation. They cannot carry executable callbacks, state mutations or admission overrides.

## 20. Trace

A **Trace** is the structured explanation record for an Interaction.

Every Trace event has:

- phase/category;
- stable machine reason code;
- human-readable summary metadata;
- correlation IDs;
- visibility/sensitivity classification;
- bounded structured details.

Trace is the source for Studio Why. It must be useful without requiring raw internal logs.

## 21. Test Scenario

A **Test Scenario** is an authoring-time executable expectation over interactions, state/context and outputs.

It can assert semantic meaning, ambiguity, response class/text constraints, capability proposals, admission outcomes, state transitions and trace reason codes.

Machine-generated scenarios have no privileged status; they are validated and compiled like human-authored tests.

## 22. Ownership table

| Concern | GVYA compiler/runtime owns | Host owns | Authoring clients own |
|---|---|---|---|
| Meaning model | yes | no | authors/proposes source |
| Conversation state | yes | persists per SDK contract | edits/tests |
| Host object identity | validates declared refs | authoritative | may help author schemas |
| Host state | no | authoritative | no |
| Capability contract | compiles/validates | implements/binds | authors/assists |
| Capability execution | no | yes | no |
| Policy/admission | deterministic decision | supplies facts | authors/tests |
| Response plan | yes | renders | authors/assists |
| Neural proposal | validates/uses as input | optional runtime resolver transport | may author resolver-safe configuration |
| Trace/Why facts | emits | may add execution result | renders/analyzes |
| `.gvya` artifact | compiler produces/runtime consumes | loads bytes | builds/inspects |
