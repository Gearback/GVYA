# GVYA Architectural Constitution — Foundation v1

This document states invariants that later implementation must satisfy. It is intentionally stricter than a roadmap: a later implementation layer may refine mechanisms, but must not silently violate these principles.

## 1. Identity and independence

GVYA is an independent product. It has no runtime, source-format, schema, identifier, file-extension, or migration contract with NPCBrain or Horuph.

Historical systems may be studied as read-only research oracles to avoid losing proven behavior and UX knowledge. Their names and formats have no authority over GVYA design.

## 2. Capability-first, conversation-complete

Capabilities are the explicit boundary for effects in a host application, game, device or robot. GVYA models what a brain may request before it models how a host executes it.

Capability-first does **not** mean tool-call-only. A valid interaction may be purely conversational: greeting, clarification, acknowledgement, explanation, reference repair, follow-up, refusal or small talk require no capability invocation.

## 3. Machine-authorable source, deterministic authority

GVYA source and its canonical CLI are first-class machine interfaces. External agents may turn arbitrary inputs into source, edit it directly, generate tests, inspect diagnostics, simulate, and build artifacts without driving Studio. The agent host owns model/provider selection, credentials, tools, context, source mutation, retry execution, and review policy. GVYA does not embed a model/provider registry, credentials, prompt/session protocol, or autonomous model-hosting agent runtime. The canonical CLI may expose deterministic authoring-loop state and next actions derived from compiler/runtime gates, but those decisions never become a second source format or an authority independent of the underlying gate.

Authoring clients never write `.gvya` bytes directly; only the canonical compiler does. At runtime, deterministic GVYA logic remains first-class. Optional external structured semantic resolvers are untrusted proposal sources. They receive only an explicit bounded resolver-safe projection of candidate meanings, typed semantic declarations, safe context and exposed host references rather than arbitrary host state. They may propose only semantic meaning/slot/reference interpretation; **Capability identity is absent from the resolver contract**. They may not:

- widen the deterministic candidate-Meaning boundary;
- invent undeclared semantic values or unexposed host references;
- mutate already-authoritative collection values;
- select or execute capabilities;
- bypass deterministic semantic/type/entity/reference review;
- bypass capability schema validation or policy/admission;
- silently mutate authoritative state;
- override a required confirmation.

An unavailable or malformed optional resolver must degrade to the ordinary deterministic conversation path rather than making resolver availability a runtime dependency.

## 4. One canonical executable semantics

Executable semantic behavior is implemented once in the canonical Rust core. JS, browser, Node, native engines and game-engine integrations must call the same core through defined boundaries rather than independently reimplementing matching/conversation algorithms.

Language-specific SDKs may provide ergonomic adapters, not divergent semantics.

## 5. Host-owned effects

GVYA never directly opens a door, sends money, moves a character, writes a device register or invokes an application API.

GVYA produces an **Invocation Proposal** only after deterministic admission. The host owns actual execution and returns an explicit result/event when further conversation depends on the effect.

There are no implicit side effects hidden inside text generation or semantic resolution.

## 6. Typed capability contracts

Every invocable capability has a declared contract. Inputs are typed and validated. Outputs may also be declared and validated.

GVYA uses a constrained JSON Schema 2020-12 profile for capability input/output shape. GVYA-specific semantics such as host-reference kinds are explicit annotations/contracts, never guessed from prose.

A behavior that references an undeclared capability is invalid and must fail compilation.

## 7. Determinism is an observable product property

Given the same:

- compiled artifact bytes;
- runtime contract version;
- interaction input;
- context snapshot;
- prior GVYA state;
- explicit nondeterminism inputs such as time/seed when allowed;

the deterministic runtime must produce the same semantic result, response plan, state transition and trace.

The core must not secretly read wall-clock time, host environment, filesystem, network, random device state or other ambient process state. Such information is explicit input.

## 8. State ownership is explicit

GVYA state, host context and host state are different things.

- **GVYA State**: brain/session state owned by the GVYA runtime contract.
- **Context Snapshot**: immutable host-supplied facts available to a turn.
- **Host State**: application/game/device state, never directly mutated by GVYA.

GVYA may propose internal GVYA state transitions. Host changes happen only through admitted capability proposals executed by the host.

## 9. References are not strings with wishful meaning

A reference to a room, NPC, file, device, inventory item or other host entity has an explicit reference kind and stable host-facing identity. Text labels are presentation/search material, not authority.

An optional external semantic resolver may suggest a reference candidate; deterministic resolution/admission establishes whether the candidate is real and usable.

## 10. Ambiguity is represented, not concealed

If GVYA cannot establish a required meaning, slot or reference safely enough, ambiguity is a first-class result. The system may clarify, decline, fall back or remain conversational; it must not fabricate required arguments merely to produce a call.

## 11. Why/Trace is architectural, not a debug afterthought

Every consequential decision must be explainable through structured trace data suitable for a human-facing Why surface:

- input normalization and semantic views;
- candidate/evidence decisions;
- slot/entity/reference resolution;
- conversation context;
- response eligibility;
- capability binding;
- validation;
- policy/admission;
- confirmation requirements;
- resolver proposals and their acceptance/rejection;
- state transitions.

Trace must use stable reason codes and redact sensitive values by policy. Raw logs are not the product contract.

## 12. Human authoring UX protects cognitive ergonomics

The human Studio must preserve these design principles:

- progressive disclosure;
- existing configuration reveals itself;
- primary authoring content remains visible;
- compact package-level scanning;
- human summaries before raw schemas;
- low-overwhelm auditing;
- Why attached to concrete simulation outcomes;
- stable primary actions.

The visual design, routing and React component architecture are free to be new. AI workflows are free to be substantially redesigned, but must not overwhelm or replace the human editor.

## 13. `.gvya` has exactly one meaning

A `.gvya` file is a compiled portable runtime artifact. It is never a source project, editor package, legacy archive or alternate export mode.

The exact binary/container format is defined in compiler/artifact layer. domain-model layer reserves the meaning only.

## 14. Source and compiled artifact are separate worlds

Authoring source may contain rich comments, tests, AI metadata, generation provenance and editor-only material. The compiler decides what is required at runtime. Runtime artifacts do not become accidental source-of-truth documents.

## 15. Composition is explicit

Packages may contribute reusable vocabulary, semantic behaviors, responses, policies, capability contracts, tests and assets, but composition must be deterministic and inspectable. Ambient global overrides and order-dependent magic are forbidden.

Exact package composition semantics are defined in package/audit/test layer.

## 16. Security and resource boundedness are design constraints

Untrusted text, context, optional neural output and compiled artifacts are all input boundaries. Parsers, schema validation, resolver calls, trace growth, recursion, asset lookup and package composition must eventually have explicit limits.

Machine-facing parsers and commands enforce these limits, and foundational layers must not choose designs that require unbounded behavior.

## 17. Failure is typed

Malformed source, compile errors, ambiguous understanding, policy denial, missing references, unavailable capabilities, confirmation requirements, host execution failures and corrupt artifacts are different conditions. They must not collapse into a generic fallback string.

## 18. No silent feature loss during re-expression

When a task re-expresses existing behavior under a new contract, the result must be `PRESERVED` or `SUPERSET` relative to the behavior it replaces. Establish that from the live owning contract and its executable tests, which are the only authority present in this source package.

Clean break permits new contracts. It does not justify shallower behavior.
