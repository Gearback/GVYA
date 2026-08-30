#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
PASS: list[str] = []
FAIL: list[str] = []

MANIFEST_NAME = "SOURCE_MANIFEST.tsv"
INTEGRITY_NAME = "INTEGRITY.sha256"
GENERATED_DIRS = {".git", "node_modules", "dist", "web-dist", "target", "__pycache__"}
RECOVERY_DIRS = {"_backup"}
CANONICAL_DOCS = {
    "AI_FIRST_DEVELOPMENT.md",
    "GETTING_STARTED.md",
    "MACHINE_AUTHORING_ARCHITECTURE.md",
    "ARTIFACT_FORMAT.md",
    "CAPABILITY_ARCHITECTURE.md",
    "COMPILER_PIPELINE.md",
    "CONSTITUTION.md",
    "CONVERSATION_ARCHITECTURE.md",
    "DOMAIN_MODEL.md",
    "ENGINE_ASSETS.md",
    "PACKAGE_ARCHITECTURE.md",
    "PACKAGE_AUTHORING_RECIPE.md",
    "README.md",
    "RELEASE.md",
    "REPOSITORY_LAYOUT.md",
    "RUNTIME_ARCHITECTURE.md",
    "RUNTIME_WIRE_PROTOCOL.md",
    "SCHEMA_PROFILE.md",
    "SEMANTIC_ARCHITECTURE.md",
    "SOURCE_FORMAT.md",
    "STUDIO_ARCHITECTURE.md",
}
REQUIRED_VALIDATION = {
    "browser-acceptance.mjs",
    "human-authoring-contract.mjs",
    "gvya-help-bot-contract.mjs",
    "engine-v1-acceptance.mjs",
    "runtime-sdk-contract.mjs",
    "runtime-sdk-wasm.mjs",
    "godot-adapter-contract.mjs",
    "godot-web-wasm.mjs",
    "godot-export-contract.mjs",
    "studio-contract.mjs",
    "studio-engine-contract.mjs",
    "web-export-contract.mjs",
    "source-export-contract.mjs",
    # Node module hooks for Vite `?raw` imports, used by web-export-contract.mjs.
    "raw-import-hooks.mjs",
}
REQUIRED_TOOLING_VALIDATION = {"release-gate-contract.py"}
REQUIRED_BENCHMARK_TEST1 = {
    "README.md",
    "METHODOLOGY.md",
    "RESULTS.md",
    "RUNTIME_LOCK.json",
    "PROGRAMY_RUNTIME_INPUTS.json",
    "PROGRAMY_SDIST_SHA256SUMS.txt",
    "frozen/benchmark-spec.json",
    "frozen/BENCHMARK_SPEC_SHA256.txt",
    "frozen/SEMANTIC_SOURCE_LOCK.json",
    "results/gvya.raw.jsonl",
    "results/gvya.summary.json",
    "results/aiml2.raw.jsonl",
    "results/aiml2.summary.json",
    "results/chatscript.raw.jsonl",
    "results/chatscript.summary.json",
    "results/test1.summary.json",
    "runners/verify_frozen.py",
    "runners/verify_runtime_lock.py",
}

REQUIRED_BENCHMARK_TEST2 = {
    "README.md",
    "METHODOLOGY.md",
    "RESULTS.md",
    "frozen/problem-brief.json",
    "frozen/blind-corpus.json",
    "frozen/PRE_EVALUATION_FREEZE.json",
    "frozen/FAIRNESS_AUDIT.json",
    "frozen/RUNTIME_AND_RESULT_LOCK.json",
    "authoring/gvya/BUDGET_LEDGER.json",
    "authoring/aiml2/BUDGET_LEDGER.json",
    "authoring/chatscript/BUDGET_LEDGER.json",
    "results/gvya.raw.jsonl",
    "results/chatscript.raw.jsonl",
    "results/aiml2.raw.jsonl",
    "results/analysis.json",
    "runners/verify.py",
}


