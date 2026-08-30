# GVYA Semantic Kernel Design

## Scope

The semantic kernel answers one bounded question: **what meaning is supported by this interaction, and which typed utterance/reference values can be resolved without guessing?**

It does not select conversational responses, mutate conversation state, admit capabilities, execute host behavior or encode `.gvya` artifacts.

## Pipeline

1. Normalize lexical text deterministically and extract typed entities using raw and normalized evidence.
2. Check eligible compiled structural patterns against the normalized ordered token stream. Structural rules are whole-utterance author authority, support bounded `*`/`^` wildcards, named String captures and Matcher Profile sets, and are compiled once when the kernel is built. A structural winner resolves before semantic scoring; structural ambiguity, binding failure, or work-budget exhaustion fails closed rather than silently entering fuzzy matching.
3. If and only if no structural rule matches, build the ordinary semantic views.
4. Retrieve bounded candidates from deterministic indexes. High-specificity exact, typo-corrected and canonical/glue-stripped evidence is considered before the short-query rejection gate.
5. Apply conservative typo-lite as an additional view, never by mutating the original utterance. Separately retrieve bounded generic fuzzy candidates from Unicode n-gram signatures so scorer-supported spelling and productive word variants remain reachable without a catalog scan.
6. Score only language-compatible candidates using explicit evidence shapes and negative/reporting guards. Positive samples, negative samples and retrieval terms all carry explicit language tags; per-Meaning counts and text bytes are compiler/runtime bounded.
7. Recover authored recall with bounded exact-sample, canonical specificity, typo-signature and explicit scoped-rescue paths. If bounded retrieval surfaces no decision-grade sample evidence, small/medium catalogs may run one bounded exhaustive sample-rescue pass through the same canonical scorer; only strong positive-sample evidence is admitted from that pass, while weak/retrieval-only rows are discarded. Posting saturation and rescue use are traceable, and dangerous exact fanout still fails closed rather than becoming hidden catalog-order authority.
8. Preserve top-candidate ambiguity rather than guessing. Conversation-layer response eligibility may rerank only after the remaining semantic frontier is re-checked for ambiguity.
9. Bind typed slots and host references deterministically. Structural wildcard captures bind normalized surface text; structural set captures bind the authored canonical set value. A confidently selected Meaning with valid bound values but unsatisfied required declarations becomes `Partial`, preserving those values and the missing declarations in authored order; it is not collapsed into `Unresolved`.
10. Emit structured Why/trace, including structural rule/captures when applicable or semantic candidate-retrieval reason and saturation data otherwise.
11. If unresolved/ambiguous and a resolver is configured, expose only bounded candidate meanings, explicitly selected safe context and visible reference candidates; validate its proposal before accepting a complete or partial semantic meaning. The resolver is never used to override a structural outcome.

## Authority boundaries

- `MeaningPattern` is executable semantic metadata, not an authoring/source format.
- Structural `patterns` are explicit author authority and are intentionally separate from fuzzy `samples`; semantic score cannot override a structural winner.
- Structural rules must contain a literal or Matcher Profile set anchor. Wildcard-only catch-alls are rejected because unresolved/repeat fallback is owned by the conversation/fallback layer.
- `SemanticProfile.pattern_sets` are explicit authored set aliases inside one language-scoped profile. They are compiled/validated once with the structural matcher; aliases are not normalized repeatedly per turn.
- `MeaningClass` is explicit; IDs and package names are never semantic classifiers.
- `retrieval_terms` are explicit metadata; IDs and topics are never secret retrieval features.
- Host reference labels and aliases are match hints. `HostReference.id` is authoritative.
- Language Profile `custom_entities` are bounded authored vocabularies. Their canonical string values and aliases are deterministic package data; they do not replace dynamic host-owned references.
- Resolver proposals cannot widen the exposed Meaning boundary, invent declarations/custom-entity values/host references, mutate already-bound collection values, or bypass required-value rules.
- Capability identity is not part of the semantic-resolver contract at all. Capability binding/admission is implemented only downstream from a validated Meaning.
- Ambient wall-clock, randomness and environment are not semantic inputs.
- Direct Rust construction validates the same canonical semantic configuration ranges as compiler source parsing and executable hydration.
- Compiler and runtime share the canonical semantic-index builder. The index is derived data: the compiler builds it only to prove the composed semantics yield a valid bounded index, and the runtime builds it once at load from the canonical patterns/profiles the artifact ships. It is never serialized.

