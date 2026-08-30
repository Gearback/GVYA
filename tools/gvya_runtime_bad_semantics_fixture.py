#!/usr/bin/env python3
"""Build a structurally valid `.gvya` whose executable semantic data cannot form a matcher index.

The container, manifest, integrity rows and payload digests are all valid. The program authors a
Meaning sample in a language the program carries no Semantic Profile for, so the runtime cannot
build the derived matcher index from it. A conforming runtime must fail closed during compiled-IR
hydration rather than dropping the unmatched language or matching without it.
"""
from __future__ import annotations
import hashlib, json, pathlib, sys
sys.path.insert(0, str(pathlib.Path(__file__).parent))
from gvya_container_reference import Entry, build, canonical, parse

root = pathlib.Path(__file__).resolve().parents[1]
source = root / "validation/fixtures/runtime-action.gvya"
rows = parse(source.read_bytes())
by_path = {path: body for _, path, _, _, body in rows}
program = json.loads(by_path["program.json"])
# Deliberate contradiction: the only enabled language is `en`, but the Meaning is sampled in `fa`.
assert len(program["semantic"]["patterns"]) == 1
assert list(program["semantic"]["profiles"]) == ["en"]
program["semantic"]["patterns"][0]["samples"] = [{"language": "fa", "text": "سلام"}]
program_bytes = canonical(program)
program_sha = hashlib.sha256(program_bytes).hexdigest()

integrity = json.loads(by_path["integrity.json"])
integrity["program"]["sha256"] = program_sha
integrity["program"]["size"] = len(program_bytes)
integrity_bytes = canonical(integrity)

manifest = json.loads(by_path["manifest.json"])
manifest["program"]["sha256"] = program_sha
manifest["program"]["size"] = len(program_bytes)
manifest["integrity"]["sha256"] = hashlib.sha256(integrity_bytes).hexdigest()
manifest_bytes = canonical(manifest)

entries = [
    Entry(1, "manifest.json", True, manifest_bytes),
    Entry(2, "program.json", True, program_bytes),
    Entry(6, "integrity.json", True, integrity_bytes),
]
for kind, path, essential, _, body in rows:
    if path.startswith("assets/"):
        entries.append(Entry(kind, path, essential, body))
artifact = build(entries)
out = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else root / "validation/fixtures/runtime-bad-semantics.gvya")
out.parent.mkdir(parents=True, exist_ok=True)
out.write_bytes(artifact)
print(f"{out} sha256={hashlib.sha256(artifact).hexdigest()} bytes={len(artifact)}")
