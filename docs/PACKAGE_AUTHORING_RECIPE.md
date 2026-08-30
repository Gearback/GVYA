# GVYA Package Authoring Recipe

This is the canonical content-design recipe for humans and external agents authoring GVYA Standard, Bot, or Fallback Packages. It complements `SOURCE_FORMAT.md`; it does not define another source format. The output remains ordinary GVYA source. Authoring starts from the conversation that must work, then maps that conversation to GVYA mechanics and only then tunes matcher evidence.

## The default path: one useful slice in five decisions

Use this path unless a concrete conversation outcome proves that you need something more advanced:

1. **Outcome** — write one replayable user goal and, if useful, one next turn the Bot should invite.
2. **Meaning** — author one stable Meaning with the smallest discriminative evidence set that expresses that goal.
3. **Boundary** — keep one close confounder out of the positive samples and state which Meaning, if any, should own it.
4. **Behavior** — answer the goal in the first useful sentence, then offer at most one cheap, already-executable next move.
5. **Proof** — add one manual Regression Case for a standalone turn or one manual Conversation Scenario for the two- or three-turn route.

Then stop. Try a few fresh phrasings that are not samples, repair only an observed miss or false match, and promote the slice when the canonical gate accepts it. Do not pre-fill a large sample matrix, add state, create a Capability, or grow a conversation tree merely because those surfaces exist.

```text
user goal
  -> Meaning + small evidence set
  -> closest held-out confounder
  -> Behavior: direct answer + one truthful next move
  -> Regression Case or 2–3 turn Scenario
```

The numbered sections below are a diagnostic reference for choosing and proving the owning mechanic; they are not thirteen mandatory stages for every Meaning. The GVYA Help Bot is the executable example: `gvya.help.authoring-recipe` explains this path, its authoring scenarios connect it to matcher troubleshooting and Why-trace inspection, and its discovery scenarios demonstrate a reusable curiosity route without inventing a new Meaning per answer.

## Product thesis and measurable low-data claim

GVYA is a deterministic conversation compiler for narrow, trustworthy Bots. It turns explicit
knowledge and reusable conversation mechanics into a portable brain that can answer, route, recover,
and stay lively without inventing knowledge. It is not a generative knowledge source. Its multiplier
is **conversation density**, not pretend intelligence: the same small supported knowledge surface can
be reached through direct questions, credible repair, bounded recovery, connected next moves, and
opt-in curiosity without copying the answer into a new tree for every route.

“Low data” and “does not feel stupid” are measured claims, not fixed sample-count slogans. Freeze a
held-out corpus before tuning and record the first accepted checkpoint with these quantities kept
separate:

- **Bot-owned authoring cost** — counts of domain Meanings, Behaviors, positive samples, negative or
  close-boundary samples, response messages/variants, manual Regression Cases, manual Scenarios, and
  total Scenario turns;
- **reused authoring dependencies** — selected Standard/Fallback Packages and Matcher Profiles,
  reported separately because they are real dependencies but are not repeated per-Bot knowledge
  entry;
- **correctness** — supported-goal recall, close/off-domain precision, ambiguity findings, broken
  promises, and confident unsupported answers;
- **conversation quality** — direct-answer quality, productive recovery, repeat quality, clean exit
  from stateful routes, and voluntary continuation through an explicit exploratory route.

A turn is productive only when it does one of four jobs: answers a supported goal, accepts a credible
near-match through an explicitly proven repair path, narrows an unknown turn toward an executable
supported route, or states a useful limit and returns control. Filler, an unchanged “rephrase” loop,
a broken continuation, a confident unsupported answer, or an unsolicited detour that ignores a
recognized question is not productive engagement.

Measure **conversation density** as the number of distinct manually proven productive transition
edges divided by the number of domain-knowledge Meanings. An edge is a tested movement such as
unknown -> recovery route, answer -> related goal, curiosity -> ordinary goal, or repeat -> changed
strategy. Do not count response variants, synonym samples, repeated traversal of the same edge, or
fallback self-loops as new density. Report the raw numerator and denominator; do not optimize a
single aggregate score that can hide a precision failure.

There is no universal minimum number of samples. A Bot earns the low-data claim when its frozen
supported, near-miss, confounder, off-domain, recovery, and repeat probes pass with a small recorded
cost vector and no known correctness gate hidden by the aggregate. If the cost grows, say so; reuse
and AI-assisted entry reduce labor, but they do not erase authored semantics.

### Disposable experiment protocol

When the recipe itself needs improvement, do not repeatedly tune only the reference Bot:

1. freeze a small held-out prompt/session corpus before authoring;
2. build a deliberately plain control Bot with a fixed knowledge budget;
3. rebuild the same knowledge using one changed mechanic at a time;
4. transfer the winning rule to a different domain or enabled Matcher Profile with a lower or equal
   Bot-owned evidence budget;