## Determinism

For the same catalog, language-scoped semantic-profile map, utterance language/fallback chain, utterance, reference candidates and exposed context, the deterministic path produces the same normalized views, ranking, decision and trace ID. The kernel does not read system time, locale, filesystem, network or RNG. Candidate retrieval uses integer/fixed-point weights before sorting/truncation. Floating-point semantic scoring normally runs on that bounded frontier; the bounded exhaustive sample-rescue path may additionally score the complete permitted catalog only when its explicit pattern/evidence/work ceilings are satisfied and the ordinary frontier has no decision-grade sample evidence. Language lookup order is explicit requested exact tag → requested base tag → explicitly supplied fallback tags/bases. No natural language and no `und` profile is injected by ambient policy.

## Profile composition and extension

`SemanticProfile::default()` is deliberately language-neutral. Executable programs carry a `SemanticProfiles` map keyed by normalized enabled language, and each authored/indexed row is canonicalized only with its own language profile. A turn selects one profile from its explicit language/fallback chain; lexical policy from another language never leaks across that boundary. The kernel contains no built-in language or transliteration profile or selector. Colloquial/canonical/suffix rewrites, script rewrite/removal rules, glue/weight sets, reporting markers, pronouns, negations, social tokens, task cues, continuation cues and lexical entity vocabularies come from standalone `gvya.source.language-profile` JSON. Structural `pattern_sets` come only from paired `gvya.source.matcher-profile` JSON selected for the Brain's active languages.

Language policy is not Package content. Shared Language/Matcher pairs are reusable templates and each Project owns portable copies in its `language-profiles/` and `matcher-profiles/` folders. Studio materializes only pairs corresponding to a Bot's enabled languages. The Persian Language Profile contains Persian/Arabic-script normalization, including Persian/Arabic digit folding to ASCII and `٪` to `%`, plus plural morphology, common formal/colloquial inflections and vocabulary; Pinglish/transliteration is neither built in nor shipped as an ambient fallback. Profiles have no human visual editor and remain directly readable/editable JSON for machine authors.

The canonical deterministic candidate budget is `2..=256`. A lower bound of two is part of the ambiguity contract: retrieval may not hide the runner-up that the decision layer needs to distinguish resolution from ambiguity. Additional lexical profiles and authored/custom entity definitions may extend the profile without changing ranking authority.

## Partial Meaning and collection handoff

Semantic decisions have four explicit outcomes: `Resolved`, `Partial`, `Ambiguous`, and `Unresolved`. `PartialMeaning` contains the selected typed `Meaning`—including every already validated slot/reference—and a bounded ordered list of `MissingRequiredValue::{Slot, Reference}`. The resolution source remains explicit (`structural_pattern`, `deterministic`, or validated `resolver_proposal`). Matcher internals and full utterance history are not persisted.

The Conversation Kernel is the sole owner of `ActiveCollection`. It stores the partial Meaning, remaining declarations, semantic authority, and starting turn in ordinary serializable `GvyaState`. Continuation interpretation reuses this semantic kernel's normal typed entity extraction, custom entity catalogs, Boolean vocabulary, and visible host-reference resolution while constraining candidates to the declared remaining targets. Invalid or ambiguous continuations do not mutate the state. A strong independent deterministic Meaning may supersede collection. When the remaining list becomes empty, Conversation feeds the completed Meaning into the same behavior and capability path used by a one-turn request; semantic code never invokes capabilities.

## Canonical semantic behavior inventory

