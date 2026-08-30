# Test 2 — Equal Authoring Budget — Final Fair Rerun

**Status: COMPLETE / FROZEN**

The earlier 120-SAU attempt was discarded because its budget accounting charged GVYA internal context/wiring decisions asymmetrically and withheld normal GVYA retrieval-term authoring. This rerun uses the same evidence-row budget model that had previously produced stable GVYA-vs-AIML results, now extended to ChatScript.

## Primary results

| System | Positive held-out coverage | Near-domain + OOD safety | Strict all-turn accuracy |
|---|---:|---:|---:|
| **GVYA** | **43/120 (35.83%)** | 24/30 (80.00%) | **67/150 (44.67%)** |
| ChatScript 14.1 | 26/120 (21.67%) | **28/30 (93.33%)** | 54/150 (36.00%) |
| AIML 2.0 / Program-Y 3.6 | 16/120 (13.33%) | 24/30 (80.00%) | 40/150 (26.67%) |

No system produced an OOD false positive: all were **15/15** on clearly off-domain requests. The safety difference is entirely on the deliberately difficult near-domain confounders.

## Track breakdown

| Track | GVYA | ChatScript | AIML 2.0 |
|---|---:|---:|---:|
| Paraphrase | **15/40 (37.5%)** | 7/40 (17.5%) | 4/40 (10.0%) |
| Compressed | **10/20 (50.0%)** | 7/20 (35.0%) | 8/20 (40.0%) |
| Typo | 6/20 (30.0%) | **8/20 (40.0%)** | 0/20 (0.0%) |
| Polite/noisy | **10/20 (50.0%)** | 1/20 (5.0%) | 0/20 (0.0%) |
| Context | 2/20 (10.0%) | 3/20 (15.0%) | **4/20 (20.0%)** |
| Near-domain rejection | 9/15 (60.0%) | **13/15 (86.7%)** | 9/15 (60.0%) |
| OOD rejection | **15/15 (100%)** | **15/15 (100%)** | **15/15 (100%)** |

## Paired comparison

On the 120 positive held-out turns:

- GVYA vs ChatScript: **30 GVYA-only correct vs 13 ChatScript-only correct**, exact two-sided paired p ≈ **0.0137**.
- GVYA vs AIML: **31 GVYA-only correct vs 4 AIML-only correct**, p ≈ **3.47×10⁻⁶**.
- ChatScript vs AIML: 20 ChatScript-only vs 10 AIML-only, p ≈ 0.0987.

Across all 150 turns, GVYA vs ChatScript narrows to 30 vs 17 because ChatScript rejects four near-domain confounders that GVYA accepts; the all-turn paired p is ≈ 0.0789. GVYA vs AIML remains 33 vs 6, p ≈ 1.43×10⁻⁵.

## What this supports

Under this specific **equal 115-row authoring budget**, GVYA generated substantially more fresh positive-language coverage than AIML and ChatScript. Its strongest advantage was paraphrase and polite/noisy phrasing. ChatScript was the safest on close confounders and had the best typo result. AIML's authored `<that>` patterns gave it the highest context score in this small context track, but its overall fresh positive coverage was lowest.

The fair conclusion is therefore not simply "GVYA wins everything":

- **Best positive coverage:** GVYA.
- **Best near-domain precision/safety:** ChatScript.
- **Best strict all-turn count on this corpus:** GVYA.
- **Best context track:** AIML, narrowly.

## GVYA failure modes

GVYA had six wrong-intent positive cases. Four broad natural requests were pulled toward the generic `device.about` meaning; two context follow-ups were captured by their corresponding direct meaning (`ocr.language` or `export.searchable`) instead of the scoped follow-up.

GVYA also accepted six of 15 near-domain confounders: printer paper jam, cloud-drive sharing, router factory reset, label-printer roller cleaning, phone-camera duplex scanning, and Google Drive document deletion. These are real precision costs of its additional recall and are intentionally retained in the raw results.

## Reproducibility

- Semantic source freeze: `frozen/PRE_EVALUATION_FREEZE.json`
- Budget/source audit: `frozen/FAIRNESS_AUDIT.json`
- Runtime and prediction locks: `frozen/RUNTIME_AND_RESULT_LOCK.json`
- Blind corpus hash: `frozen/BLIND_CORPUS_SHA256.txt`
- Raw predictions: `results/*.raw.jsonl`
- Full paired/error analysis: `results/analysis.json`

A fresh-process rerun produced exactly the same prediction hashes for all three engines. No semantic source file changed after the blind corpus was revealed.
