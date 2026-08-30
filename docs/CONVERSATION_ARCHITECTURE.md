# GVYA Conversation & Response Kernel Design

## Scope

conversation kernel turns semantic kernel semantic understanding into a deterministic, renderer-independent conversation lifecycle. It does **not** execute or authorize host capabilities.

The central rule is that conversation is first-class. An input such as `hi`, `why?`, `again`, `the other one`, or an unresolved turn is not required to map to a tool call to be legitimate.

## State model

`GvyaState` deliberately contains two authorities:

- `author`: mutable, author-addressable scalar/tree values for conversational memory. Optional `author_numbers` declarations give selected numeric paths explicit default/min/max bounds; initialization and every assign/increment clamp deterministically to those bounds. Parent/child declaration overlap is rejected.
- `conversation`: runtime-owned lifecycle state such as topic/follow-up/repeat/fallback-stage/focus/hints/history.

Host `ContextSnapshot` is read-only in conversation kernel. `system` values are explicit request inputs. There is no ambient clock, process environment, or hidden RNG.

## Turn pipeline

`ConversationKernel::respond` owns ordering:

1. normalize user input and derive deterministic bounded helpers such as basic math;
2. tick active topic at turn start;
3. snapshot follow-up state;
4. observe configured user style;
5. continue an active value collection before ordinary repeat/global handling when `conversation.active_collection` is set;
6. intercept global identical-message repeat when threshold is reached;
7. try active follow-up semantic scope;
8. compare global and active-topic semantic results;
9. begin value collection when the selected semantic decision is a Partial Meaning;
10. only when that result has no strong standalone evidence, try explicit short referential continuation from prior meaning/focus;
11. finalize missed follow-up TTL;
12. answer a resolved normal Behavior; otherwise try explicit repair continuation: a safe prior repair candidate for generic follow-up, then a current near-match candidate above the separate repair floor when its normal Behavior explicitly opts in;
13. if no repair-eligible Behavior can answer, evaluate the selected Fallback Package for the current fallback trigger;
14. select eligible locale/hint/repeat-stage response;
15. apply author effects before template rendering;
16. apply response follow-up directives and topic/focus/history state;
17. render one or more renderer-neutral messages under the six-message cap;
18. commit repeat/fallback-stage/recent-user state and advance turn index.

Each branch emits author-visible trace events rather than hiding why a route won.

## Semantic resolution vs fallback resolution

semantic kernel remains canonical for semantic evidence. conversation kernel never injects fake confidence scores.

Three explicitly conversational paths are distinct from normal semantic authority:

- `Continuation`: a tightly bounded referential utterance may reuse prior behavior/meaning focus only after the current utterance has failed to provide strong standalone semantic evidence. A short phrase such as `got it` therefore cannot replay the prior answer when it owns a strong Meaning itself.
- `RepairContinuation`: an unresolved turn may use a below-threshold semantic candidate only when the score meets the separate `repair_candidate_min_score`, the candidate can bind its required slots/references, and its normal Behavior explicitly sets `repair_continuation_candidate`. Ordinary Behavior eligibility and response eligibility still apply. Generic follow-up may reuse `repair.last_candidate` only when the prior Meaning has no required slots that would need reconstruction. Normal matcher thresholds are never lowered for this path.
- `Collection`: when semantics selects a Meaning but a required typed declaration has no value, the decision is a Partial Meaning rather than generic unresolved. Conversation stores exactly one bounded `ConversationState.active_collection`, renders the next declaration's authored localized elicitation prompt, and emits no Meaning to the capability layer. This is not a form framework: there is no reprompt counter, validation DSL, or cardinality system. The next turn is interpreted against the remaining declaration types only; invalid or ambiguous continuations preserve already-bound values and re-ask, while a strong independent deterministic Meaning supersedes collection. Completion clears collection and re-enters the same Behavior selection, capability binding, policy, and confirmation path used by a complete one-turn Meaning, so collected and directly supplied Meanings reach identical capability arguments. Because a required declaration may carry authored prompts for only a subset of the enabled languages, a turn whose language has no renderable prompt fails closed: collection is cleared and the ordinary Fallback path answers instead of holding a mute collection.
- `Fallback`: when neither normal resolution nor explicit repair continuation produces an eligible normal Behavior, the kernel evaluates first-class `FallbackBehavior` entries from the Brain's selected Fallback Package. A Fallback Behavior has no Meaning and cannot participate in semantic matching.

Fallback selection is deterministic: trigger (`unresolved` or `repeat`) → condition eligibility → highest priority tier → normal response eligibility/locale/recent-avoidance selection. If no authored fallback is eligible, the language-neutral kernel returns a Silent fallback outcome. Clarification/rephrase/unknown prose is ordinary authored Fallback Package data; there is no hidden engine-owned natural-language bank.

The returned semantic analysis remains unresolved when fallback wins. Trace/mode reports fallback explicitly, so an AI resolver or renderer cannot disguise conversational recovery as semantic certainty.

