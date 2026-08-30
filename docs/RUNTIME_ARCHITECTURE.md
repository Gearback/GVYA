# GVYA Runtime Architecture

## Authority model

```text
.gvya bytes
  -> strict container validation
  -> manifest/integrity/program cross-check
  -> compiled IR hydration
  -> one canonical Rust Semantic Kernel
  -> one canonical Rust Conversation Kernel
  -> one canonical Rust Capability Kernel
  -> RuntimeInteractionOutput
  -> thin wire/C/WASM/JS/Godot adapters
  -> host executes capability proposals
```

Only the Rust runtime owns semantic, conversation and capability behavior. Adapters transport bytes/JSON and expose assets; they do not score, select, mutate conversation state or admit capabilities.

## Runtime crates

### `gvya-artifact`

Shared pure container parser/encoder. Compiler and runtime use the same deterministic container contract rather than duplicating byte-layout parsing.

### `gvya-runtime`

Owns strict executable-program hydration, manifest/integrity/signature boundary, integrated turn/open execution, asset access and stable JSON wire serialization.

### `gvya-ffi`

Audited raw-pointer edge. It owns opaque handles and buffer ownership only. It calls `gvya-runtime` and contains no semantic algorithms.

## Strict load sequence

1. Parse `.gvya` container with hard bounds and canonical layout rules.
2. Require exact `manifest.json`, `program.json`, `integrity.json` entries.
3. Parse manifest/integrity with unknown-field rejection.
4. Recompute and cross-check program/integrity digests and sizes and derive the canonical content root.
5. Parse/validate the optional signature envelope and apply the host trust policy before expensive executable hydration where possible. Authenticity never substitutes for structural/digest checks.
6. Hydrate `gvya.program/1` under canonical executable `ProgramLimits` (bytes, depth, nodes, strings, collections/packages) and strict IR validation.
7. Validate project/brain identity across program and manifest.
8. Validate source-package provenance across manifest/integrity/program.
9. Validate optional debug-map declaration/presence.
10. Validate asset ID/path/digest/size sets across all layers.
11. Construct one runtime from hydrated kernels.

Signature verification can establish authenticity but can never bypass structural/digest validation.

## Derived semantic indexes

The matcher support tables are **derived data and are never serialized into `.gvya`**. The artifact carries only the canonical matcher inputs: `semantic.config`, `semantic.profiles` and `semantic.patterns`. Runtime load builds the support tables once through the same canonical `SemanticIndex::build(...)` implementation the compiler uses as a build-time validation gate, so the index cannot drift from the patterns it indexes — there is nothing to drift from. Semantics that cannot produce a valid bounded index (missing language profile, unsafe exact fanout) fail the load. No support index is rebuilt per turn.

`gvya.program/1` carries explicit Meaning structural `patterns` plus `semantic.profiles`, an exact map whose keys must cover `enabled_languages`. Each entry contains one language-isolated Language Profile merged only with its same-language Matcher Profile `pattern_sets`; it also carries that profile's authored Boolean values and deterministic custom entity canonical values/aliases. Cross-language lexical or structural profile data is never merged. Runtime hydration constructs the kernel-owned structural matcher once when the program is loaded: literal atoms, Matcher Profile sets, and custom-entity virtual sets are normalized/canonicalized and validated at that boundary, captures are checked against declared compatible slots, and malformed or conflicting set/entity data fails the load. Per turn, the request/default language selects one explicit semantic profile before normalization; structural matching is then ordered, whole-utterance and globally work-bounded; a structural winner is authoritative, while ambiguity, binding failure or budget exhaustion fails closed. The semantic sample scorer runs only when the structural layer returns no match. Structural rules and set aliases are never reparsed or renormalized per turn.

`SemanticKernel::new(...)` is the one kernel construction path. It returns a checked result and rejects invalid semantic configuration, missing language profiles and unsafe exact-fanout indexes. Compiler, authoring tools and production `.gvya` runtime load all go through it, so they enforce identical range/index invariants.

Conversation topic/follow-up filtering uses `SemanticKernel::analyze_allowed(...)` over the same full runtime-built index; it does not build subset indexes.

## Integrated execution

Every public typed Rust Runtime entry validates canonical `RuntimeLimits` before kernel work. JSON/C/WASM adapters add earlier byte/JSON-shape checks, but direct Rust hosts cannot bypass the Runtime-owned resource policy. External semantic resolver requests/proposals are independently bounded inside the semantic kernel, and resolver transport/decoder failure degrades to the ordinary deterministic conversation path.

`Runtime::turn(...)` executes:

1. Resolve explicit request language, active conversation language, and the compiled Bot default without consulting Project catalog order.
2. Semantic understanding through the hydrated conversation kernel.
3. Conversation lifecycle/response/state behavior.
4. Capability binding/admission/confirmation using the selected conversational result.
5. Combined human-first Why report.

When semantics confidently selects a Meaning but required typed values are absent, the decision is `Partial` rather than generic unresolved. Conversation stores exactly one bounded `active_collection` in `GvyaState`, renders the next declaration's authored/localized elicitation prompt, and emits no Meaning to the capability layer. The next turn is interpreted against the remaining declaration types before ordinary repeat/global handling. Invalid or ambiguous answers preserve collection; a strong independent deterministic Meaning supersedes it; a turn language with no renderable authored prompt clears collection and falls through to the ordinary Fallback path instead of answering with an empty response. Completion clears collection and re-enters the same behavior selection, capability binding, schema validation, policy, and confirmation path as a complete one-turn Meaning. There is no collection-specific capability path and no hidden session storage.