def check(ok: bool, label: str) -> None:
    (PASS if ok else FAIL).append(label)


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def is_manifest_source_file(path: Path) -> bool:
    rel = path.relative_to(ROOT)
    if not path.is_file():
        return False
    if any(part in GENERATED_DIRS or part in RECOVERY_DIRS for part in rel.parts):
        return False
    if path.name.endswith(".tsbuildinfo") or path.name == ".DS_Store":
        return False
    return rel.as_posix() not in {MANIFEST_NAME, INTEGRITY_NAME}


def current_source_files() -> list[Path]:
    return sorted(
        (path for path in ROOT.rglob("*") if is_manifest_source_file(path)),
        key=lambda path: path.relative_to(ROOT).as_posix(),
    )


def write_source_manifests() -> None:
    rows: list[tuple[str, int, str]] = []
    for path in current_source_files():
        rel = path.relative_to(ROOT).as_posix()
        rows.append((sha256(path), path.stat().st_size, rel))
    manifest = ROOT / MANIFEST_NAME
    manifest.write_text(
        "sha256\tbytes\tpath\n" + "".join(f"{digest}\t{size}\t{rel}\n" for digest, size, rel in rows),
        encoding="utf-8",
    )
    integrity_rows = [(sha256(manifest), MANIFEST_NAME), *[(digest, rel) for digest, _size, rel in rows]]
    integrity_rows.sort(key=lambda row: row[1])
    (ROOT / INTEGRITY_NAME).write_text(
        "".join(f"{digest}  {rel}\n" for digest, rel in integrity_rows),
        encoding="utf-8",
    )


def validate_source_manifests() -> tuple[bool, str]:
    manifest = ROOT / MANIFEST_NAME
    integrity = ROOT / INTEGRITY_NAME
    if not manifest.is_file() or not integrity.is_file():
        return False, "source manifest/integrity files are present"
    try:
        lines = manifest.read_text(encoding="utf-8").splitlines()
        if not lines or lines[0] != "sha256\tbytes\tpath":
            return False, "source manifest header is canonical"
        manifest_rows: dict[str, tuple[str, int]] = {}
        ordered_paths: list[str] = []
        for line in lines[1:]:
            parts = line.split("\t")
            if len(parts) != 3:
                return False, "source manifest rows are structurally valid"
            digest, size_text, rel = parts
            if (
                not re.fullmatch(r"[0-9a-f]{64}", digest)
                or not size_text.isdigit()
                or not rel
                or rel.startswith("/")
                or "\\" in rel
            ):
                return False, "source manifest rows are structurally valid"
            if rel in manifest_rows:
                return False, "source manifest paths are unique"
            manifest_rows[rel] = (digest, int(size_text))
            ordered_paths.append(rel)
        if ordered_paths != sorted(ordered_paths):
            return False, "source manifest paths are sorted deterministically"

        current = {path.relative_to(ROOT).as_posix(): path for path in current_source_files()}
        if set(manifest_rows) != set(current):
            missing = sorted(set(manifest_rows) - set(current))[:3]
            extra = sorted(set(current) - set(manifest_rows))[:3]
            return False, f"source manifest membership matches checkout (missing={missing}, extra={extra})"
        for rel, path in current.items():
            expected_hash, expected_size = manifest_rows[rel]
            if path.stat().st_size != expected_size or sha256(path) != expected_hash:
                return False, f"source manifest hash/size matches checkout: {rel}"

        integrity_rows: dict[str, str] = {}
        integrity_order: list[str] = []
        for line in integrity.read_text(encoding="utf-8").splitlines():
            match = re.fullmatch(r"([0-9a-f]{64})  (.+)", line)
            if not match:
                return False, "integrity rows are structurally valid"
            digest, rel = match.groups()
            if rel in integrity_rows:
                return False, "integrity paths are unique"
            integrity_rows[rel] = digest
            integrity_order.append(rel)
        if set(integrity_rows) != set(manifest_rows) | {MANIFEST_NAME}:
            return False, "integrity membership matches source manifest plus manifest root"
        if integrity_order != sorted(integrity_order):
            return False, "integrity paths are sorted deterministically"
        if integrity_rows.get(MANIFEST_NAME) != sha256(manifest):
            return False, "integrity pins exact source manifest"
        for rel, (digest, _size) in manifest_rows.items():
            if integrity_rows.get(rel) != digest:
                return False, f"integrity agrees with source manifest: {rel}"
    except (OSError, UnicodeError, ValueError):
        return False, "source manifest/integrity files are readable and valid"
    return True, "source manifest membership, sizes, hashes, and integrity root are exact"


