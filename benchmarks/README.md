# GVYA Benchmarks

GVYA benchmark material is published as reproducible evidence, not as marketing-only score tables. Each completed benchmark keeps its methodology, frozen inputs, authored competitors, raw outputs, runtime identities, and failure cases together.

## Benchmark suite

The two completed tests answer different questions and should be read together.

| Test | Question | GVYA | ChatScript 14.1 | AIML 2.0 / Program-Y 3.6 | Main takeaway |
|---|---|---:|---:|---:|---|
| [Test 1 — Equal Authored Evidence](test1-equal-authored-evidence/) | What does each engine do with the same sparse semantic evidence? | **49.17% in-domain** | 42.08% | 40.00% | GVYA recovered the most in-domain cases; all three had 0% OOD false-positive rate. |
| [Test 2 — Equal Authoring Budget](test2-equal-authoring-budget/) | What can each system build with the same problem and exactly 115 user-language evidence rows? | **35.83% positive coverage** | 21.67% | 13.33% | GVYA produced the most fresh positive coverage; ChatScript had the strongest near-domain safety. |

These percentages are not interchangeable: Test 1 measures in-domain routing accuracy over its own frozen 240-case in-domain set, while Test 2 reports positive held-out coverage over 120 positive cases. Read each test's methodology before comparing absolute percentages.

## Test 1 — Equal Authored Evidence

[`test1-equal-authored-evidence/`](test1-equal-authored-evidence/) gives all three engines exactly 96 authored semantic examples and evaluates them on a frozen 288-turn corpus.

Headline result:

- GVYA: **118/240 (49.17%)** in-domain accuracy, 0/48 OOD false positives.
- ChatScript 14.1: **101/240 (42.08%)**, 0/48 OOD false positives.
- AIML 2.0 / Program-Y 3.6: **96/240 (40.00%)**, 0/48 OOD false positives.

The full result, including GVYA's three wrong-intent close-confounder cases and the shared 0/48 unseen-paraphrase result, is in [`test1-equal-authored-evidence/RESULTS.md`](test1-equal-authored-evidence/RESULTS.md).

## Test 2 — Equal Authoring Budget

[`test2-equal-authoring-budget/`](test2-equal-authoring-budget/) gives all three systems the same FolioBox product brief and exactly 115 user-language evidence rows, while allowing idiomatic native authoring.

Headline result:

- GVYA: **43/120 (35.83%)** positive held-out coverage; 24/30 near-domain + OOD safety; **67/150 (44.67%)** strict all-turn accuracy.
- ChatScript 14.1: **26/120 (21.67%)** positive coverage; **28/30 (93.33%)** safety; 54/150 strict all-turn accuracy.
- AIML 2.0 / Program-Y 3.6: **16/120 (13.33%)** positive coverage; 24/30 safety; 40/150 strict all-turn accuracy.

The full result, budget/source audit, freeze chronology, raw predictions, and failure analysis are in [`test2-equal-authoring-budget/RESULTS.md`](test2-equal-authoring-budget/RESULTS.md) and [`test2-equal-authoring-budget/METHODOLOGY.md`](test2-equal-authoring-budget/METHODOLOGY.md).

## Reading the two tests together

The supported conclusion is bounded:

- with **identical sparse authored evidence**, GVYA ranked first on Test 1's in-domain routing axis;
- with **equal user-language authoring budget**, GVYA ranked first on Test 2's positive-coverage and strict all-turn axes;
- ChatScript was the strongest close-boundary rejector in Test 2;
- AIML remained strongest on Test 2's small context sub-track;
- none of these results establishes universal chatbot superiority, prose quality, or normalized runtime performance.

Both tests publish their failures as part of the result rather than only publishing the winning cases.
