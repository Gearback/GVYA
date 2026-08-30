from __future__ import annotations
import hashlib,json,pathlib,re,xml.etree.ElementTree as ET
R=pathlib.Path(__file__).resolve().parents[1]
# frozen blind
expected=(R/'frozen/BLIND_CORPUS_SHA256.txt').read_text().split()[0]
assert hashlib.sha256((R/'frozen/blind-corpus.json').read_bytes()).hexdigest()==expected
# semantic pre-evaluation freeze
freeze=json.loads((R/'frozen/PRE_EVALUATION_FREEZE.json').read_text())
for row in freeze['semantic_files']:
 p=R/row['path']; assert p.is_file(),row['path']; assert hashlib.sha256(p.read_bytes()).hexdigest()==row['sha256'],row['path']
# source-derived budget audit
for sys in ['gvya','aiml2','chatscript']:
 led=json.loads((R/f'authoring/{sys}/BUDGET_LEDGER.json').read_text()); assert led['rows']==115 and len(led['entries'])==115
# GVYA actual fields
gd=gc=0
for p in (R/'authoring/gvya/source/packages/base/fragments/meanings').glob('*.json'):
 v=json.loads(p.read_text())['value']; n=sum(len(v.get(k,[])) for k in ('samples','negative_samples','retrieval_terms','patterns'))
 if v['id'].startswith('context.'): gc+=n
 else: gd+=n
assert (gd,gc)==(100,15)
# AIML actual categories
txt=(R/'authoring/aiml2/source/benchmark.aiml').read_text(); root=ET.fromstring(txt); budget=context=fallback=0
for c in root.findall('category'):
 p=' '.join((c.findtext('pattern') or '').split()); t=(c.findtext('template') or '').strip(); that=c.find('that')
 if p=='*' and t=='BENCH:FALLBACK' and that is None: fallback+=1
 else: budget+=1; context+= int(that is not None)
assert (budget,context,fallback)==(115,15,1); assert '<set>' not in txt and '<map>' not in txt
# ChatScript actual members/rules/rejoinders
lines=(R/'authoring/chatscript/source/benchmark.top').read_text().splitlines(); members=direct=rejo=0; rule_lines=[]
for line in lines:
 s=line.strip()
 if s.startswith('concept:'):
  m=re.search(r'\((.*)\)\s*$',s); assert m; members+=len(m.group(1).split())
 elif line.startswith('u: '):
  rule_lines.append(line)
  if 'BENCH:FALLBACK' not in line: direct+=1
 elif line.startswith('    a: '): rejo+=1; rule_lines.append(line)
assert (members,direct,rejo)==(40,60,15); assert '[' not in '\n'.join(rule_lines) and '|' not in '\n'.join(rule_lines)
# result prediction hashes and deterministic replay
lock=json.loads((R/'frozen/RUNTIME_AND_RESULT_LOCK.json').read_text())
for sys in ['gvya','chatscript','aiml2']:
 def load(name): return [json.loads(x) for x in (R/'results'/name).read_text().splitlines() if x.strip()]
 raw=load(f'{sys}.raw.jsonl'); replay=load(f'{sys}.replay.jsonl')
 seq=lambda rows:'\n'.join(f"{x.get('setup_predicted')}\t{x.get('predicted')}" for x in rows)+'\n'
 assert seq(raw)==seq(replay),sys+' replay'
 assert hashlib.sha256(seq(raw).encode()).hexdigest()==lock['prediction_sha256'][sys],sys+' prediction hash'
# reported summary recompute basic primary counts
spec={x['id']:x for x in json.loads((R/'frozen/blind-corpus.json').read_text())['evaluation']}
expected_summary={'gvya':(43,24,67),'chatscript':(26,28,54),'aiml2':(16,24,40)}
for sys,vals in expected_summary.items():
 rows=[json.loads(x) for x in (R/'results'/f'{sys}.raw.jsonl').read_text().splitlines() if x.strip()]
 def ok(x): return (not x.get('setup') or x.get('setup_predicted')==x.get('setup_expected')) and x.get('predicted')==x.get('expected')
 pos=sum(ok(x) for x in rows if x['expected'] is not None); neg=sum(ok(x) for x in rows if x['expected'] is None)
 assert (pos,neg,pos+neg)==vals,(sys,pos,neg)
print('Test 2 frozen evidence: PASS')
print('budget: GVYA 115 / ChatScript 115 / AIML 115')
print('positive: GVYA 43/120, ChatScript 26/120, AIML 16/120')
print('negative safety: GVYA 24/30, ChatScript 28/30, AIML 24/30')
