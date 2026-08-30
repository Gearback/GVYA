#!/usr/bin/env python3
"""Build a runtime-action artifact with a deterministic dummy signature envelope.

The signature bytes are intentionally not cryptographic. The fixture only proves that Runtime calls
host trust policy *after* structural/content-root validation and reports trust status correctly.
"""
from __future__ import annotations
import hashlib, pathlib, sys
sys.path.insert(0, str(pathlib.Path(__file__).parent))
from gvya_container_reference import Entry, build, canonical, content_root, parse

root = pathlib.Path(__file__).resolve().parents[1]
source = root / "validation/fixtures/runtime-action.gvya"
rows = parse(source.read_bytes())
root_digest = content_root(rows).hex()
entries = [Entry(kind, path, essential, body) for kind, path, essential, _, body in rows]
signature = canonical({
    "format": "gvya.signature",
    "version": 1,
    "content_root": root_digest,
    "algorithm": "fixture-v1",
    "key_id": "fixture-key",
    "signature": "fixture-signature",
})
entries.append(Entry(5, "signature.json", False, signature))
artifact = build(entries)
out = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else root / "validation/fixtures/runtime-signed.gvya")
out.parent.mkdir(parents=True, exist_ok=True)
out.write_bytes(artifact)
print(f"{out} sha256={hashlib.sha256(artifact).hexdigest()} content_root={root_digest} bytes={len(artifact)}")