5. keep the useful-near-miss and close/off-domain corpora paired while tuning repair;
6. run only the changed Regression Cases and short Scenarios during the inner loop, then one broader
   checkpoint after the candidate is green;
7. delete disposable source after recording its cost vector, failures, winning rule, and transfer
   result. Do not ship a laboratory Bot as product content.

A failed experiment is evidence. Preserve the observation, not the temporary Bot. If transfer needs
many more samples, hidden language assumptions, a lower normal threshold, or a new Meaning per
transition, the claimed authoring multiplier has not been demonstrated.

## 1. Start from conversation outcomes

Do not begin by inventing samples or editing matcher data. First write the smallest set of user-visible conversation outcomes the Package must support.

For each outcome, state:

- what the user is trying to accomplish or understand;
- what the Bot must answer, ask, remember, propose, or refuse;
- whether the turn is standalone or depends on prior topic, follow-up, state, confirmation, or host result;
- which language(s) must behave equivalently;
- what must remain unresolved rather than guessed.

A useful outcome is concrete enough to replay. “Support account help” is too broad; “user asks how to reset a password, receives the reset path, then asks ‘what if I no longer have that email?’ and stays in the same topic” is authorable.

## 2. Make the mechanic map explicit

Translate each outcome into the runtime mechanics it actually needs. Do not use a mechanic merely because it exists.

| Mechanic | Use it when | Direct proof should observe |
| --- | --- | --- |
| `semantic_resolution` | a user utterance must resolve to one Meaning | intended Meaning and relevant boundary cases |
| `behavior_response` | a Meaning must produce a particular answer | selected response/answer contract |
| `topic_scope` | a Behavior is valid only inside an active topic | behavior succeeds inside the topic and does not steal unrelated turns |
| `topic_activation` | a response establishes/refreshes topic context | active topic after the turn |
| `followup_scope` | a Behavior answers one explicit pending follow-up | the active follow-up makes the Behavior eligible |
| `followup_open` | a response creates a pending follow-up | expected active follow-up after the turn |
| `repair_continuation` | a below-threshold candidate may continue through deterministic repair | `repair_continuation` mode and the intended response |
| `repeat_ladder` | repeated input/Meaning changes the response path | repeat mode/stage at the relevant threshold |
| `state_eligibility` | author/conversation/context values gate a Behavior | state/context condition plus selected or rejected behavior |
| `state_response` | response selection depends on state | state/context condition plus response identity |
| `state_effect` | a response mutates author state | resulting author-state value |
| `opening` | the Bot initiates a conversation | Open interaction and its response/topic result |
| `fallback_recovery` | unresolved or repeat recovery is intentional | fallback/repeat mode and fallback response |
| `capability_proposal` | the Bot proposes host work | capability ID and relevant arguments |
| `capability_policy` | admission depends on policy/state/context | proposal receipt outcome under the policy condition |
| `capability_confirmation` | host work requires or ceases to require confirmation | confirmation-required receipt and Confirm flow, or the new non-confirming outcome |
| `capability_result` | accepted host results drive conversation | CapabilityResult step and result response |

This map is the slice contract. If the design needs none of the stateful mechanics, do not create stateful source. If it needs one, its proof belongs in the same slice.

## 3. Author vertical slices, not file-shaped batches

A normal authoring batch is one **vertical conversation slice**: the smallest coherent source change that makes one outcome work end to end.

A slice may change several source kinds together—for example Meaning + Behavior + follow-up + Scenario. That is preferable to a “Meanings batch” followed later by a “Behaviors batch”, because the latter leaves intermediate source that cannot prove a user-visible outcome.

For an existing target:

1. keep the last accepted source snapshot immutable as `BASE`;
2. copy it to an ordinary GVYA source tree and make one candidate slice;
3. add or modify the manual Regression Cases / Conversation Scenarios that directly prove the mechanics introduced by that slice;
4. run `gvya author-step BASE CANDIDATE --json` (which embeds the canonical `check-change/1` gate);
5. repair only the rejected candidate until its mechanic coverage and selected tests pass;
6. promote the passing candidate to become the next accepted baseline.

GVYA defines no patch dialect, AI-session format, or hidden authoring state. The semantic diff between two normal source snapshots is the slice.

## 4. Design the Meaning map before matcher wording

Give each independently answerable user goal one stable semantic ID. Split two goals when their correct answers, effects, or continuation mechanics differ. Merge paraphrases when they lead to the same behavior.

For every close Meaning pair, write the distinguishing decision in one sentence. Example:

