# Test 1 — Equal Authored Evidence

**Status: COMPLETE**

This test asks one deliberately narrow question: **when three offline authored conversation engines receive the same semantic evidence, how much useful matching behavior does each engine provide without extra engine-specific semantic authoring?**

It does **not** measure expert authoring productivity. Engine-specific semantic authoring is reserved for Test 2.

## Headline result

| System | In-domain accuracy | OOD false-positive | Wrong-intent | Macro F1 | Replay |
|---|---:|---:|---:|---:|---:|
| **GVYA** | **49.17%** (118/240) | **0.00%** | 1.25% | 64.32% | PASS |
| ChatScript 14.1 | 42.08% (101/240) | **0.00%** | **0.00%** | 58.40% | PASS |
| AIML 2.0 / Program-Y 3.6 | 40.00% (96/240) | **0.00%** | **0.00%** | 56.46% | PASS |

On Test 1's primary in-domain accuracy axis, **GVYA ranks first**: 49.17% vs 42.08% for ChatScript and 40.00% for Program-Y, with 0/48 OOD false positives for all three systems.

This is a bounded result, not a universal superiority claim.

## Track breakdown

| System | Seen | Unseen paraphrase | Typo/noise | Word order | Close confounder | OOD rejection |
|---|---:|---:|---:|---:|---:|---:|
| **GVYA** | 100.00% | 0.00% | **66.67%** | **4.17%** | **10.42%** | 100.00% |
| ChatScript 14.1 | 100.00% | 0.00% | 20.83% | 0.00% | 0.00% | 100.00% |
| AIML 2.0 / Program-Y 3.6 | 100.00% | 0.00% | 0.00% | 0.00% | 0.00% | 100.00% |

## Paired comparison

| Pair | Both correct | First only | Second only | Both wrong | Exact p |
|---|---:|---:|---:|---:|---:|
| GVYA vs ChatScript | 101 | **17** | 0 | 122 | 1.5258789e-05 |
| GVYA vs Program-Y | 96 | **22** | 0 | 122 | 4.7683716e-07 |
| ChatScript vs Program-Y | 96 | 5 | 0 | 139 | 0.0625 |

## What GVYA got wrong

GVYA produced three wrong-intent decisions, all on close-confounder cases:

| Input | Expected | GVYA prediction |
|---|---|---|
| `change my password not my email address` | `password.reset` | `email.change` |
| `i do not want a refund i only need the receipt` | `billing.receipt` | `billing.refund` |
| `refund the charge but do not cancel my subscription` | `billing.refund` | `subscription.cancel` |

ChatScript and Program-Y produced no wrong-intent decision in this test; they fell back more often. GVYA therefore buys additional recall with a small close-boundary precision cost.

## Important negative result

**None of the three systems matched any of the 48 unseen paraphrases.** Four literal training examples per intent are too sparse for broad paraphrase coverage in this configuration.

Test 2 asks the practical follow-up question: with the same authoring budget, how effectively can each system use its native abstractions? It is published separately under [`../test2-equal-authoring-budget/`](../test2-equal-authoring-budget/).

## Frozen design

- 24 intents × 4 authored examples = 96 semantic evidence rows per engine.
- 288 evaluation turns: 240 in-domain + 48 OOD.
- No semantic-source or expected-label tuning after the first frozen GVYA run.
- Two full passes per engine; prediction sequences must be identical.

Benchmark specification SHA-256: `8dc54256a3eebc5fbca4c2930d3ad133378d524fc54a002909c6b97482e220d4`

Semantic-source lock SHA-256: `add80b1f898b652928d024d3b3b50bd1525a4f1456ade2d7bfc023ab6ae73198`

See `METHODOLOGY.md` for the fairness boundary and `RUNTIME_LOCK.json` for exact runtime identities. Raw predictions are under `results/`.

## Timing note

Per-turn timings remain in the summaries but are **not** a performance comparison because the host boundaries differ (GVYA WASM/JSON ABI, in-process Program-Y, and ChatScript localhost TCP).
