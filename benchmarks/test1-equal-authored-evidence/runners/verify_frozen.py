from __future__ import print_function
import argparse
import hashlib
import json
from pathlib import Path

EXPECTED_SPEC = "8dc54256a3eebc5fbca4c2930d3ad133378d524fc54a002909c6b97482e220d4"
EXPECTED_SEMANTIC_LOCK = "add80b1f898b652928d024d3b3b50bd1525a4f1456ade2d7bfc023ab6ae73198"


def sha(path):
    h = hashlib.sha256()
    with open(str(path), "rb") as f:
        while True:
            b = f.read(1024 * 1024)
            if not b:
                break
            h.update(b)
    return h.hexdigest()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", required=True)
    a = ap.parse_args()
    root = Path(a.root).resolve()
    spec = root / "frozen" / "benchmark-spec.json"
    lock_path = root / "frozen" / "SEMANTIC_SOURCE_LOCK.json"
    if sha(spec) != EXPECTED_SPEC:
        raise SystemExit("benchmark-spec.json hash mismatch")
    if sha(lock_path) != EXPECTED_SEMANTIC_LOCK:
        raise SystemExit("SEMANTIC_SOURCE_LOCK.json hash mismatch")
    lock = json.loads(lock_path.read_text(encoding="utf-8"))
    if lock["benchmark_spec_sha256"] != EXPECTED_SPEC:
        raise SystemExit("semantic lock references wrong benchmark spec")
    for row in lock["files"]:
        p = root / "frozen" / row["path"]
        if not p.is_file():
            raise SystemExit("locked file missing: %s" % row["path"])
        if p.stat().st_size != row["bytes"]:
            raise SystemExit("locked file size mismatch: %s" % row["path"])
        if sha(p) != row["sha256"]:
            raise SystemExit("locked file hash mismatch: %s" % row["path"])
    print("Frozen benchmark verified: spec=%s semantic_lock=%s files=%d" % (EXPECTED_SPEC, EXPECTED_SEMANTIC_LOCK, len(lock["files"])))

if __name__ == "__main__":
    main()
