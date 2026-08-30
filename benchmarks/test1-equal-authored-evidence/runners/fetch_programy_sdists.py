from __future__ import print_function
import argparse
import hashlib
import json
from pathlib import Path
try:
    from urllib.request import urlopen
except ImportError:
    from urllib2 import urlopen


def sha256(path):
    h=hashlib.sha256()
    with open(str(path),'rb') as f:
        while True:
            b=f.read(1024*1024)
            if not b: break
            h.update(b)
    return h.hexdigest()


def main():
    ap=argparse.ArgumentParser()
    ap.add_argument('--root',required=True)
    ap.add_argument('--output',required=True)
    a=ap.parse_args()
    root=Path(a.root).resolve(); out=Path(a.output).resolve(); out.mkdir(parents=True,exist_ok=True)
    spec=json.loads((root/'PROGRAMY_RUNTIME_INPUTS.json').read_text(encoding='utf-8'))
    for item in spec['packages']:
        api='https://pypi.org/pypi/%s/%s/json' % (item['project'],item['version'])
        meta=json.loads(urlopen(api,timeout=30).read().decode('utf-8'))
        matches=[x for x in meta.get('urls',[]) if x.get('filename')==item['filename']]
        if len(matches)!=1:
            raise SystemExit('Expected one PyPI file for %s, found %d' % (item['filename'],len(matches)))
        target=out/item['filename']
        data=urlopen(matches[0]['url'],timeout=60).read()
        target.write_bytes(data)
        actual=sha256(target)
        if actual!=item['sha256']:
            raise SystemExit('SHA-256 mismatch for %s: %s' % (item['filename'],actual))
        print('%s  %s' % (actual,item['filename']))

if __name__=='__main__': main()