## Response model

A `ResponsePlan` contains ordered `ResponseMessage`s. Each message contains renderer-independent items:

- text + language,
- asset reference + optional alt text,
- HTTP/HTTPS link.

Extra messages remain separate messages rather than concatenated strings. This preserves timing/rendering flexibility for games, devices, chat UIs and accessibility surfaces.

## Template authority

The template DSL is deterministic and bounded:

- allowlisted roots: `author`, `context`, `meaning`, `conversation`, `system`;
- assignment may target only `author.*`;
- conditions, comparisons, arithmetic, fallback interpolation and deterministic `rnd`/`pick` are supported;
- at most eight expansion passes;
- division by zero is defined;
- system facts are request-provided, never read from ambient time/environment.

Response effects execute before the response template so the response can immediately describe its own authored state transition.

## Locale policy

In Studio, the Project's standalone Matcher Profile JSON documents define its available-language catalog. Each Bot compile target declares a non-empty `enabled_languages` subset and one required `default_language` inside that subset. Profile-document order and disabled authored languages have no runtime authority. If a required Bot or resolved-Package language has no Project Matcher Profile, Studio blocks compilation by keeping the Bot in an Overview-only Disabled state; it never silently substitutes another language.

Every ordinary turn is evaluated independently with every enabled Language/Matcher Profile. The canonical semantic comparison chooses one joint language-and-Meaning result from the same localized samples and structural patterns that prove the match. Structural evidence retains its normal authority; otherwise evidence tier, score and deterministic scorer ordering decide the winner. A close cross-language tie between different Meanings remains ambiguous. If equivalent evidence resolves the same Meaning in more than one language, the enabled `active_language` breaks the exact tie, followed by deterministic language order.

The winning evidence language selects the localized response and becomes `state.conversation.active_language` only when the turn produces a resolved or partial semantic interaction with a renderable response. This permits a single conversation to switch languages on any later matched turn. Unresolved, ambiguous, repeat, opening and capability-result paths do not infer a new language: they retain the enabled active language, or use the compiled Bot `default_language` for a fresh session. A neutral input such as `--#--` therefore cannot switch languages by itself.

There is no conversational language or language-policy input on the Runtime wire and no host/user override in Studio Simulate. Authored `und` content participates only when `und` is explicitly enabled for the Bot. Selection never picks a disabled Package language or treats Project/profile array order as preference. Language-sensitive evidence remains explicit package/profile data; the kernel ships no built-in language, transliteration profile, script detector, or ambient `und` fallback.

## Deferred authority

conversation kernel intentionally does not contain:

- capability contracts or argument binding,
- policy/admission/confirmation,
- invocation proposals,
- host/world/player mutation,
- execution success/failure semantics,
- package composition,
- persistent session/storage IO,
- model/provider-specific resolver adapters inside the Conversation Kernel. The kernel consumes only the provider-neutral `SemanticResolver` contract; concrete adapter/model hosting remains outside it.

Those belong to other explicit architecture layers. Keeping them out is an architectural boundary, not missing functionality.

## Canonical conversation behavior inventory

This inventory is normative for the current GVYA architecture; implementation and executable tests must agree with it.