| Meaning | Owns | Must not steal |
| --- | --- | --- |
| `gvya.use-cases` | Where GVYA can be used | How to create a Bot |
| `gvya.getting-started` | First implementation steps | Conceptual architecture |
| `gvya.packages` | Package composition and reuse | Matcher Profile configuration |

If the distinction cannot be explained, the Meaning boundary is not ready for samples.

Do not split a goal merely because one bounded value changes. When the operation, eligibility,
effect, and continuation contract stay the same, one Meaning may bind a declared String slot and
let response conditions, templates, or capability bindings consume that value. Split the Meaning
when the conversational contract changes, not when a finite parameter changes.

Use a normal Meaning + Behavior for recognizable user intent. Use a Capability only for explicit host work. Use the selected Fallback Package for unresolved/repeat recovery. Do not create catch-all Meanings that compete with every domain question.

## 5. Choose matching authority before adding evidence

Before writing samples, choose the narrowest matcher surface that truthfully represents the goal:

| Input shape | Authoring surface | Why |
| --- | --- | --- |
| fixed, high-confidence whole utterance | literal `patterns` rule | deterministic author authority without sample expansion |
| fixed grammar with optional words | anchored `patterns` rule with `*` or `^` | represents order and arity directly |
| finite aliases that produce one canonical value | Matcher Profile `pattern_sets` + declared String slot | one bounded rule can bind profile-native/domain aliases |
| bounded free text that is genuinely consumed downstream | `*{slot}` or `^{slot}` | captures normalized surface text for a template or binding |
| open natural-language paraphrase | positive `samples` | semantic evidence can generalize across phrasing |
| vocabulary that should make a Meaning reachable but should not resolve alone | `retrieval_terms` | retrieval without pretending the term is an utterance |
| a close utterance the Meaning must not own | `negative_samples` | expresses a real confusion boundary |
| unknown or unrelated input | selected Fallback Package | stays honest instead of creating a catch-all Meaning |

Structural patterns run before semantic retrieval and scoring. A structural winner is authoritative;
semantic evidence cannot override it. If no structural rule matches, GVYA runs the ordinary bounded
semantic matcher. Structural-only, semantic-only, and hybrid Meanings are all valid.

### Structural patterns are small and explicit

GVYA's structural grammar is AIML-inspired, not AIML-compatible:

- literal tokens are ordered and anchored to the whole utterance;
- `*` consumes one or more tokens and `^` consumes zero or more;
- `*{slot}` and `^{slot}` capture normalized surface text into a declared String slot;
- `<set:name>` matches a finite Matcher Profile set, while `<set:name>{slot}` captures its authored canonical String value;
- every rule needs at least one literal or set anchor, so wildcard-only fallback rules are invalid.

GVYA does not import AIML files and does not implement AIML categories, templates, `<that>`, SRAI,
recursion, or hidden topic state. Use explicit GVYA Behaviors, responses, follow-ups, topics, and state
for those conversation contracts.

Treat structural authority as a precision tool, not a shortcut to arbitrary understanding. A broad
rule such as `^ MATCHER ^` can steal distant utterances before semantic scoring. Pattern priority
cannot repair an overbroad rule because literal/set specificity and wildcard shape rank first. A
matched rule that later has ambiguous captures, missing required bindings, or no eligible response
fails closed; it does not quietly fall through to a semantic runner-up. Consume every captured value
in a response condition/template or capability binding—otherwise the capture is decorative data.

### Build semantic evidence after the boundary exists

GVYA normalizes text, retrieves a bounded candidate set, scores authored positive/negative evidence, then resolves only above the configured threshold and outside the ambiguity margin. Matcher evidence should express the Meaning boundary; it must not invent the boundary.

For each intended language, author natural phrasing along two independent axes:

- **speaking style** — professional/technical, everyday, young/simple;
- **subject familiarity** — newcomer, partially familiar, expert.

Use the 3×3 matrix as a coverage checklist, not a quota for near-duplicates. Keep genuinely different lexical/syntactic evidence and omit cells that add nothing.

Across the set, include when useful:

- a compact discriminative semantic spine, usually 2–5 meaningful content tokens;
- natural questions and command forms;
- terse/search-like wording that is still unambiguous;
- alternate why/when/where/how framing that still means the same goal;
- phrasing native to each language rather than literal translation symmetry.

Do not create punctuation, casing, or trivial inflection variants that normalization already owns.
Do not pre-fill all nine matrix cells. Start with the smallest discriminative set—often one semantic
spine plus a few genuinely different natural phrasings—and add evidence only when a held-out user
voice exposes a real gap. The 3x3 matrix discovers missing audiences; it is never a sample quota.