if len(sys.argv) == 2 and sys.argv[1] == "--write-manifests":
    write_source_manifests()
    print(f"wrote {MANIFEST_NAME} and {INTEGRITY_NAME}")
    raise SystemExit(0)
if len(sys.argv) != 1:
    print("usage: validate-source.py [--write-manifests]", file=sys.stderr)
    raise SystemExit(2)

# Package identity and clean source policy.
required_root = [
    "Cargo.toml", "rust-toolchain.toml", "package.json", ".npmrc", "README.md", "TODO.md",
    "LICENSE", "CONTRIBUTING.md", "SECURITY.md",
    MANIFEST_NAME, INTEGRITY_NAME, "tools/release_gate.py", "include/gvya.h",
]
check(all((ROOT / rel).is_file() for rel in required_root), "required root source and policy files are present")
check(
    not [path for path in ROOT.rglob("*.zip") if not any(part in RECOVERY_DIRS for part in path.relative_to(ROOT).parts)],
    "source tree contains no embedded ZIP archives",
)
engine_v1_dir = ROOT / "apps" / "studio" / "public" / "engine" / "v1"
engine_v1_files = (
    {path.name for path in engine_v1_dir.iterdir() if path.is_file()}
    if engine_v1_dir.is_dir()
    else set()
)
check(
    engine_v1_files in (set(), {"manifest.json", "gvya-ffi.wasm"}),
    "Engine v1 assets are either absent for the explicit pre-WASM source state or present as one complete canonical set",
)

# Documentation is current-state only: one canonical document per concern, no history/report tree.
doc_files = {path.name for path in (ROOT / "docs").iterdir() if path.is_file()}
doc_dirs = {path.name for path in (ROOT / "docs").iterdir() if path.is_dir()}
check(doc_files == CANONICAL_DOCS and not doc_dirs, "documentation layout is canonical and has no parallel history tree")
forbidden_name = re.compile(r"(?:report|changelog|history|handoff|release[_-]?notes|readiness|hardening|provenance|conformance|adr)", re.I)
forbidden_docs = [
    path.relative_to(ROOT).as_posix()
    for path in ROOT.rglob("*.md")
    if not any(part in RECOVERY_DIRS for part in path.relative_to(ROOT).parts)
    and path.relative_to(ROOT).as_posix() not in {"README.md", "TODO.md"}
    and any(part not in {"reference"} for part in path.relative_to(ROOT).parts)
    and forbidden_name.search(path.name)
]
check(not forbidden_docs, f"report/history-style markdown is absent from current source ({forbidden_docs[:3]})")

# Backticked local Markdown references must resolve; this catches stale doc names after consolidation.
missing_md_refs: list[str] = []
md_ref = re.compile(r"`([^`\n]+\.md)`")
for path in [ROOT / "README.md", ROOT / "TODO.md", *sorted((ROOT / "docs").glob("*.md")), ROOT / "apps/studio/README.md", ROOT / "packages/runtime-sdk/README.md", ROOT / "adapters/godot/README.md"]:
    if not path.is_file():
        continue
    for raw in md_ref.findall(path.read_text(encoding="utf-8")):
        if raw.startswith("/"):
            target = ROOT / raw.lstrip("/")
        else:
            target = (path.parent / raw).resolve()
        try:
            target.relative_to(ROOT.resolve())
        except ValueError:
            missing_md_refs.append(f"{path.relative_to(ROOT)} -> {raw} (outside root)")
            continue
        if not target.is_file():
            missing_md_refs.append(f"{path.relative_to(ROOT)} -> {raw}")
check(not missing_md_refs, f"local Markdown references resolve ({missing_md_refs[:3]})")

