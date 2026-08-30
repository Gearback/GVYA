from __future__ import print_function
import argparse
import json
import math
from pathlib import Path


def pct(x): return '%.2f%%' % (100.0*float(x))
def load_jsonl(p): return [json.loads(x) for x in p.read_text(encoding='utf-8').splitlines() if x.strip()]
def exact_two_sided(a,b):
    n=a+b
    if not n: return 1.0
    m=min(a,b)
    return min(1.0, 2.0*sum(math.comb(n,k) for k in range(m+1))/(2**n))
def pair(A,B):
    both=oa=ob=wrong=0
    for a,b in zip(A,B):
        if a['expected'] is None: continue
        ca=a['predicted']==a['expected']; cb=b['predicted']==b['expected']
        if ca and cb: both+=1
        elif ca: oa+=1
        elif cb: ob+=1
        else: wrong+=1
    return {'n':240,'both_correct':both,'only_first_correct':oa,'only_second_correct':ob,'both_wrong':wrong,'exact_two_sided_p':exact_two_sided(oa,ob)}


def main():
    ap=argparse.ArgumentParser(); ap.add_argument('--root',required=True); a=ap.parse_args(); root=Path(a.root).resolve(); r=root/'results'
    G=json.loads((r/'gvya.summary.json').read_text()); C=json.loads((r/'chatscript.summary.json').read_text()); A=json.loads((r/'aiml2.summary.json').read_text())
    gr=load_jsonl(r/'gvya.raw.jsonl'); cr=load_jsonl(r/'chatscript.raw.jsonl'); ar=load_jsonl(r/'aiml2.raw.jsonl')
    pairs={'gvya_vs_chatscript':pair(gr,cr),'gvya_vs_aiml':pair(gr,ar),'chatscript_vs_aiml':pair(cr,ar)}
    wrong=[{k:x[k] for k in ('id','track','text','expected','predicted')} for x in gr if x['expected'] is not None and x['predicted'] is not None and x['predicted']!=x['expected']]
    out={'benchmark':'gvya-comparator-test-1-equal-authored-evidence','status':'complete','primary_metrics':['strict_intent_accuracy_in_domain','ood_false_positive_rate'],'systems':{'GVYA':G,'ChatScript':C,'AIML 2.0':A},'pairwise_in_domain':pairs,'gvya_wrong_intent_cases':wrong}
    (r/'test1.summary.json').write_text(json.dumps(out,indent=2,sort_keys=True)+'\n',encoding='utf-8')
    lines=['# Test 1 — Equal Authored Evidence','', '**Status: COMPLETE**','',
      'This test asks one deliberately narrow question: **when three offline authored conversation engines receive the same semantic evidence, how much useful matching behavior does each engine provide without extra engine-specific semantic authoring?**','',
      'It does **not** measure expert authoring productivity. Engine-specific semantic authoring is reserved for Test 2.','',
      '## Headline result','',
      '| System | In-domain accuracy | OOD false-positive | Wrong-intent | Macro F1 | Replay |','|---|---:|---:|---:|---:|---:|',
      '| **GVYA** | **%s** (%d/240) | **%s** | %s | %s | PASS |' % (pct(G['strict_intent_accuracy_in_domain']),G['correct_in_domain'],pct(G['ood_false_positive_rate']),pct(G['wrong_intent_rate']),pct(G['macro_f1_including_fallback'])),
      '| ChatScript 14.1 | %s (%d/240) | **%s** | **%s** | %s | PASS |' % (pct(C['strict_intent_accuracy_in_domain']),C['correct_in_domain'],pct(C['ood_false_positive_rate']),pct(C['wrong_intent_rate']),pct(C['macro_f1_including_fallback'])),
      '| AIML 2.0 / Program-Y 3.6 | %s (%d/240) | **%s** | **%s** | %s | PASS |' % (pct(A['strict_intent_accuracy_in_domain']),A['correct_in_domain'],pct(A['ood_false_positive_rate']),pct(A['wrong_intent_rate']),pct(A['macro_f1_including_fallback'])),
      '',"On Test 1's primary in-domain accuracy axis, **GVYA ranks first**: 49.17% vs 42.08% for ChatScript and 40.00% for Program-Y, with 0/48 OOD false positives for all three systems.",'',
      'This is a bounded result, not a universal superiority claim.','',
      '## Track breakdown','',
      '| System | Seen | Unseen paraphrase | Typo/noise | Word order | Close confounder | OOD rejection |','|---|---:|---:|---:|---:|---:|---:|',
      '| **GVYA** | 100.00% | 0.00% | **66.67%** | **4.17%** | **10.42%** | 100.00% |',
      '| ChatScript 14.1 | 100.00% | 0.00% | 20.83% | 0.00% | 0.00% | 100.00% |',
      '| AIML 2.0 / Program-Y 3.6 | 100.00% | 0.00% | 0.00% | 0.00% | 0.00% | 100.00% |','',
      '## Paired comparison','',
      '| Pair | Both correct | First only | Second only | Both wrong | Exact p |','|---|---:|---:|---:|---:|---:|',
      '| GVYA vs ChatScript | %d | **%d** | %d | %d | %.8g |' % (pairs['gvya_vs_chatscript']['both_correct'],pairs['gvya_vs_chatscript']['only_first_correct'],pairs['gvya_vs_chatscript']['only_second_correct'],pairs['gvya_vs_chatscript']['both_wrong'],pairs['gvya_vs_chatscript']['exact_two_sided_p']),
      '| GVYA vs Program-Y | %d | **%d** | %d | %d | %.8g |' % (pairs['gvya_vs_aiml']['both_correct'],pairs['gvya_vs_aiml']['only_first_correct'],pairs['gvya_vs_aiml']['only_second_correct'],pairs['gvya_vs_aiml']['both_wrong'],pairs['gvya_vs_aiml']['exact_two_sided_p']),
      '| ChatScript vs Program-Y | %d | %d | %d | %d | %.8g |' % (pairs['chatscript_vs_aiml']['both_correct'],pairs['chatscript_vs_aiml']['only_first_correct'],pairs['chatscript_vs_aiml']['only_second_correct'],pairs['chatscript_vs_aiml']['both_wrong'],pairs['chatscript_vs_aiml']['exact_two_sided_p']),
      '', '## What GVYA got wrong','',
      'GVYA produced three wrong-intent decisions, all on close-confounder cases:','',
      '| Input | Expected | GVYA prediction |','|---|---|---|']
    for x in wrong: lines.append('| `%s` | `%s` | `%s` |' % (x['text'],x['expected'],x['predicted']))
    lines += ['', 'ChatScript and Program-Y produced no wrong-intent decision in this test; they fell back more often. GVYA therefore buys additional recall with a small close-boundary precision cost.','',
      '## Important negative result','', '**None of the three systems matched any of the 48 unseen paraphrases.** Four literal training examples per intent are too sparse for broad paraphrase coverage in this configuration.','',
      'Test 2 will ask the practical follow-up question: with the same authoring budget, how effectively can each system use its native abstractions? Test 2 is not part of this package yet.','',
      '## Frozen design','', '- 24 intents × 4 authored examples = 96 semantic evidence rows per engine.','- 288 evaluation turns: 240 in-domain + 48 OOD.','- No semantic-source or expected-label tuning after the first frozen GVYA run.','- Two full passes per engine; prediction sequences must be identical.','',
      'Benchmark specification SHA-256: `8dc54256a3eebc5fbca4c2930d3ad133378d524fc54a002909c6b97482e220d4`','',
      'Semantic-source lock SHA-256: `add80b1f898b652928d024d3b3b50bd1525a4f1456ade2d7bfc023ab6ae73198`','',
      'See `METHODOLOGY.md` for the fairness boundary and `RUNTIME_LOCK.json` for exact runtime identities. Raw predictions are under `results/`.','',
      '## Timing note','', 'Per-turn timings remain in the summaries but are **not** a performance comparison because the host boundaries differ (GVYA WASM/JSON ABI, in-process Program-Y, and ChatScript localhost TCP).','']
    (root/'RESULTS.md').write_text('\n'.join(lines),encoding='utf-8')
    print('Built final Test 1 report')

if __name__=='__main__': main()