This inventory is normative for the current GVYA architecture; implementation and executable tests must agree with it.

| Behavior | Canonical rule / owner |
|---|---|
| Unicode NFC + lowercase/whitespace normalization | pinned Unicode 15.1 tables in `semantic/unicode_nfc.rs` + `semantic/normalization.rs` |
| Script/transliteration/diacritic rewrites | explicit `SemanticProfile.normalization_rewrites` / `normalization_remove_chars`; no built-in language policy |
| Colloquial token rewriting | `SemanticProfile::normalize_colloquial_tokens`; authored longest-phrase match followed by detached-suffix removal |
| Authored token canonicalization | `SemanticProfile::canonical_token`; exact aliases, exact suffix-exception guard, then longest authored suffix rewrite, with no built-in morphology table |
| Glue/content token distinction | `SemanticProfile::content_tokens` |
| Multiple normalized/colloquial/entity/clean/content views | `build_semantic_views`; typed `SemanticView` names |
| Raw + normalized entity extraction | `entities.rs`; structural-token tests |
| number/date/time/color/quantity/unit/email/phone/url/origin entities | `entities.rs`; typed values and statuses |
| relative-date lexical entities | profile-authored token → symbolic value; no ambient clock |
| calendar date validation | invalid dates rejected, leap-year logic |
| explicit structural whole-utterance rules | `semantic/structural.rs`; run before semantic retrieval/scoring and cannot be score-overridden |
| structural wildcards and captures | `*` = 1+, `^` = 0+; named wildcard captures bind normalized surface String values |
| structural Matcher Profile sets | `<set:name>` / `<set:name>{slot}`; aliases compile once, capture authored canonical values, conflicts fail closed |
| structural precedence/ambiguity | deterministic specificity + rule/Meaning priority; cross-Meaning ties and capture-partition ties return ambiguity |
| structural work bound | global per-analysis work budget; exhaustion returns unresolved `structural_match_budget_exceeded` without semantic fallback |
| exact / phrase-start / phrase-span / short phrase-end | `matching.rs` |
| content-token coverage | `content_token_coverage` |
| relaxed ordered-subsequence match | `relaxed_ordered_subsequence_match` |
| weighted token/edit/stem similarity | `token_similarity`, `weighted_token_f1` |
| numeric-window similarity | `numeric_window_similarity` |
| authored sample-length priority | exact contiguous authored samples remain bounded candidates when embedded; non-exact lexical strength is class-agnostic and monotonically favors shorter authored samples (for otherwise comparable evidence) |
| embedded exact sample retrieval | bounded exact-sample n-gram retrieval (1–8 tokens) preserves authored samples across the candidate frontier; `MeaningClass` does not create matcher strength, while Language Profile singleton ambiguity/direct-cue data remains a lexical safety signal |
| social or clarification phrase embedded beside a supported task | explicit `MeaningClass`; a non-exact conversational wrapper yields only when an independently scored `General` Meaning reaches the normal resolution floor |
| reported-speech suppression | profile-authored reporting verbs/nouns + span-aware pre-match reporting evidence + explicit rejection reason |
| exact/strong multi-content negative hard block | `scoring.rs` tests |
| fuzzy single-token negative not hard-veto | dedicated test |
| soft negative penalty | `negative_evidence` |
| positive-assumption negation penalty | `score_pattern`; negation markers come only from selected matcher-profile data |
| sample-weight cap and exact/phrase-start bonus | `scoring.rs` constants/branches |
| evidence tiers / strength | `classify_evidence_tier`, `evidence_strength` |
| authored priority only near ties | explicit deterministic comparator contract |
| Deterministic retrieval ranking | integer/fixed-point posting weights in `SemanticIndex`; no platform-dependent transcendental floating-point function participates in candidate ordering |
| exact-sample/content/meta/start indexes | exact/typo/canonical specificity is evaluated before short-query rejection; the support tables are derived once at load from the canonical patterns/profiles and are never serialized into `.gvya` |
| bounded candidate pruning | canonical 2..256 ordinary candidate budget + posting-visit/feature budgets + per-Meaning sample/negative/retrieval count/text budgets; saturation is traced and dangerous high-specificity fanout fails closed; the only O(all Meanings) path is the explicit strong-sample rescue below its fixed catalog/evidence/work ceilings |
| direct Rust semantic configuration validation | `SemanticKernel::new` validates the same canonical ranges as source compilation and executable hydration; no fail-open direct-host config path |
| exhaustive strong-sample rescue | only when the ordinary frontier has no decision-grade sample evidence; permitted catalog ≤1024 Meanings, positive+negative evidence ≤16384 items and view×evidence+pattern work ≤65536; uses `score_pattern` unchanged and appends only unblocked strong sample evidence |
| typo-lite one-edit unique corrections, max two | one-edit signature lookup keeps per-turn work bounded without disabling an entire large length bucket |
| whole-phrase typo rescue | when ordinary specificity has no candidate, bounded character-bigram retrieval may expose 2–8-token phrases to the scorer; the scorer requires equal token count, at most two corrected tokens, conservative Unicode Damerau similarity, and for equal-length three-character tokens only an adjacent transposition is accepted |
| generic spelling/productive-word recall | bounded Unicode n-gram signature retrieval makes existing edit/stem scorers reachable; it never rewrites input, assumes a language, or scans the catalog |
| hidden intent/topic-name domain rescue | replaced by explicit `retrieval_terms` |
| hidden pack/id casual/clarification classification | replaced by explicit `MeaningClass` |
| typed slot binding from extracted entities | `SlotKind`, deterministic binding and ambiguity |
| partial Meaning | selected Meaning plus preserved typed values and authored-order missing required declarations; distinct from ambiguity/unresolved |
| multi-turn collection | one bounded `ConversationState.active_collection`; semantic continuation supplies typed values, Conversation owns lifecycle/prompting |
| authored Boolean vocabulary | Language Profile `boolean_values`; exact normalized phrase authority during constrained collection |
| authored custom entities | Language Profile `custom_entities`; canonical value + aliases, normalized collision rejection, semantic extraction and virtual structural `<set:entity.kind>` use |
| typed host-reference resolution from labels/aliases | aliases are hints; ID remains authority |
| multiple slot/reference candidates | first-class `Ambiguous`, never silent guessing |
| deterministic semantic Why/trace | structured `TraceEvent` sequence and stable FNV trace ID |
| optional neural/LLM semantic assistance | `SemanticResolver` sees bounded candidates/safe context and returns untrusted proposal |
| neural proposal inventing capability | capability is ignored at semantic layer; capability authority remains later deterministic layer |
| neural proposal inventing meaning/reference/slot | rejected against catalog/candidate/reference/type boundaries |
| topic/follow-up/pronoun contextual boosts | conversation kernel owns explicit topic/follow-up state; continuation consults the selected language-scoped SemanticProfile task-cue vocabulary and generic follow-up phrases continue only after standalone semantic evidence remains weak |
| direct-address continuity | conversation kernel conversation tests preserve social direct address while embedded task language remains protected |
| answer eligibility/response shortcuts | conversation kernel Conversation Kernel retries bounded ranked semantic candidates when a higher-ranked behavior has no eligible response, but re-checks the remaining ambiguity frontier before reranking; genuine semantic ambiguity still wins |
| memory/repeat/repair state | conversation kernel conversation kernel |
| capability availability/policy/confirmation/execution | capability kernel capability kernel |
| JS runtime branch using undefined local in relaxed-match path | single Rust implementation removes port drift |
| Godot normalization/pruning drift | single Rust implementation is canonical |

