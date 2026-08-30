# Benchmark Test 1 — Equal Authored Evidence

**GVYA vs ChatScript 14.1 vs AIML 2.0 / Program-Y 3.6**

This directory contains the complete reproducible evidence for GVYA benchmark Test 1.

The question is intentionally narrow: with exactly the same 96 authored semantic examples, what does each engine provide without extra engine-specific semantic authoring?

Final in-domain accuracy:

- **GVYA: 49.17%**
- **ChatScript 14.1: 42.08%**
- **AIML 2.0 / Program-Y 3.6: 40.00%**

All three systems produced **0/48 OOD false positives** and passed deterministic replay.

Read [`RESULTS.md`](RESULTS.md) for the complete result and failure analysis, then [`METHODOLOGY.md`](METHODOLOGY.md) for the fairness boundary and freeze chronology.

Test 2 — same problem plus the same authoring budget — is published separately under [`../test2-equal-authoring-budget/`](../test2-equal-authoring-budget/).
