#!/usr/bin/env python3
"""Independent compiler/artifact reference encoder/parser for the documented GVYA container v1.

This is a specification/validation oracle, not a second production compiler. The production
compiler remains Rust. Keeping the tiny container codec independently executable lets packaging
environments without Rust still byte-test determinism, bounds, table ordering and tamper failure.
"""
from __future__ import annotations
import hashlib, json, struct
from dataclasses import dataclass

MAGIC=b"GVYA\r\n\x1a\n"
VERSION=1
HEADER_LEN=24
FIXED=52
KINDS={"manifest":1,"program":2,"asset":3,"debug":4,"signature":5,"integrity":6}

@dataclass(frozen=True)
class Entry:
    kind:int
    path:str
    essential:bool
    data:bytes

def safe_path(path:str)->bool:
    if not path or path.startswith('/') or path.endswith('/') or '\\' in path or '\x00' in path: return False
    parts=path.split('/')
    if any(not p or p in ('.','..') for p in parts): return False
    return not any(ord(ch)<0x20 or ord(ch)==0x7f for ch in path)

def sha(data:bytes)->bytes:return hashlib.sha256(data).digest()

def valid_convention(e:Entry)->bool:
    if e.kind==1:return e.path=='manifest.json' and e.essential
    if e.kind==2:return e.path=='program.json' and e.essential
    if e.kind==6:return e.path=='integrity.json' and e.essential
    if e.kind==3:return e.path.startswith('assets/') and e.essential
    if e.kind==4:return e.path=='debug/source-map.json' and not e.essential
    if e.kind==5:return e.path=='signature.json' and not e.essential
    return False
def canonical(obj)->bytes:return json.dumps(obj,sort_keys=True,separators=(',',':'),ensure_ascii=False).encode()

def build(entries:list[Entry])->bytes:
    entries=sorted(entries,key=lambda e:e.path)
    if len({e.path for e in entries})!=len(entries):raise ValueError('duplicate')
    for e in entries:
        if not safe_path(e.path):raise ValueError('path')
        if not valid_convention(e):raise ValueError('convention')
        if len(e.path.encode())>512 or len(e.data)>64*1024*1024:raise ValueError('bound')
    for path,kind in [('manifest.json',1),('program.json',2),('integrity.json',6)]:
        rows=[e for e in entries if e.path==path]
        if len(rows)!=1 or rows[0].kind!=kind or not rows[0].essential:raise ValueError('required')
    table_len=sum(FIXED+len(e.path.encode()) for e in entries)
    offset=HEADER_LEN+table_len
    table=bytearray(); payload=bytearray()
    for e in entries:
        p=e.path.encode(); table += struct.pack('<BBHQQ',e.kind,1 if e.essential else 0,len(p),offset,len(e.data)); table += sha(e.data); table += p
        payload += e.data; offset += len(e.data)
    out=MAGIC+struct.pack('<HHIQ',VERSION,0,len(entries),table_len)+bytes(table)+bytes(payload)
    if len(out)>512*1024*1024:raise ValueError('total')
    return out

def parse(data:bytes):
    if len(data)<HEADER_LEN or data[:8]!=MAGIC:raise ValueError('magic')
    version,flags,count,table_len=struct.unpack_from('<HHIQ',data,8)
    if version!=1 or flags!=0:raise ValueError('header')
    end=HEADER_LEN+table_len
    if end>len(data):raise ValueError('truncated')
    cur=HEADER_LEN; rows=[]; last=None;expected=end
    for _ in range(count):
        if cur+FIXED>end:raise ValueError('truncated')
        kind,essential,plen,off,length=struct.unpack_from('<BBHQQ',data,cur); digest=data[cur+20:cur+52]; cur+=FIXED
        pbytes=data[cur:cur+plen]; cur+=plen
        path=pbytes.decode('utf8')
        if not safe_path(path) or essential not in (0,1):raise ValueError('path/flag')
        if not valid_convention(Entry(kind,path,bool(essential),b'')):raise ValueError('convention')
        if last is not None and last>=path:raise ValueError('order')
        last=path
        if off<end or off+length>len(data):raise ValueError('offset')
        if off!=expected:raise ValueError('layout')
        expected=off+length
        body=data[off:off+length]
        if sha(body)!=digest:raise ValueError('digest')
        rows.append((kind,path,bool(essential),digest,body))
    if cur!=end:raise ValueError('table-size')
    if expected!=len(data):raise ValueError('layout')
    return rows

def content_root(rows):
    buf=bytearray()
    for kind,path,essential,digest,_ in sorted((r for r in rows if r[2] and r[0]!=5),key=lambda r:r[1]):
        p=path.encode();buf+=struct.pack('<BI',kind,len(p))+p+digest
    return sha(bytes(buf))

def golden_entries():
    program=canonical({'format':'gvya.program','version':1,'project_id':'golden','brain_id':'golden','enabled_languages':['en'],'default_language':'en','packages':[]})
    asset=b'GVYA\x00golden\n'
    integrity=canonical({'format':'gvya.integrity','version':1,'program':{'path':'program.json','sha256':hashlib.sha256(program).hexdigest(),'size':len(program)},'assets':[{'path':'assets/golden.bin','sha256':hashlib.sha256(asset).hexdigest(),'size':len(asset)}],'source_packages':[]})
    manifest=canonical({'format':'gvya.artifact','version':1,'container_version':1,'project_id':'golden','brain_id':'golden','program':{'path':'program.json','format':'gvya.program','version':1,'sha256':hashlib.sha256(program).hexdigest(),'size':len(program)},'integrity':{'path':'integrity.json','sha256':hashlib.sha256(integrity).hexdigest()}})
    return [Entry(1,'manifest.json',True,manifest),Entry(2,'program.json',True,program),Entry(6,'integrity.json',True,integrity),Entry(3,'assets/golden.bin',True,asset)]

if __name__=='__main__':
    import argparse, pathlib
    ap=argparse.ArgumentParser();ap.add_argument('--write-golden');args=ap.parse_args()
    a=build(golden_entries());b=build(list(reversed(golden_entries())))
    assert a==b
    rows=parse(a);root=content_root(rows)
    tampered=bytearray(a);tampered[-1]^=1
    try:parse(bytes(tampered));raise AssertionError('tamper accepted')
    except ValueError as e:assert str(e)=='digest'
    try:build(golden_entries()+[Entry(3,'assets/../bad',True,b'x')]);raise AssertionError('traversal accepted')
    except ValueError as e:assert str(e)=='path'
    try:build(golden_entries()+[Entry(4,'debug/source-map.json',True,b'{}')]);raise AssertionError('bad essential flag accepted')
    except ValueError as e:assert str(e)=='convention'
    shifted=bytearray(a);off=struct.unpack_from('<Q',shifted,HEADER_LEN+4)[0];struct.pack_into('<Q',shifted,HEADER_LEN+4,off+1)
    try:parse(bytes(shifted));raise AssertionError('noncanonical payload layout accepted')
    except ValueError as e:assert str(e)=='layout'
    if args.write_golden:
        p=pathlib.Path(args.write_golden);p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(a)
        print(f'{p} sha256={hashlib.sha256(a).hexdigest()} content_root={root.hex()} bytes={len(a)}')
    else: print(f'PASS sha256={hashlib.sha256(a).hexdigest()} content_root={root.hex()} bytes={len(a)}')