After the first evidence pass, run `gvya analysis PROJECT --json` and inspect `discoverability`. This bounded diagnostic removes identity terms shared by the Project and Bot IDs from authored samples and reports which Meaning the canonical kernel actually resolves. Treat `meanings_requiring_review` as a discovery queue, not an acceptance score: add natural name-free evidence only when the probe represents something a user would genuinely say. Generated discoverability probes never count as manual proof.

### Precision before coverage

Before adding broad evidence, write the closest plausible counterexample. A Meaning is not ready merely because its positive prompts resolve; it is ready when nearby prompts that mean something else also stay out. For every new or materially changed Meaning, keep at least one close-boundary/confounder probe held out from its positive samples.

Treat short or generic evidence as high-impact. Exact contiguous authored samples remain eligible evidence inside longer utterances, and otherwise-comparable shorter samples receive more lexical priority than longer samples. Language Profile ambiguity/direct-cue data still protects one-word collisions without giving any MeaningClass a private boost. A word such as `right`, `back`, `continue`, `meaning`, `runtime`, `authoring`, or `چی` can therefore be a useful embedded cue as well as a standalone utterance. Prove the intended embedded case and its nearest confounder; when a collision is unwanted, use a discriminative sample, authored negative, or Language Profile weighting rather than a hidden MeaningClass-specific exception or many near-duplicate samples.

Examples of the required authoring thought process:

```text
right
  -> acknowledgement may be correct

right triangle formula
  -> acknowledgement must not steal the turn

runtime
  -> topic entry may be correct

how does this capability reach the runtime?
  -> the generic runtime entry must not win merely because the token appears
```

Reusable Shared Smalltalk deserves the same boundary discipline. If a Shared social Meaning legitimately owns its canonical short utterances but steals a Bot-specific domain question, do not weaken or mutate the Shared Package globally. Specialize locally with an explicit Bot-owned whole-item `Replace` only when that Bot truly needs a different boundary. Because replacement has no hidden inheritance, preserve the canonical samples/responses the Bot still intends to support and add only the local boundary evidence. Prove both ordinary Smalltalk and the protected domain question after the replacement.

### Positive samples

Positive samples are authoritative evidence that really means this goal. Do not use internal IDs or keyword bags as positive sentences.

### Retrieval terms

Use `retrieval_terms` for specific domain vocabulary, former names, acronyms, or related terms that make a Meaning reachable but are awkward standalone utterances. Retrieval metadata is not a hidden synonym oracle and a vague single term must not become a catch-all resolver.

Keep retrieval bags content-bearing. Do not put question starters, connectives, pronouns, generic helper verbs, or other glue into `retrieval_terms` merely because they occurred in a useful sentence. Terms such as `why`, `what`, `instead`, or their language-native equivalents can combine with one ordinary domain verb and manufacture strong retrieval evidence for an unrelated turn. Strip the sentence down to the concept vocabulary the Meaning actually owns. Run an off-domain precision probe after retrieval-term edits; unrelated questions are the fastest tripwire for glue leakage.

### Negative samples

Use negative samples for real confusion boundaries: a close competing Meaning, reported speech that must not trigger, or a negation that reverses the assumed goal. Negatives are natural counterexamples, not unrelated filler. Do not assume a negative sample is a magic veto: inspect the actual competing scores and strengthen the semantic boundary from the side that is genuinely under-specified.

A negative must differ from the Meaning's positive evidence in meaningful content tokens, not only in conversational intent or a generic object. If the negative preserves the same content spine as a valid positive, bounded negative matching may penalize or block the Meaning's own best utterance. Before retaining a negative, compare its normalized/content-token spine with the nearest positives in every enabled language and prove both the intended exclusion and the nearby positive route.

### Repair tuning

Tune repair only with two held-out corpora:

1. **useful near-misses** — typos, clipped wording, or incomplete but still credible domain questions that should continue through repair;
2. **obvious off-domain utterances** — unrelated questions that must not be converted into a confident domain answer.

Move the repair floor only when the two corpora show a real separable boundary. Do not lower the normal resolution threshold or repair floor merely to make one stubborn test pass. If ordinary matcher evidence can cover a clearly in-domain question directly, prefer that over depending on repair.

Record the lowest useful near-miss score and the highest credible confounder score. Put the separate
repair floor only inside a demonstrated gap, and opt in only Behaviors that are safe to execute from
that weaker evidence. If the score ranges overlap, repair is not safe: add one discriminative lexical
spine, fix a reusable normalization gap in the owning Language Profile, split the Meaning boundary,
or let the turn fall back. A negative sample is boundary evidence, not a guaranteed score veto; read
the Why trace after adding it.