| Behavior / invariant | Canonical expression | Notes |
|---|---|---|
| Author state separate from runtime conversation state | `GvyaState { author, conversation }` | Removes legacy mixed/untyped state coupling. |
| Active topic + TTL | `ActiveTopic`, start-of-turn tick | TTL is explicit and deterministic. |
| Mentioned/last topic survives active expiry | `mentioned_topics`, `last_topic` | Available to templates/conditions after expiry. |
| Active follow-up + TTL | `ActiveFollowup`, `FollowupTurnSnapshot` | TTL=1 is eligible on current turn and expires only after miss. |
| Follow-up consumed only on scoped acceptance | `consume_followup` | Global repeat does not consume it. |
| Same follow-up ID does not implicitly refresh | `refresh_if_same` | Refresh must be authored explicitly. |
| Follow-up expiring this turn cannot silently reopen itself | `expired_id` guard | Fail-closed lifecycle. |
| Short referential continuation | explicit `ConversationMode::Continuation` | No fake semantic score is manufactured. |
| Partial Meaning value collection | one bounded `ConversationState.active_collection` + authored localized `elicitation` | Conversation owns the lifecycle; semantics owns constrained continuation interpretation. Completion uses the ordinary Behavior/capability path, never a collection-specific one. |
| Unelicitable collection language | cleared collection + ordinary Fallback | A required declaration needs at least one authored prompt, not one per enabled language, so a language miss must fail closed rather than answer with nothing. |
| Active-topic contextual preference | separate semantic pass + explicit margin | Topic stickiness is inspectable. |
| Global identical-message repeat detection | bounded recent user history + threshold | Current turn participates in count. |
| Per-meaning/per-input repeat stages | `RepeatMemory`, `RepeatStage` | repeat → annoyed → final preference. |
| Per-Behavior repeat thresholds | `repeat_same_input_after`, `repeat_same_meaning_after` | Optional 2..20 thresholds override the global repeat threshold for that Behavior without changing matcher confidence. |
| Explicit repair-eligible normal Behavior | `repair_continuation_candidate` | Source/runtime-only opt-in; near-match or prior-candidate repair still obeys normal eligibility and required-slot safety. |
| Separate near-match repair floor | `repair_candidate_min_score` | Does not lower `resolution_threshold`; below-floor candidates remain fallback-only. |
| Bounded numeric author state | `author_numbers { path, default, min, max }` | Initializes declared numbers deterministically and clamps authored assign/increment effects. |
| Authored fallback | `FallbackBehavior` in the selected Fallback Package | No Meaning/matching; trigger + conditions + priority choose the eligible tier. |
| State-aware fallback personality | ordinary typed `ValueCondition` | The same author/conversation/context/system state can select angry, friendly, contextual, or other authored fallback behavior. |
| Authored repeat fallback | `FallbackBehavior { trigger: Repeat }` | Uses the same first-class fallback model instead of a separate pool. |
| No authored fallback | Silent engine outcome | The language-neutral kernel emits no hidden prose; user-facing recovery text must be authored in a Fallback Package. |
| Behavior follow-up scope | `followup_scope` | Behavior is eligible only while the named follow-up is active. |
| Follow-up reachability | opener/consumer graph | A scoped consumer is useful only if an installed reachable response can open the same scope; orphan consumers/openers are authoring findings and should be proven with an opener -> consumer Scenario. |
| Behavior identity by semantic scope | one default Behavior per Meaning plus one per distinct `(Meaning, followup_scope)` | A shared Meaning such as formal yes/no may have its ordinary response and separate handlers for explicit follow-ups; duplicate Behaviors in the same scope fail catalog construction. |
| Behavior required values | `requires_values` | Every exact typed value must match before the Behavior is eligible. |
| Behavior forbidden values | `forbidden_values` | Any exact matching forbidden value makes the Behavior ineligible; missing values do not block. |
| Openings | `OpeningDefinition` + `open()` | Typed topic/effects/follow-up support; no semantic fabrication. |
| Response conditions | typed `ValueCondition` over namespaces | Select between responses after Behavior eligibility; they do not replace Behavior-level required/forbidden values. |
| Response-side author memory effects | `ConversationEffect` | Only `author.*` is mutable in conversation kernel. |
| Effect before template rendering | `apply_and_render_selected` ordering | Same-turn effect values are visible in template. |
| Topic activate/refresh | `set_active_topic`, `refresh_or_activate_topic` | Mention history and `last_topic` updated. |
| Hint ladders | `HintRequest`, hint progress | Exact/higher/lower authored level selection retained. |
| Locale-aware response variants | arbitrary normalized locales + explicit base locale selection | No language-specific fallback pool exists in authored source. |
| Avoid recent response/variant when alternatives exist | bounded recent IDs/variant keys | Deterministic selection. |
| Safe template expressions | deterministic template parser/evaluator | No `eval`; explicit roots; no ambient host state. |
| Template if/elseif/else | template renderer |  |
| Arithmetic/logical/comparison expressions | template evaluator | division by zero → 0 as floor. |
| `rnd()` / `pick()` | explicit deterministic seed | Removes ambient RNG. |
| Template assignment | allowlisted `author.*` only | Host/context mutation prohibited. |
| Max template expansion passes | 8 | Bounded. |
| Simple two-operand math helper | `basic_math_result` | Explicitly projected into system context. |
| Extra messages | distinct `ResponseMessage`s | Deterministic chance + max six total messages. |
| Assets | typed `ResponseItem::Asset` with `AssetId` | Renderer independent. |
| HTTP/HTTPS links | validated `ResponseLink` | Label ≤120 chars; per-message URL dedupe case-insensitive. |
| User formality/style observation | configurable `StyleLexicon` + `UserStyle` | No language hardcode in kernel. |
| Why/turn trace | first-class `Trace` events | Lifecycle/mode decisions are inspectable. |
| Extracted semantic entities persisted for debugging/training | semantic analysis/trace owns them | No hidden duplicate session arrays. |
| Legacy tiny state | typed state + Meaning/focus | No redundant untyped mirror. |
| Host/world/player mutation | typed capability/state authority | conversation kernel host context is read-only. |
| Actions/capability execution | invocation proposal/admission | Runtime never directly executes host capability. |
| Harm/safety/policy admission | deterministic policy/admission | Not copied as ad-hoc answer heuristics. |
| Package composition / reusable presets | package model |  |
| Auditor / human Why UI / scenario model | audit/test model | Kernel trace foundation exists now. |
| Persistence/session storage | runtime/SDK responsibility | Kernel is storage-neutral. |
| AI training queues / authoring AI | Studio AI workflow | No hidden runtime authority. |
