from __future__ import print_function
import argparse
import hashlib
import json
from pathlib import Path

EXPECTED = {
    'wasm': '4880e5b622171bdbf63dd28249ce553b2d7f720e01cfcd2b723f0c4763846f78',
    'manifest': 'f8d50e74422b070c2661f0217b803f333cfa0b30017773c291bcd7c097e0897f',
    'gvya': '3df5ed4c94e94efa83108b925b8ec213028a2ac11db909b9595e93155d4dcefd',
    'aiml2': 'dcc0b6cf2732ae9501e1f27b40106a156179f24a221e6cab7b3789da168c0f8b',
    'chatscript': 'ac9d2d45d091dc2368266ab287bfd62bc09ff53383b87d4e9e655e495d1ef80e',
}
PROGRAMY_COMMIT='6c9a0d34bac9b0e83da8e3674b9d7ee28af5db6d'
CHATSCRIPT_COMMIT='9f5eec4736ba22bd992a6498c1e0052e2a795125'
CHATSCRIPT_BLOB='5c9df404a088c3675a26d4beef84f09e6ea9f20a'


def sha_file(path):
    h=hashlib.sha256()
    with open(str(path),'rb') as f:
        while True:
            b=f.read(1024*1024)
            if not b: break
            h.update(b)
    return h.hexdigest()


def prediction_digest(path):
    rows=[json.loads(x) for x in path.read_text(encoding='utf-8').splitlines() if x.strip()]
    canonical=''.join('%s\t%s\n' % (r['id'], r.get('predicted') if r.get('predicted') is not None else '<NULL>') for r in rows)
    return hashlib.sha256(canonical.encode('utf-8')).hexdigest()


def main():
    ap=argparse.ArgumentParser(); ap.add_argument('--root',required=True); ap.add_argument('--repo-root')
    a=ap.parse_args(); root=Path(a.root).resolve(); repo=Path(a.repo_root).resolve() if a.repo_root else root.parent.parent
    lock=json.loads((root/'RUNTIME_LOCK.json').read_text(encoding='utf-8'))
    wasm=repo/'apps/studio/public/engine/v1/gvya-ffi.wasm'; manifest=repo/'apps/studio/public/engine/v1/manifest.json'
    if sha_file(wasm)!=EXPECTED['wasm']: raise SystemExit('GVYA WASM hash mismatch')
    if sha_file(manifest)!=EXPECTED['manifest']: raise SystemExit('GVYA Engine manifest hash mismatch')
    if lock['systems']['aiml2']['git_commit']!=PROGRAMY_COMMIT: raise SystemExit('Program-Y commit pin mismatch')
    if lock['systems']['chatscript']['git_commit']!=CHATSCRIPT_COMMIT: raise SystemExit('ChatScript commit pin mismatch')
    if lock['systems']['chatscript']['linux_binary_git_blob_sha']!=CHATSCRIPT_BLOB: raise SystemExit('ChatScript binary blob pin mismatch')
    for key,file in [('gvya','gvya.raw.jsonl'),('aiml2','aiml2.raw.jsonl'),('chatscript','chatscript.raw.jsonl')]:
        actual=prediction_digest(root/'results'/file)
        if actual!=EXPECTED[key]: raise SystemExit('%s prediction lock mismatch: %s' % (key,actual))
        if lock['systems'][key]['prediction_lock_sha256']!=EXPECTED[key]: raise SystemExit('%s RUNTIME_LOCK digest mismatch' % key)
    print('Runtime lock verified: Engine + Program-Y + ChatScript pins and all three prediction sequences')

if __name__=='__main__': main()