# Public benchmark evidence is part of the GitHub Edition source closure.
benchmark_test1 = ROOT / "benchmarks" / "test1-equal-authored-evidence"
benchmark_missing = sorted(
    rel for rel in REQUIRED_BENCHMARK_TEST1 if not (benchmark_test1 / rel).is_file()
)
check(
    benchmark_test1.is_dir() and not benchmark_missing,
    f"public Test 1 benchmark evidence is complete ({benchmark_missing[:3]})",
)

benchmark_test2 = ROOT / "benchmarks" / "test2-equal-authoring-budget"
benchmark2_missing = sorted(
    rel for rel in REQUIRED_BENCHMARK_TEST2 if not (benchmark_test2 / rel).is_file()
)
check(
    benchmark_test2.is_dir() and not benchmark2_missing,
    f"public Test 2 benchmark evidence is complete ({benchmark2_missing[:3]})",
)

# TODO/FIXME/TBD tasks must not be scattered through source/docs.
marker = re.compile(r"(?im)(?:^|\s)(?:TODO|FIXME|TBD|XXX)\s*:")
scattered_markers: list[str] = []
text_suffixes = {".md", ".rs", ".ts", ".tsx", ".js", ".mjs", ".py", ".gd", ".json", ".toml", ".h", ".css"}
for path in current_source_files():
    rel = path.relative_to(ROOT).as_posix()
    if rel == "TODO.md" or path.suffix.lower() not in text_suffixes:
        continue
    try:
        if marker.search(path.read_text(encoding="utf-8")):
            scattered_markers.append(rel)
    except UnicodeDecodeError:
        pass
check(not scattered_markers, f"unfinished task markers exist only in root TODO.md ({scattered_markers[:3]})")

# Generated/build/history debris is never part of a clean source package.
stray: list[str] = []
for path in ROOT.rglob("*"):
    if not path.is_file():
        continue
    rel = path.relative_to(ROOT)
    if any(part in RECOVERY_DIRS for part in rel.parts) or ".git" in rel.parts:
        continue
    if any(part in GENERATED_DIRS for part in rel.parts) or path.name.endswith(".tsbuildinfo") or path.name == ".DS_Store":
        stray.append(rel.as_posix())
check(not stray, f"generated/cache outputs are absent ({stray[:3]})")
check(not list((ROOT / "validation").glob("*.log")), "generated validation logs are absent")
check(not list((ROOT / "tools").glob("validate-step*")), "historical step validators are absent")

# Validation inventory: executable behavioral suites only; source hygiene lives in this validator.
validation_scripts = {path.name for path in (ROOT / "validation").glob("*.mjs")}
check(validation_scripts == REQUIRED_VALIDATION, "validation script inventory contains only current executable suites")
tooling_validation_scripts = {path.name for path in (ROOT / "validation").glob("*.py")}
check(
    tooling_validation_scripts == REQUIRED_TOOLING_VALIDATION,
    "validation tooling inventory contains only current executable suites",
)
fixture_dir = ROOT / "validation" / "fixtures"
check(fixture_dir.is_dir() and any(fixture_dir.iterdir()), "runtime/source fixtures are present")
authoring_e2e_root = ROOT / "validation" / "authoring-e2e"
authoring_e2e_root_files = {
    path.name for path in authoring_e2e_root.iterdir() if path.is_file()
} if authoring_e2e_root.is_dir() else set()
check(
    authoring_e2e_root_files == {"README.md", "run.mjs"},
    "authoring E2E root contains only the canonical runner and README",
)

# Root command registration only. Behavioral semantics belong to executable tests, not source-text greps here.
try:
    root_pkg = json.loads((ROOT / "package.json").read_text(encoding="utf-8"))
except (OSError, json.JSONDecodeError):
    root_pkg = {}
scripts = root_pkg.get("scripts", {}) if isinstance(root_pkg, dict) else {}
node_engine = root_pkg.get("engines", {}).get("node")
check(isinstance(node_engine, str) and re.fullmatch(r">=\d+", node_engine.strip()) is not None, "canonical Node minimum engine contract is registered")
check(root_pkg.get("version") == "0.0.0-dev", "root development version is normalized")
check(root_pkg.get("license") == "Apache-2.0", "root npm package metadata declares Apache-2.0")
try:
    cargo_root = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