Also inspect `gvya analysis PROJECT --json` -> `repair_boundaries`. A manual `repair_continuation` expectation is flagged as fragile when its observed candidate lies within the configured warning margin of either the repair floor or normal resolution threshold. Move the boundary only with the paired corpora above; otherwise strengthen the evidence or rewrite the test so it does not depend on a three-hundredths-wide score window.

### Ambiguity debugging procedure

When two Meanings tie or a wrong candidate wins, stop adding samples blindly. Inspect in this order:

1. the top candidate IDs and scores;
2. which positive/retrieval/negative evidence each candidate matched;
3. shared short/generic tokens;
4. the intended Meaning as a standalone prompt;
5. the same vocabulary embedded in the competing longer prompt;
6. the closest semantic neighbor and its held-out boundary case.

Repair the semantic boundary, not the individual demo sentence. Prefer discriminative evidence, scoped lexical/profile primitives already supported by GVYA, or a clearer Meaning split. Threshold changes are the last resort and require corpus evidence.

### Matcher Profiles

GVYA's kernel and this recipe privilege no language. The Project's enabled Language/Matcher Profile pairs define
the languages in the current authoring and test matrix; if that profile set changes, replace or
extend the localized data and parity probes accordingly. Do not encode an ambient default language
or preserve examples for a language that the Project no longer enables.

Keep Language Profiles lexical and reusable: script normalization, colloquial expansion, canonical variants, glue/low-weight terms, negations, pronouns, reporting vocabulary, or continuation phrases that improve many Meanings. Keep Matcher Profiles structural and reusable through transparent `pattern_sets`. Do not encode domain answers or Meaning IDs in either profile merely to fix one case.

Use `pattern_sets` for a genuinely finite alias vocabulary used by structural rules. Keep aliases
language-native and map equivalent aliases in each language to the same canonical String values.
Set aliases compose deterministically; normalized alias collisions with different values, blank
values, and references to missing sets fail compilation. A set is transparent bounded author data,
not a synonym oracle or a place to hide answers.

### Bootstrap a Matcher Profile with AI, then prove it deterministically

Matcher Profiles are intentionally ordinary JSON authoring data, so an external AI can draft a
useful language profile quickly. Start with the smallest operational layer: language registration
and normalization, genuinely required glue/negation/number or transliteration rules, finite
`pattern_sets` used by structural or recovery routes, and held-out close-boundary utterances. Do not
wait for an exhaustive dictionary, and do not manufacture a synonym cloud before a real failure
requires it.

AI may propose aliases, colloquial forms, confounders, and locale-native Scenario turns for the
profiles the Project currently enables; it is not matcher authority. Compile the draft to catch
normalized collisions, prove every short/generic alias both inside and outside its intended scope,
exercise recovery-route parity across the enabled profile set, and add new lexical data only from
observed held-out gaps. Keep provider/session state outside GVYA source: the durable result is the
explicit profile data plus its tests.

## 6. Author responses as the product contract

Every response should answer the detected goal directly in its first useful sentence, then add only the smallest necessary next step, boundary, or example.

For content localized through enabled Matcher Profiles:

- provide a non-empty response path for every enabled language that owns the behavior;
- write native phrasing rather than literal translations;
- keep technical identifiers exact (`gvya.project.json`, `.gvya`, `package.json`);
- do not let any enabled locale promise behavior another enabled locale omits.

Use multiple variants only when all variants are equally correct. Variants provide response diversity; they do not provide semantic coverage.

A response must not advertise a continuation that the Bot cannot actually handle. Phrases such as “ask me why that matters”, “want the workflow?”, “say architecture”, “another one”, or any menu of next topics are executable promises. Every such promise needs a direct ordinary Meaning/Behavior path or an explicit follow-up path, plus a Scenario that proves the offered continuation.

Audit follow-up reachability, not only branch correctness. Every `followup_scope` consumer or conditioned continuation branch must have a real opener that can activate that scope along the same installed Bot/package graph. A branch behind a scope that no reachable response opens is dead authoring even if its conditions and response text are individually valid. Prove the opener -> scoped consumer path in a Scenario; orphan consumers/openers are design findings, not harmless unused content.

### Minimum engaging conversation spine

A small Bot can feel intentional without a large intent inventory. Start with one concise opening
that offers two or three truthful routes, answer each recognized goal in the first useful sentence,
and wire at most one useful next move. Add a bounded unresolved ladder, a repeat ladder only when
repetition should change the response, and one unknown -> offered route -> successful answer
Scenario. Response variants, a relevant pivot, and honest recovery create engagement; lower matcher
precision and near-duplicate samples do not.

### Low-data curiosity loop

Do not try to make every answer its own miniature conversation tree. Reuse a small truthful
curiosity surface across the Bot: one route that goes deeper, one lateral route, and one clean exit
to an ordinary goal. Rotate several equally-correct prompts for those routes, and keep a larger pool
of short, self-contained discoveries than the number likely to appear in one session. This creates
room to wander without multiplying Meanings or pretending the Bot understood unsupported input.