## Optional external structured semantic resolver boundary

The deterministic/offline runtime remains the default. `Runtime::turn` is unchanged. Optional assistance is opt-in through `Runtime::turn_with_resolver` and a `SemanticResolver` implementation. The resolver is an **untrusted semantic interpreter**, not a tool executor and not a source of capability authority.

### Resolver-safe request projection

`ResolverRequest` has two explicit tasks: `ResolveMeaning` and `FillCollection`. It contains a bounded, language-aware projection only:

- current utterance and explicit semantic language fallback order;
- the complete bounded candidate-Meaning boundary;
- per-candidate origin (`deterministic_match`, broader `resolver_recall`, or `active_collection`);
- coarse stable deterministic evidence bands and bounded matched terms, never private index internals;
- up to six bounded positive semantic hints per candidate, drawn only from language-compatible samples, retrieval terms and structural patterns;
- the candidate's typed semantic slot/reference declarations, including required flags and bounded canonical custom-entity values where exhaustive;
- the complete bounded set of concrete host reference IDs explicitly exposed for the turn;
- explicitly exposed resolver context;
- for collection turns, the already-authoritative Meaning, read-only already-bound values, and the exact remaining collectable declarations.

The projection does **not** expose responses, negative samples, tests/scenarios, provenance/debug data, full package/source content, arbitrary GVYA state, the host Capability catalog, or host execution hooks. The candidate list is exhaustive resolver authority; there is no catalog fallback or fuzzy Meaning-ID recovery after a proposal is returned.