except OSError:
    cargo_root = ""
check(re.search(r'(?m)^license\s*=\s*"Apache-2\.0"\s*$', cargo_root) is not None, "Rust workspace metadata declares Apache-2.0")
workspace_license_ok = True
for package_json in [ROOT / "apps" / "studio" / "package.json", ROOT / "packages" / "runtime-sdk" / "package.json"]:
    try:
        package_data = json.loads(package_json.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        workspace_license_ok = False
        continue
    workspace_license_ok = workspace_license_ok and package_data.get("license") == "Apache-2.0"
check(workspace_license_ok, "JavaScript workspace package metadata declares Apache-2.0")
required_scripts = {
    "check", "test:studio", "build:studio", "clean", "test:source", "validate:source", "test:sdk", "test:godot",
    "test:authoring-e2e", "test:gvya-help",
    "bootstrap:locks", "certify:preflight", "test:browser", "certify:core", "certify:release", "refresh:manifests",
    "test:release-tooling", "package:release", "certify:archive",
    "engine:build:v1", "engine:verify:v1", "engine:accept:v1",
    "benchmark:test1:verify", "benchmark:test1:gvya", "benchmark:test2:verify",
}
check(required_scripts.issubset(scripts), "current build/test/certification commands are registered")
check(scripts.get("test:authoring-e2e") == "node validation/authoring-e2e/run.mjs --source-proof", "authoring end-to-end source/runtime proof command is canonical")
source_test = scripts.get("test:source", "")
check("npm run test:authoring-e2e" in source_test, "source test gate includes authoring E2E source/runtime proof")
check("npm run test:gvya-help" not in source_test, "full GVYA Help suite stays out of the bounded source edit loop")
check(scripts.get("validate:source") == "node tools/python.mjs tools/validate-source.py", "source validator command is canonical and cross-platform")
check(scripts.get("refresh:manifests") == "node tools/python.mjs tools/validate-source.py --write-manifests", "manifest refresh command is canonical and cross-platform")
check(scripts.get("certify:release") == "node tools/python.mjs tools/release_gate.py certify --browser --security", "release certification command is fail-closed")
check(scripts.get("test:release-tooling") == "node tools/python.mjs validation/release-gate-contract.py", "release-tooling regression command is canonical and cross-platform")
check(scripts.get("package:release") == "node tools/python.mjs tools/release_gate.py package", "release packaging command is canonical and cross-platform")
check(scripts.get("certify:archive") == "node tools/python.mjs tools/release_gate.py certify-archive --browser --security", "fresh-archive certification command is fail-closed")
check(
    scripts.get("benchmark:test1:verify") == "node tools/python.mjs benchmarks/test1-equal-authored-evidence/runners/verify_frozen.py --root benchmarks/test1-equal-authored-evidence && node tools/python.mjs benchmarks/test1-equal-authored-evidence/runners/verify_runtime_lock.py --root benchmarks/test1-equal-authored-evidence --repo-root .",
    "Test 1 stored-evidence verification command is canonical",
)
check(
    scripts.get("benchmark:test1:gvya") == "node benchmarks/test1-equal-authored-evidence/runners/run_gvya.mjs --root benchmarks/test1-equal-authored-evidence --repo-root .",
    "Test 1 GVYA replay command is canonical",
)
check(
    scripts.get("benchmark:test2:verify") == "node tools/python.mjs benchmarks/test2-equal-authoring-budget/runners/verify.py",
    "Test 2 frozen-evidence verification command is canonical",
)

manifest_ok, manifest_label = validate_source_manifests()
check(manifest_ok, manifest_label)

for label in PASS:
    print(f"PASS: {label}")
for label in FAIL:
    print(f"FAIL: {label}")
print(f"\n{len(PASS)} passed, {len(FAIL)} failed")
raise SystemExit(1 if FAIL else 0)
