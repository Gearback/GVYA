# Methodology — Test 1: Equal Authored Evidence

## Question

Test 1 isolates engine behavior from authoring strategy:

> If GVYA, AIML 2.0, and ChatScript receive exactly the same authored semantic evidence, what routing behavior does each engine provide without additional engine-specific semantic authoring?

This is deliberately different from Test 2. Test 2 asks what each system can build when the problem and authoring budget are held constant and idiomatic authoring is allowed; see [`../test2-equal-authoring-budget/`](../test2-equal-authoring-budget/).

## Neutral domain

The benchmark uses a synthetic product-support domain created for this comparison. It is not the GVYA help bot and is not reused from the corpora that guided GVYA matcher development.

There are 24 support intents covering account, billing, orders, shipping, product/app operation, data, security, and support contact topics.

## Equal evidence rule

Each intent owns exactly four training utterances in `frozen/benchmark-spec.json`. Those 96 utterances are the complete semantic evidence budget for every engine.

Generation is mechanical:

- GVYA receives each utterance as a positive Meaning sample;
- AIML receives each utterance as a literal `<pattern>` category;
- ChatScript receives each utterance as an input pattern.

Responses are machine-scored `BENCH:*` tokens so prose generation cannot affect routing accuracy.

No system receives evaluation utterances as authored evidence.

## What is allowed

Engine-native runtime processing is allowed. This includes normalization, tokenization, morphology, spelling behavior, and other processing shipped as part of the selected engine/runtime.

The benchmark does not add engine-specific semantic knowledge. In particular:

- AIML gets no wildcard generalizers, sets, maps, or benchmark-specific spelling corpus;
- ChatScript gets no hand-authored concepts, synonyms, or semantic wildcard rules;
- GVYA gets no negative samples, retrieval terms, or structural patterns.

The ChatScript control file is routing/runtime infrastructure only. It directs input to the generated benchmark topic and enables the standard English processing used by the engine; it contains no benchmark intent vocabulary.

## Evaluation corpus

The 288 evaluation turns are frozen in the same benchmark specification:

| Track | N | Meaning |
|---|---:|---|
| Seen | 96 | exact authored evidence rows |
| Unseen paraphrase | 48 | new semantic rewordings |
| Typo/noise | 24 | misspelled/noisy forms |
| Word order | 24 | same intent with reordered wording |
| Close confounder | 48 | in-domain requests containing vocabulary close to neighboring intents |
| OOD | 48 | clearly out-of-domain requests; expected result is fallback |

Close confounders are **in-domain positive cases**, not negative cases. They test whether a system can select the intended nearby meaning rather than simply refusing ambiguous-looking language.

## Metrics

The two primary metrics are reported separately:

1. strict intent accuracy on the 240 in-domain turns;
2. false-positive rate on the 48 OOD turns.

There is deliberately no composite score that trades recall against safety using arbitrary weights.

Additional diagnostics include wrong-intent rate, macro F1 including fallback, per-track accuracy, raw predictions, and exact paired binomial/McNemar-style comparisons.

## Isolation and replay

Each AIML evaluation turn receives a fresh Program-Y client context. Each ChatScript turn receives a unique user identity and starts a fresh conversation. GVYA turns use empty state. This prevents case order from creating hidden conversational advantages.

Every engine executes two complete passes. A benchmark run fails if the prediction sequence changes between passes.

## Freeze chronology

1. The neutral domain, 96 training rows, 288 evaluation cases, and expected labels were created.
2. Three semantic sources were generated mechanically.
3. `benchmark-spec.json` and all semantic sources were hash-frozen.
4. GVYA was executed.
5. No corpus, expected label, semantic source, or GVYA prediction was changed afterward.
6. Exact Program-Y and ChatScript runtimes were acquired and executed against the same frozen inputs.
7. Final runs reproduced all three prediction sequences exactly.

This chronology is recorded because a held-out evaluation loses meaning if the benchmark is tuned after observing competitor results.

## Scope limits

This is one synthetic English routing benchmark. It does not test authored multi-turn dialogue, rich response control, multilingual behavior, capabilities/actions, authoring ergonomics, or runtime performance under normalized host boundaries.

Most importantly, it does not let AIML or ChatScript use their strongest authoring abstractions. That would violate the equal-evidence question. Those capabilities belong in Test 2, where all systems receive the same problem and the same authoring budget.