Canonical projection ceilings include 64 candidate Meanings, 6 hints per candidate, 160 bytes per hint, 32 slots and 16 reference declarations per candidate, 32 canonical entity values per entity slot, 64 concrete host reference candidates, 64 resolver-context entries, 64 collection targets/read-only bound values, and 8 matched terms per candidate. The JSON bridge additionally enforces 256 KiB request/response ceilings and strict bounded scalar/collection parsing.

### Provider-neutral JSON bridge

`JsonSemanticResolver` adapts a host-owned `String -> Result<String, String>` callback. The host owns model choice, local/remote transport, timeout/cancellation and availability policy; the runtime crate adds no model or network dependency. The serialized request is `gvya.semantic.resolver.request/1`. A future model-specific adapter may translate the typed candidate descriptors into that model's constrained schema/tool representation, but such adapter logic is outside the kernel contract.

### Semantic firewall

`ResolverProposal` can express only a Meaning, semantic slots, semantic host references, bounded confidence/evidence. It has **no Capability field**. The deterministic semantic kernel independently re-validates every proposal:

- selected Meaning must be inside the exact request candidate set;
- only declarations owned by that Meaning may be proposed, without duplicates;
- slot values must pass the canonical GVYA type/canonicalization rules;
- custom-entity values must be canonical values authorized by the active profile;
- host reference IDs must be among the explicitly exposed IDs of the declared kind;
- required-value completeness/Partial semantics are the same as deterministic binding;
- collection proposals may touch only the exact remaining collectable declarations and may not mutate already-bound values or switch Meaning;
- confidence must be finite/in-range and above configured acceptance threshold, but never bypasses any authority rule.

Accepted proposals are recorded as `resolver_proposal` semantic authority and then rejoin the ordinary Conversation/Capability path. Authored capability binding, capability input schema, policy/confirmation and admission remain the only route to an `InvocationProposal`.

A syntactically valid but wrong/out-of-bound proposal is rejected and leaves the deterministic semantic outcome untouched. If the optional resolver itself is unavailable, throws, times out at the host callback, or returns malformed/undecodable JSON, the canonical Conversation/Runtime path re-runs the same turn without resolver assistance, preserves the deterministic/fallback outcome, and records the stable non-sensitive trace `semantic.resolver.unavailable`. Resolver failure therefore cannot reduce deterministic runtime availability.

## Invocation rule

The resolver is not called when deterministic semantics already resolves or yields a valid Partial Meaning. A collection continuation may consult it only after deterministic typed collection cannot progress, and its authority is narrowed to the active collection. Conversation follow-up/topic scopes use the same allowed-Meaning boundary before resolver review. Explicit conversation continuation and global repeat logic remain deterministic conversation behavior.