`Runtime::open(...)` executes opening behavior through the same conversation/capability path.

GVYA never invokes game/device/application side effects itself. The host receives `InvocationProposal`, executes it under host authority, then can pass a correlated `CapabilityResultInput` back to GVYA validation.

runtime/SDK layer also hardens result validation by revalidating proposal version, arguments and proposal fingerprint before accepting the host result.

## Assets

Runtime assets are authenticated artifact entries, not extracted files. Hosts can resolve by `AssetId` or logical artifact path and receive:

- media type;
- logical path;
- SHA-256 digest;
- borrowed/copyable bytes.

Adapters do not need to parse `program.json` to discover asset paths.

## Native / C / WASM / JS / Godot

### Native Rust

Use `gvya-runtime` directly.

### Stable C ABI

`gvya-ffi` exposes opaque `u64` runtime handles, explicit input byte spans and caller-freed output buffers. The raw-pointer crate is isolated; canonical crates keep `unsafe_code = forbid`.

### WebAssembly

The same C ABI surface is compiled to WebAssembly. ABI v1 opens artifacts with strict `gvya.runtime.open-options/1` JSON. The host can set artifact limits and choose `allow_unsigned`, `require_present`, or `require_verified`; verified trust requires an explicit host-owned preverification attestation bound to the exact content root and signature envelope. Artifact length is rejected against the selected total-byte limit before the FFI edge copies artifact bytes. The TS SDK validates ABI version, pointer width and buffer struct size, copies bounded request/artifact bytes into WASM memory, and decodes the same JSON wire responses.

### Browser and Node

`@gvya/runtime-sdk` is a transport wrapper. Browser and Node can use the same WASM backend. Native hosts can bind the C ABI directly. There is no JavaScript semantic fallback.

### Godot

`adapters/godot/GVYARuntime.gd` is the supported thin Godot host surface. Godot Web loads `adapters/godot/web/gvya-godot-web.js` through `JavaScriptBridge`; that bridge validates the wasm32 ABI/layout, preserves strict `gvya.runtime.open-options/1` and the canonical adapter-side byte ceilings, and delegates runtime, capability, asset and diagnostics calls to the exact Engine WASM. It contains no semantic/runtime fallback.

Native Godot may inject/register a `GVYANativeRuntime` GDExtension exposing the same JSON/byte methods, but this baseline ships the Web/WASM implementation rather than a native extension binary. Godot parity is executable release-gate work, not an architecture-only claim.

## Determinism and explicit authority

The runtime does not acquire wall clock, random seed, network, filesystem state or hidden environment values as semantic authority. Relevant context/state/seed/available capabilities/confirmation grants are explicit request fields.

## Parity claim boundary

Every target routes to the same Rust implementation and versioned wire contract. A cross-runtime parity claim additionally requires the executable native/WASM/SDK parity gates defined in `RELEASE.md`; architecture alone is not empirical proof.

## Confirmation round trip

`NeedsConfirmation` is resolved by retrying the exact canonical turn/open interaction with an exact `ConfirmationGrant`; proposal identity does not depend on the grant. SDK/Studio preserve the originating request for this retry, while current host context may be refreshed so changed availability/policy/arguments fail closed. Confirmation does not create a second execution authority path.

## Canonical runtime and SDK inventory

This inventory is normative for the current GVYA architecture; implementation and executable tests must agree with it.

| Concern | Canonical runtime / SDK rule |
|---|---|
| Browser/Node portability | same canonical Rust runtime exposed through WASM + TS transport |
| Worker-style isolation | one versioned JSON wire contract usable through WASM/worker/host transports |
| Native embedding | direct Rust API + stable C ABI |
| Godot integration | supported Godot Web JavaScriptBridge/GDScript adapter over canonical Engine WASM; optional native GDExtension injection uses the same wire surface |
| Matcher/index construction | derived index is never shipped; runtime builds it once at load from canonical patterns/profiles/config and fails closed when it cannot |
| Topic/follow-up scoped matching | same full runtime-built index + allowed-Meaning filtering; no subset rebuild |
| Runtime state/response behavior | one Rust conversation kernel for every target |
| Partial Meaning lifecycle | one serializable bounded `ConversationState.active_collection`; completion returns to the ordinary behavior/capability path |
| Elicitation | required slot/reference owns localized authored prompts; runtime contains no English fallback question |
| Custom deterministic entities | compiled Language Profile catalogs, exact canonical extraction, normalized collision rejection, no duplication of host-reference authority |
| Capability actions | typed versioned invocation proposals; host executes |
| Capability-result correlation | proposal version/arguments/fingerprint/output revalidated |
| Assets/attachments | authenticated entry bytes + logical metadata by ID/path, no extraction assumption |
| Artifact loading | one strict `.gvya` version/layout only |
| Signature trust | content-root signature hook after structural validation; host owns trust |
| Runtime diagnostics | full structured Meaning/state/capability/Why/Trace wire result |
| Cross-runtime parity | one Rust implementation plus the executable native/WASM/SDK parity gate defined in `RELEASE.md` |
| Non-finite numbers | wire/effect paths reject non-finite values fail-closed |
| Handle lifetime | collision-safe opaque runtime handle registry in C ABI |


## Studio Engine asset use

The WebAssembly runtime surface is part of the single versioned internal Studio Engine module. Studio does not ask authors to locate it. Simulation and Web export reuse/copy the exact prebuilt Engine bytes; authored Bot/Package changes never rebuild Engine WASM.
