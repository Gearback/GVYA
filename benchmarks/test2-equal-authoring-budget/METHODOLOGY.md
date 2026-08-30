# Test 2 — Equal Authoring Budget

## Question

If GVYA, ChatScript 14.1, and AIML 2.0 / Program-Y 3.6 receive the same fictional product brief and exactly the same user-language authoring budget, which system turns that budget into the most useful held-out conversational coverage?

This is a routing/context/safety benchmark. Response prose is identical tokenized output and is not scored.

## Budget

Each engine receives exactly **115 user-language evidence rows**:

- **100 direct rows:** 20 direct meanings × 5 rows.
- **15 contextual rows:** 5 follow-ups × 3 rows.

Engine plumbing does not consume the budget. That includes GVYA behavior/follow-up wiring, AIML `<that>` attachment itself, ChatScript topic/control wiring, IDs, response facts, generated wrappers, and the single global fallback.

The 115 rows are spent idiomatically:

- **GVYA:** each direct meaning uses 3 positive samples + 2 retrieval terms; each context uses 2 samples + 1 retrieval term.
- **AIML 2.0:** each direct meaning uses 3 literal categories + 2 `^` zero-or-more generalizers; each context uses 2 literal `<that>` categories + 1 generalized `<that>` category.
- **ChatScript:** each direct meaning uses 2 concept members + 3 rules; each context uses 3 rejoinders.

A separately authored lexical member is one row. Hidden alternatives are not allowed to compress multiple authoring decisions into one row. The fairness audit verifies the source rather than trusting the ledger alone.

## Freeze order

1. Create the FolioBox product brief.
2. Create and hash the 150-turn blind corpus.
3. Author all three bots using the product brief only. The authoring script does not read the blind corpus.
4. Require all three bots to pass 20 canonical direct prompts + 5 canonical setup/follow-up exchanges.
5. Freeze hashes of all semantic source files.
6. Reveal/run the blind corpus.
7. Make no semantic authoring changes after reveal.
8. Repeat every engine in fresh processes and compare prediction hashes.

## Corpus

150 scored turns:

- 40 paraphrases
- 20 compressed requests
- 20 typo/noisy requests
- 20 polite/noisy natural requests
- 20 contextual follow-ups
- 15 near-domain confounders
- 15 clearly off-domain requests

Positive coverage is reported separately from negative safety. There is no invented weighted composite score.

## Runtime policy

All three systems run locally with no LLM and no network lookup at runtime. Exact runtime and dependency hashes are in `frozen/RUNTIME_AND_RESULT_LOCK.json`.