Make every suggested reply cheap to type and already executable. A one-word lane, a scoped number,
or an existing direct question is better than “tell me more” when the Bot cannot tell what “more”
refers to. Depth comes from connected authored islands and varied transitions, not from an
unbounded fallback, a synonym cloud, or a unique follow-up Meaning for every response.

### Controlled initiative, not random derailment

A lively Bot may change pace without abandoning coherence. Treat an “ADHD” personality as a response
style with a strict initiative contract:

1. answer the recognized goal first;
2. offer at most one optional next move;
3. keep that move tethered to supported knowledge or an honest recovery route;
4. make lateral curiosity explicit or user-invoked rather than silently replacing the answer; and
5. keep a cheap direct exit to an ordinary Meaning.

Use one shared curiosity Meaning/Behavior with several equally-correct discoveries before inventing a
tree of per-answer follow-ups. Give discoveries a deeper route, a lateral route, and a clean exit.
Novel wording can vary; factual authority cannot. Randomness may choose among equally-valid authored
discoveries, but it must not choose what the Bot claims to know, whether a Capability may run, or
whether the user's recognized question gets answered.

### Recovery-first authoring

First-turn resolution is useful, but it is not the whole product goal. When evidence is insufficient,
optimize for **productive recovery**: keep the user willing to continue while each turn reduces
uncertainty or reaches a real supported route. Fail closed rather than inventing an answer, but do
not make the user discover a magic sentence.

Each unresolved turn must do at least one new job:

- narrow the likely goal with two to four truthful choices;
- accept a terse answer such as “second”, “2”, or its language-native equivalent inside a bounded
  follow-up scope;
- give one concrete example of a supported direct question;
- change strategy after another miss instead of repeating “rephrase”; or
- offer an honest exploratory route that is actually wired and tested.

Keep short choice aliases follow-up-scoped so they cannot steal standalone input. A chosen route
should deliver useful content immediately, may reopen the same bounded chooser for another branch,
and must yield to an ordinary recognized Meaning or explicit topic change. Give the repair loop a
small budget—normally two or three misses—then switch tracks or state the limitation plainly.

Judge this layer separately from raw intent accuracy. Record at least: unresolved-to-supported-route
recovery, turns to recovery, repeated dead ends, offered-promise completion, normal-Meaning exit from
repair state, voluntary continuation through an exploratory route, and parity across enabled
languages. These observations are a quality scorecard, not permission to add hidden analytics or
runtime telemetry to the source contract.

## 7. Use stateful mechanics only when the conversation needs them

Use topics for short-lived domain context, not as a substitute for ordinary Meaning boundaries. Use `topic_scoped` only when a Behavior truly must not resolve outside that context.

Use `opens_followup` + `followup_scope` for a specific expected continuation. A generic standalone question still needs its own ordinary Meaning/Behavior path.

Use `repair_continuation_candidate` only for a Meaning that is safe to continue below the normal semantic score floor. Do not turn repair into a second fuzzy matcher.

Use repeat ladders only when repetition should change the conversational response. Keep thresholds intentional and prove the threshold transition in a Scenario. When repeated fallback would otherwise sound mechanical, author a small ladder such as acknowledgement -> pivot -> stronger topic change, and give later stages several equally-correct variants. Seeded deterministic selection may vary phrasing without introducing nondeterministic semantics.

Unresolved fallback should also avoid a dead-end rephrase loop. Prefer a bounded progression: acknowledge the miss, offer a truthful nearby path, then become more explicit about the limitation if misses continue. Do not make the user repeat an unrelated question indefinitely, and do not invent a domain answer merely to keep the conversation moving.

Use author/context/conversation state only for facts that genuinely govern eligibility or response/effect behavior. Do not create hidden state merely to steer wording.

## 8. Capabilities remain host-work contracts

A Capability is a proposal for host work, never permission for GVYA Runtime to perform the work itself.

Author the full vertical flow when applicable:

1. a Meaning/Behavior or result Behavior reaches the binding;
2. binding produces typed arguments;
3. policy admits, rejects, or requires confirmation;
4. confirmation is replayed through the canonical Confirm step when required;
5. the host returns a CapabilityResult;
6. result behavior turns the accepted result into conversation.

Tests must assert the proposal/receipt/result surfaces that matter to the changed slice. A test that only resolves the triggering Meaning is not proof of a changed capability policy or confirmation mechanic.

## 9. Tests have two different jobs

Do not confuse **slice proof** with **blast-radius safety**.

### Direct mechanic proof

