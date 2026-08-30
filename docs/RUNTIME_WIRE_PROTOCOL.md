# GVYA Runtime Wire Protocol v1

The wire protocol is the stable serialized boundary used by C/WASM/JS-facing adapters. It is not GVYA source and it contains no target-specific semantic logic.

## Rules

- JSON UTF-8.
- Every request/result has an explicit `format` and `version`.
- Input structs reject unknown top-level/compiler-owned fields.
- JSON numbers must be finite and representable by the runtime model.
- Runtime serialization fails closed if typed internal state contains `NaN` or infinity; such values are never silently emitted as JSON `null`.
- Capability execution remains host-owned.

## Formats

| Purpose | Format | Version |
|---|---|---:|
| Turn request | `gvya.runtime.turn` | 1 |
| Turn result | `gvya.runtime.turn-result` | 1 |
| Opening request | `gvya.runtime.open` | 1 |
| Capability result request | `gvya.runtime.capability-result` | 1 |
| Capability result continuation | `gvya.runtime.capability-result-result` | 1 |
| Runtime info | `gvya.runtime.info` | 1 |
| Capability catalog | `gvya.runtime.capabilities` | 1 |
| Capability contract | `gvya.runtime.capability-info` | 1 |
| Asset metadata | `gvya.runtime.asset-info` | 1 |

## Turn request authority

A turn request carries the utterance text and explicit runtime inputs needed by the canonical kernels, including host context/visible references/available capabilities, GVYA state, resolver reference candidates/context, system values, hint request, deterministic seed and confirmation grants.

No adapter augments this with ambient clock/network/filesystem state.

The wire has no conversational language or language-policy input. Runtime evaluates each ordinary utterance with every artifact-enabled Language/Matcher Profile. The language attached to the winning localized semantic evidence selects the response and becomes `state.conversation.active_language`; a later match may switch it again. Unresolved or non-semantic interactions retain an enabled active language and otherwise use the artifact's Bot `default_language`. Runtime info exposes `enabled_languages` and `default_language`; the broader Studio/Project authoring catalog is not part of the artifact or wire contract, and array order is never an implicit preference contract.

## Turn result

The result serializes the canonical runtime observation, including:

- conversation mode;
- structured Meaning and typed slots/references;
- selected behavior/response messages/items;
- resulting state;
- semantic diagnostics and traces; every ranked semantic score row includes the stable `meaning` ID alongside the implementation diagnostic `pattern_index`;
- capability decisions and typed invocation proposals;
- combined Why report.

Hosts may choose what diagnostic visibility to expose to end users. The wire preserves the structured result so Studio/AI/CI can inspect it without scraping prose.

## Partial Meaning and collection state

A semantic decision may be `type: "partial"`. It carries the selected typed `meaning`, an authored-order `missing_required_values` array (`{"type":"slot","name":...}` or `{"type":"reference","kind":...}`), and the semantic `source`. It is distinct from `resolved`, `ambiguous`, and `unresolved`.

The single serializable collection authority is `state.conversation.active_collection`:

```json
{
  "meaning": { "id": "order.create", "slots": [], "references": [] },
  "remaining": [{ "type": "slot", "name": "count" }],
  "authority": "deterministic",
  "started_turn": 4
}
```

`meaning` preserves already validated typed slots/references. `remaining` is non-empty, ordered, bounded, and must agree with the compiled Meaning catalog when execution resumes. `authority` is exactly `structural_pattern`, `deterministic`, or `resolver_proposal`. Malformed, duplicate, stale, unknown, empty, or oversized state fails closed before it can reach capability admission.

## Capability result continuation

The host returns the exact proposal plus the execution result together with the latest GVYA state returned by the interaction that admitted it. That state contains a bounded runtime-owned `conversation.pending_capabilities` receipt ledger. GVYA requires an exact pending receipt, then checks proposal ID/version, argument schema/fingerprint and output schema/correlation before acceptance. The accepted receipt is consumed before continuation, so fabricated, modified, stale and replayed results do not re-enter authored handlers.

A valid result then enters the deterministic capability-result conversation path using explicit context/state/system/seed inputs. Language is derived from runtime-owned conversation state, falling back to the compiled Bot default when needed; hosts cannot override it on the result request. The result may select an authored result handler, render a continuation, update GVYA-owned author state, emit Why evidence and produce another ordinary capability proposal. Invalid results do not enter that continuation. Host effects are never executed by this path.

## Capability introspection

`gvya.runtime.capabilities` lists lightweight declared-capability summaries and `gvya.runtime.capability-info` returns one exact full contract, including schemas and host-effect metadata. These are read-only declarations; introspection never adds an entry to `available_capabilities` and therefore grants no execution authority.

## Asset access

Binary asset bytes are not encoded into JSON. C/WASM edge methods return raw bytes by logical path or `AssetId`; a separate JSON call returns asset metadata.

## Buffer ownership

C ABI outputs use `gvya_buffer { ptr, len, capacity }`. A non-empty output must be released with `gvya_buffer_free`. Scratch input allocations returned by `gvya_alloc(len)` must be released with `gvya_dealloc(ptr,len)` using the exact length.

## Versioning

All runtime messages remain v1 during pre-publication development. Contract changes update v1 in place as a clean break: adapters accept only v1 and do not contain compatibility readers, aliases, migrations, or dual shapes. The separate `gvya.runtime.open-options/1` FFI loading contract is likewise v1.


## Runtime wire budgets

Wire v1 is decoded through canonical `WireLimits` before typed request conversion. The runtime enforces a request-byte ceiling, JSON depth/node/string/collection ceilings, and explicit limits for visible references, available capabilities, resolver candidates, confirmations, language fallbacks, and active collection values. Runtime-managed state and author state have their own lifecycle budgets. The typed Rust `Runtime` validates the complete interaction result against its response-byte budget before returning, so direct API users cannot bypass the wire boundary. JSON output uses a capped writer that refuses growth beyond `max_response_bytes` instead of serializing an unbounded buffer and rejecting it afterward. Adapters must not bypass these limits by pre-parsing unbounded host JSON into alternate runtime structures.