Every runtime-affecting candidate slice must add or modify manual, non-generated tests that directly prove each required mechanic reported by `check-change`.

A single changed test may prove several mechanics if it genuinely observes all of them. Conversely, changing two Meanings does not become complete merely because one of them has a changed test.

Generated tests, unchanged historical tests, semantic-neighbor tests, and sentinels do not satisfy mechanic proof by themselves.

Mechanic-specific positive-state proof is required for mechanics the candidate enables or retains. When a Behavior removes a topic/follow-up/repeat/state mechanic, prove the candidate's observable response and let the old/new blast radius exercise the previous neighborhood; do not invent a positive assertion for state that should now be absent. A removed fallback may instead use direct negative response evidence.

Use a Regression Case for one-turn behavior that can be observed in one interaction. Use a Conversation Scenario when proof depends on Open, prior state, topic/follow-up lifetime, repetition, confirmation, capability result, or another multi-step relationship.

For structural resolution, direct proof should assert the Meaning, `meaning_slots` for every capture,
the response or capability surface that consumes those slots, and Why codes such as
`semantic.structural.matched`. Add a neighboring utterance the rule must not steal. For a hybrid
Meaning, also prove one natural semantic paraphrase for which no structural rule matches.

### Blast-radius safety

GVYA separately selects tests affected by the old and new dependency neighborhood, semantic neighbors, and stable sentinels. Those tests protect nearby behavior from regressions even when they are not part of the slice proof.

Keep proof inputs held out from positive samples. Exact sample replay can be useful as a narrow regression, but it does not demonstrate that the authored boundary generalizes.

## 10. Use `author-step` as the agent inner loop

The canonical agent loop is:

```text
accepted BASE
    -> ordinary candidate source slice
    -> gvya author-step BASE CANDIDATE --json
       -> embedded check-change/1 authority
          -> semantic ChangeSet
          -> required mechanic coverage
          -> changed/manual proof tests
          -> blast radius + semantic neighbors + sentinels
          -> canonical build/runtime execution
       -> deterministic next_actions
    -> repair candidate and rerun, keep unchanged BASE, or promote accepted candidate
```

`gvya.cli.check-change/1` remains the acceptance authority and exposes `impact.mechanic_coverage` with:

- `required` — number of required mechanic obligations;
- `covered` — obligations directly proven by changed manual tests;
- `missing` — obligations still blocking the slice;
- `requirements[]` — mechanic label, source kind, subject, coverage state, and the changed tests that prove it.

`gvya.cli.author-step/1` consumes that exact evaluation rather than recreating it. A missing requirement becomes `add_direct_mechanic_proof` with the exact mechanic/source-kind/subject. Build, Runtime-load, selected Regression/Scenario failures, and incomplete selected-suite execution become their own bounded actions. Candidate source load/resolution/composition errors are also repairable pre-gate actions; an invalid BASE instead blocks the loop because the accepted snapshot must remain trustworthy.

Treat the embedded gate as authority. An external agent must not recompute acceptance from filenames, prompts, or its own guessed dependency graph, and it must not mutate BASE while repairing a rejected candidate.

`ready_to_promote` means the candidate slice passed its incremental boundary and may be promoted by the host. `no_change` keeps BASE. Neither state makes the full target permanently valid.

### Validate the host loop without inventing a second gate

The durable external-agent fixture chain lives in `validation/authoring-e2e/`. Use it at authoring-loop milestones to prove malformed-candidate repair, missing mechanic proof, stateful and capability slices, intentional removal, targeted failing-test repair, global full-suite escalation, and sequential host-owned promotion. The runner may assert expected actions, but it must never infer mechanic requirements itself; those continue to come only from `author-step` / `check-change/1`.

## 11. Escalate global changes instead of pretending they are local

Package order, structural Meaning patterns, Matcher Profile semantics (including `pattern_sets`), project language/semantic/conversation configuration, global capability/type/asset contracts, and exhausted semantic-neighbor analysis can influence too much of the target for a bounded local claim. `check-change` therefore sets `full_suite_required=true` and selects the complete candidate test suite.

Do not bypass this escalation to make an authoring loop faster. The point of slices is to keep ordinary changes cheap while making genuinely global changes pay the global validation cost.

After a sequence of accepted slices, and always at a milestone/release checkpoint, run the normal full-target `gvya check` / `gvya test` gates.

## 12. Blind conversation and authoring-quality pass

After deterministic authored tests pass, use blind prompts that were not copied from samples or test fixtures. Judge the resulting conversation as a user would:

- did the right goal resolve?
- did the first answer actually answer it?
- did stateful continuation feel intentional rather than sticky?
- did capability boundaries remain explicit?
- did nearby Meanings stay distinct?
- did every enabled Language/Matcher Profile pair preserve the same contract?
- when the Bot did not know, did it avoid both a dead-end “rephrase” loop and a confident wrong answer?

A freeze-quality Bot should cover these distinct surfaces:

| Surface | What to prove |
| --- | --- |
| Domain-noun smoke | first-class domain nouns and concepts can be asked directly |
| Name-free discoverability | natural questions still reach their Meaning without requiring the product/Bot name |
| Boundary/confounder precision | short/generic evidence works where intended but does not hijack longer unrelated turns |
| Off-domain precision | clearly unrelated prompts go to fallback instead of the nearest domain Meaning |
| Typo/near-miss tolerance | ordinary misspellings and clipped domain questions still resolve or repair when credible |
| Ambiguity | close Meanings stay separated above the configured margin |
| Structural authority | explicit rules win only their intended whole-utterance shapes and bind the expected canonical/surface slots |
| Repeat quality | repeated input advances the intended ladder and later stages do not sound like one fixed sentence |
| Fallback quality | consecutive misses remain honest, useful, and bounded |
| Follow-up truthfulness | every continuation/menu offered by a response actually works |
| Progressive recovery | an unknown turn narrows to a terse scoped choice or wired exploration route, and a later known turn exits recovery cleanly |
| Enabled-profile parity | every enabled language profile preserves the same semantic and recovery contract |

### Mixed adversarial conversation

Before freeze, run one mixed session of roughly twenty turns. It should deliberately combine: known domain questions, terse topic words, typos, an obvious off-domain question, a sudden topic change, vague input, exact repeat, a promised follow-up, profile/locale switches when more than one language is enabled, and a return to a known domain topic. The goal is not a magic score; it is to expose sticky state, false confidence, repetitive recovery text, and menus that do not work.

At a reference-Bot or recipe milestone—not during every edit—expand the checkpoint to roughly fifty
turns from several user postures: an impatient task user, a vague novice, a skeptical boundary tester,
an off-domain provocateur, and a curious repeater. Include at least one continuous twenty-turn session
so sticky state cannot hide inside short resets. Record productive turns, unsupported-confidence
failures, broken promises, repeated dead ends, distinct productive transition edges, and voluntary
continuation/exit. Run the canonical broad gate once after this checkpoint, not after each persona.

### Delta-aware iteration

Do not rerun the entire conversational corpus after every evidence edit. During iteration, run the changed Meaning, its nearest semantic neighbors, short/generic-token confounders, and any affected follow-up/repeat path. When that bounded delta is clean, run the broader blind/quality corpus once at checkpoint closure. This keeps the inner loop fast without trading away precision.

### Quality scorecard

For nontrivial Bots, record the checkpoint verdict across: domain recall, off-domain precision, typo tolerance, ambiguity rate/findings, repeat quality, fallback quality, follow-up truthfulness, multi-turn recovery, and enabled-profile parity. A single aggregate number is optional and must not hide a known boundary defect.

When a blind failure exposes a real semantic gap, add a held-out regression/scenario that reproduces the gap, then repair source through another normal candidate slice. Do not patch only the demo prompt. Keep quality probes held out from positive samples.

The GVYA Help Bot keeps a concrete executable example in `validation/fixtures/gvya-help-authoring-quality.json`, consumed by the existing `validation/gvya-help-bot-contract.mjs` path. During authoring, `npm run test:gvya-help:quality` runs only that bounded quality surface; `npm run test:gvya-help` remains the broader checkpoint contract. This fixture is an example of the discipline, not a second semantic authority or a universal fixed prompt inventory.

## 13. Definition of done

A Package or Bot authoring milestone is complete when:

- conversation outcomes and Meaning boundaries are explicit;
- every used runtime mechanic is intentional;
- each accepted slice has complete direct mechanic proof;
- blast-radius and selected sentinel tests pass through the canonical compiled Runtime;
- global changes have paid full-suite validation when required;
- matcher evidence is discriminative and language-native rather than a keyword dump;
- structural rules are anchored, their captures are consumed, and semantic fallback remains proven for hybrid Meanings;
- responses and capability flows match the product contract;
- held-out and blind conversation checks reveal no known unresolved boundary defect;
- domain-noun, boundary/confounder, off-domain, repeat/fallback, truthful-follow-up, recovery, and language-parity quality checks have been exercised for the target;
- bounded name-free discoverability diagnostics have been reviewed without promoting generated probes into proof;
- repair/threshold tuning, when changed, is justified by both useful near-miss and obvious off-domain corpora;
- full `gvya check` / `gvya test` passes at the checkpoint boundary.

Do not optimize authoring for the number of samples, files changed, or tests executed. Optimize for the smallest coherent conversation slice whose mechanics are explicit and directly proven.
