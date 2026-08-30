#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import tomllib
import zipfile
from pathlib import Path
from pathlib import PurePosixPath

ROOT = Path(__file__).resolve().parents[1]
MANIFEST_NAME = "SOURCE_MANIFEST.tsv"
INTEGRITY_NAME = "INTEGRITY.sha256"
ARCHIVE_PREFIX = "gvya"
FIXED_ZIP_DATE = (1980, 1, 1, 0, 0, 0)
MAX_ARCHIVE_BYTES = 512 * 1024 * 1024
WINDOWS_SAFE_PATH_LIMIT = 240


class GateError(RuntimeError):
    pass


def toolchain_contract() -> tuple[str, int, str]:
    try:
        rust = tomllib.loads((ROOT / "rust-toolchain.toml").read_text(encoding="utf-8"))["toolchain"]
        rust_version = str(rust["channel"])
        targets = rust.get("targets", [])
        if not isinstance(targets, list) or len(targets) != 1 or not isinstance(targets[0], str):
            raise GateError("rust-toolchain.toml must declare exactly one release WASM target")
        node_engine = json.loads((ROOT / "package.json").read_text(encoding="utf-8"))["engines"]["node"]
    except (OSError, KeyError, TypeError, tomllib.TOMLDecodeError, json.JSONDecodeError) as exc:
        raise GateError("could not read canonical toolchain contract from rust-toolchain.toml/package.json") from exc
    match = re.fullmatch(r">=(\d+)", str(node_engine).strip())
    if not match:
        raise GateError(f"unsupported Node engine contract for release gate: {node_engine!r}; expected >=MAJOR")
    return rust_version, int(match.group(1)), targets[0]


def resolve_argv(argv: list[str]) -> list[str]:
    # Windows launcher shims such as npm.CMD are not executable by bare name through
    # subprocess, so every command is resolved to a concrete path before execution.
    # This keeps the gate identical on POSIX and Windows without shell interpolation.
    if not argv:
        raise GateError("empty command")
    executable = argv[0]
    if os.path.isabs(executable):
        return argv
    resolved = shutil.which(executable)
    if not resolved:
        raise GateError(f"missing required command: {executable}")
    return [resolved, *argv[1:]]


def run(
    argv: list[str], *, env: dict[str, str] | None = None, cwd: Path = ROOT
) -> None:
    print("$", " ".join(argv), flush=True)
    merged = os.environ.copy()
    if env:
        merged.update(env)
    subprocess.run(resolve_argv(argv), cwd=cwd, env=merged, check=True)


def output(argv: list[str], *, cwd: Path = ROOT) -> str:
    try:
        return subprocess.check_output(
            resolve_argv(argv), cwd=cwd, text=True, stderr=subprocess.STDOUT
        ).strip()
    except (FileNotFoundError, subprocess.CalledProcessError) as exc:
        raise GateError(f"required command failed: {' '.join(argv)}") from exc


def version_tuple(text: str) -> tuple[int, int, int]:
    match = re.search(r"(\d+)\.(\d+)\.(\d+)", text)
    if not match:
        raise GateError(f"could not parse semantic version from: {text!r}")
    return tuple(int(part) for part in match.groups())  # type: ignore[return-value]


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def source_manifest_rows(root: Path = ROOT) -> list[tuple[str, str, int]]:
    manifest = root / MANIFEST_NAME
    try:
        lines = manifest.read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeError) as exc:
        raise GateError(f"could not read {MANIFEST_NAME}") from exc
    if not lines or lines[0] != "sha256\tbytes\tpath":
        raise GateError(f"unexpected {MANIFEST_NAME} header")

    rows: list[tuple[str, str, int]] = []
    for line in lines[1:]:
        parts = line.split("\t")
        if len(parts) != 3:
            raise GateError(f"invalid {MANIFEST_NAME} row: {line!r}")
        digest, size_text, rel = parts
        path = PurePosixPath(rel)
        if (
            not re.fullmatch(r"[0-9a-f]{64}", digest)
            or not size_text.isdigit()
            or path.is_absolute()
            or not path.parts
            or any(part in {"", ".", ".."} for part in path.parts)
            or "\\" in rel
        ):
            raise GateError(f"invalid {MANIFEST_NAME} row: {line!r}")
        rows.append((rel, digest, int(size_text)))
    return rows


def release_archive_members(root: Path = ROOT) -> list[str]:
    rows = source_manifest_rows(root)
    return sorted([rel for rel, _digest, _size in rows] + [MANIFEST_NAME, INTEGRITY_NAME])


def inspect_release_archive(archive: Path) -> tuple[str, list[zipfile.ZipInfo]]:
    if not archive.is_file():
        raise GateError(f"release archive does not exist: {archive}")
    try:
        with zipfile.ZipFile(archive) as zipped:
            infos = zipped.infolist()
    except (OSError, zipfile.BadZipFile) as exc:
        raise GateError(f"release archive is not a readable ZIP: {archive}") from exc

    if not infos:
        raise GateError("release archive is empty")
    total_bytes = sum(info.file_size for info in infos)
    if total_bytes > MAX_ARCHIVE_BYTES:
        raise GateError(
            f"release archive expands beyond the {MAX_ARCHIVE_BYTES}-byte safety limit"
        )

    names: list[str] = []
    casefolded: set[str] = set()
    prefixes: set[str] = set()
    for info in infos:
        name = info.filename
        path = PurePosixPath(name)
        unix_mode = info.external_attr >> 16
        if (
            info.is_dir()
            or name.endswith("/")
            or "\\" in name
            or path.is_absolute()
            or path.as_posix() != name
            or len(path.parts) < 2
            or any(part in {"", ".", ".."} for part in path.parts)
            or any(
                ":" in part or part.endswith(" ") or part.endswith(".")
                for part in path.parts
            )
            or info.flag_bits & 0x1
            or stat.S_IFMT(unix_mode) == stat.S_IFLNK
        ):
            raise GateError(f"unsafe or non-canonical release archive member: {name!r}")
        folded = name.casefold()
        if folded in casefolded:
            raise GateError(f"duplicate release archive member: {name!r}")
        casefolded.add(folded)
        names.append(name)
        prefixes.add(path.parts[0])

    if len(prefixes) != 1:
        raise GateError("release archive must contain exactly one top-level directory")
    prefix = next(iter(prefixes))
    if prefix != ARCHIVE_PREFIX:
        raise GateError(
            f"release archive top-level directory must be {ARCHIVE_PREFIX!r}; got {prefix!r}"
        )
    required = {
        f"{prefix}/{MANIFEST_NAME}",
        f"{prefix}/{INTEGRITY_NAME}",
        f"{prefix}/tools/release_gate.py",
    }
    missing = sorted(required - set(names))
    if missing:
        raise GateError(f"release archive is missing required members: {missing}")
    return prefix, infos


def package_release_archive(archive: Path) -> None:
    validate_source_checkout()
    archive = archive.expanduser().resolve()
    if archive.suffix.lower() != ".zip":
        raise GateError("release archive output must use the .zip extension")
    try:
        archive.relative_to(ROOT)
    except ValueError:
        pass
    else:
        raise GateError("release archive output must be outside the source tree")
    if archive.exists():
        raise GateError(f"refusing to overwrite existing release archive: {archive}")

    rows = source_manifest_rows()
    for rel, digest, size in rows:
        path = ROOT.joinpath(*PurePosixPath(rel).parts)
        if not path.is_file() or path.stat().st_size != size or sha256(path) != digest:
            raise GateError(f"source changed after manifest validation: {rel}")

    archive.parent.mkdir(parents=True, exist_ok=True)
    temporary: Path | None = None
    try:
        with tempfile.NamedTemporaryFile(
            prefix=f".{archive.name}.", suffix=".tmp", dir=archive.parent, delete=False
        ) as handle:
            temporary = Path(handle.name)
        with zipfile.ZipFile(
            temporary, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
        ) as zipped:
            for rel in release_archive_members():
                info = zipfile.ZipInfo(
                    f"{ARCHIVE_PREFIX}/{rel}", date_time=FIXED_ZIP_DATE
                )
                info.create_system = 3
                info.compress_type = zipfile.ZIP_DEFLATED
                info.external_attr = (stat.S_IFREG | 0o644) << 16
                zipped.writestr(info, ROOT.joinpath(*PurePosixPath(rel).parts).read_bytes())
        temporary.replace(archive)
        temporary = None
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)

    _prefix, infos = inspect_release_archive(archive)
    print(f"packaged {len(infos)} files -> {archive}")
    print(f"zip bytes  {archive.stat().st_size}")
    print(f"zip sha256 {sha256(archive)}")


def release_temp_base() -> Path:
    configured = os.environ.get("GVYA_RELEASE_TEMP")
    base = Path(configured).expanduser() if configured else Path(tempfile.gettempdir())
    try:
        base.mkdir(parents=True, exist_ok=True)
        return base.resolve()
    except OSError as exc:
        source = "GVYA_RELEASE_TEMP" if configured else "system temporary directory"
        raise GateError(f"could not prepare {source}: {base}") from exc


def extract_release_archive(
    archive: Path, destination: Path, infos: list[zipfile.ZipInfo]
) -> None:
    try:
        with zipfile.ZipFile(archive) as zipped:
            for info in infos:
                target = destination.joinpath(*PurePosixPath(info.filename).parts)
                target.parent.mkdir(parents=True, exist_ok=True)
                with zipped.open(info) as source, target.open("xb") as output_file:
                    shutil.copyfileobj(source, output_file)
    except (OSError, zipfile.BadZipFile) as exc:
        raise GateError(f"could not extract release archive: {archive}") from exc


def certify_release_archive(archive: Path, *, security: bool, browser: bool) -> None:
    archive = archive.expanduser().resolve()
    prefix, infos = inspect_release_archive(archive)
    temp_base = release_temp_base()
    with tempfile.TemporaryDirectory(prefix="g-", dir=temp_base) as temp_name:
        extraction_root = Path(temp_name)
        if os.name == "nt":
            longest = max(infos, key=lambda info: len(info.filename)).filename
            projected = extraction_root.joinpath(*PurePosixPath(longest).parts)
            if len(str(projected)) > WINDOWS_SAFE_PATH_LIMIT:
                raise GateError(
                    "fresh-extraction path is too long for reliable Windows tooling "
                    f"({len(str(projected))} characters); set GVYA_RELEASE_TEMP to a short "
                    "directory such as C:\\g"
                )
        print(f"extracting exact release ZIP under short root: {extraction_root}")
        extract_release_archive(archive, extraction_root, infos)
        source_root = extraction_root / prefix
        argv = [sys.executable, str(source_root / "tools" / "release_gate.py"), "certify"]
        if browser:
            argv.append("--browser")
        if security:
            argv.append("--security")
        run(argv, cwd=source_root)
    print("\nFRESH-EXTRACTION CERTIFICATION PASS")
    print(f"archive sha256 {sha256(archive)}")


def require_command(name: str) -> str:
    path = shutil.which(name)
    if not path:
        raise GateError(f"missing required command: {name}")
    return path


def check_environment(*, require_locks: bool, require_security_tools: bool) -> None:
    rust_version, node_min_major, wasm_target = toolchain_contract()
    for command in ["rustc", "cargo", "rustfmt", "clippy-driver", "node", "npm"]:
        require_command(command)

    rustc = output(["rustc", "--version"])
    if version_tuple(rustc) != version_tuple(rust_version):
        raise GateError(f"Rust must be exactly {rust_version}; got {rustc}")

    cargo = output(["cargo", "--version"])
    if version_tuple(cargo) != version_tuple(rust_version):
        raise GateError(f"Cargo must match Rust {rust_version}; got {cargo}")

    # Installation mechanism is deliberately irrelevant. Official standalone Rust
    # distributions and rustup-managed toolchains are both valid as long as the
    # exact compiler/tool versions and required target libraries are present.
    target_libdir = Path(output(["rustc", "--print", "target-libdir", "--target", wasm_target]))
    if not target_libdir.is_dir() or not any(target_libdir.glob("libstd-*.rlib")):
        raise GateError(f"missing Rust target libraries: {wasm_target} ({target_libdir})")

    node = output(["node", "--version"])
    if version_tuple(node)[0] < node_min_major:
        raise GateError(f"Node must be >= {node_min_major}; got {node}")

    npm = output(["npm", "--version"])
    print(f"toolchain: {rustc}; {cargo}; node {node}; npm {npm}")

    if require_locks:
        for rel in ["Cargo.lock", "package-lock.json"]:
            path = ROOT / rel
            if not path.is_file() or path.stat().st_size == 0:
                raise GateError(f"missing frozen lockfile: {rel}; run `npm run bootstrap:locks` in a compliant connected environment, review the diff, then certify")

    if require_security_tools:
        require_command("cargo-audit")


def bootstrap_locks() -> None:
    check_environment(require_locks=False, require_security_tools=False)
    if (ROOT / "Cargo.lock").exists() or (ROOT / "package-lock.json").exists():
        raise GateError("lockfile bootstrap refuses to overwrite an existing Cargo.lock or package-lock.json; review/remove intentionally before regenerating")

    run(["cargo", "generate-lockfile"])
    run(["npm", "install", "--package-lock-only", "--ignore-scripts"])

    cargo_lock = ROOT / "Cargo.lock"
    npm_lock = ROOT / "package-lock.json"
    if not cargo_lock.is_file() or not npm_lock.is_file():
        raise GateError("lockfile bootstrap did not produce both required lockfiles")

    # Prove both tools accept the frozen graphs before a human review/freeze.
    run(["cargo", "metadata", "--locked", "--format-version", "1"], env={"CARGO_NET_OFFLINE": "false"})
    run(["npm", "install", "--package-lock-only", "--ignore-scripts"])
    print("lockfiles generated; review them before certification:")
    print(f"  Cargo.lock      {sha256(cargo_lock)}")
    print(f"  package-lock.json {sha256(npm_lock)}")
    print("After reviewing the lockfile diff, run `npm run refresh:manifests` to pin the reviewed lockfiles before preflight/certification.")
    print("No freeze claim is made by bootstrap-locks itself.")


def validate_source_checkout() -> None:
    # Membership/hash validation is intentionally independent of Rust/Node availability so a
    # tampered or stale checkout fails before expensive toolchain work begins.
    run([sys.executable, "tools/validate-source.py"])


def preflight() -> None:
    validate_source_checkout()
    check_environment(require_locks=True, require_security_tools=False)
    run(["cargo", "metadata", "--locked", "--format-version", "1"])
    # `npm ci --ignore-scripts --dry-run` is not consistently side-effect free across npm releases.
    # Parsing the lock here plus the real `npm ci` in certify keeps preflight non-mutating.
    lock = json.loads((ROOT / "package-lock.json").read_text(encoding="utf-8"))
    if lock.get("lockfileVersion") not in {2, 3}:
        raise GateError(f"unsupported npm lockfileVersion: {lock.get('lockfileVersion')!r}")
    print("preflight PASS")


def certify(*, security: bool, browser: bool) -> None:
    # Source structure and exact manifest membership/hash must be clean before toolchain work or
    # generated output exists. This keeps INTEGRITY.sha256 an enforced release boundary.
    validate_source_checkout()
    check_environment(require_locks=True, require_security_tools=security)

    run(["cargo", "fmt", "--check"])
    run(["cargo", "check", "--workspace", "--locked"])
    run(["cargo", "test", "--workspace", "--locked"])
    # `cargo check` above enforces the workspace's deny(warnings) contract for rustc.
    # Cap this separate diagnostic pass at warn so the explicitly advisory Clippy
    # all/pedantic groups remain visible instead of being promoted by deny(warnings).
    run(
        [
            "cargo",
            "clippy",
            "--workspace",
            "--all-targets",
            "--locked",
            "--message-format=short",
            "--",
            "--cap-lints",
            "warn",
        ]
    )
    _rust_version, _node_min_major, wasm_target = toolchain_contract()
    wasm_artifact = ROOT / "target" / wasm_target / "debug" / "gvya_ffi.wasm"
    gvya_binary = ROOT / "target" / "debug" / ("gvya.exe" if os.name == "nt" else "gvya")
    run(["cargo", "build", "-p", "gvya-ffi", "--target", wasm_target, "--locked"])
    if not wasm_artifact.is_file():
        raise GateError(f"expected WASM artifact missing: {wasm_artifact.relative_to(ROOT)}")
    run(["cargo", "build", "-p", "gvya-cli", "--locked"])
    if not gvya_binary.is_file():
        raise GateError(f"expected canonical CLI missing: {gvya_binary.relative_to(ROOT)}")

    run(["npm", "ci"])
    run(["npm", "run", "test:source"])
    run(["npm", "run", "test:gvya-help"])
    run(["node", "validation/authoring-e2e/run.mjs", "--process-proof-only", "--gvya", str(gvya_binary)])
    run(["node", "validation/runtime-sdk-wasm.mjs", str(wasm_artifact)])
    run(["node", "validation/godot-web-wasm.mjs", str(wasm_artifact)])
    run(["npm", "run", "build:studio"])
    run([sys.executable, "tools/verify_engine_assets.py"])
    run(["node", "validation/studio-engine-contract.mjs"])
    run(["node", "validation/engine-v1-acceptance.mjs"])

    if browser:
        run(["node", "validation/browser-acceptance.mjs"])

    if security:
        run(["cargo", "audit"])
        run(["npm", "audit", "--audit-level=high"])

    print("\nCERTIFICATION EXECUTION PASS")
    print("This proves the executable gates for this checkout/toolchain. A final release still requires reviewing the frozen lockfiles and repeating this command from a fresh extraction of the release ZIP.")


def main() -> int:
    parser = argparse.ArgumentParser(description="Fail-closed GVYA release certification gate")
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("bootstrap-locks", help="generate first Cargo/npm lockfiles; never overwrites existing locks")
    sub.add_parser("preflight", help="verify exact toolchain/targets and frozen lockfiles without building")
    certify_parser = sub.add_parser("certify", help="run executable release gates")
    certify_parser.add_argument("--security", action="store_true", help="also require cargo-audit and run Cargo/npm vulnerability audits")
    certify_parser.add_argument("--browser", action="store_true", help="also run rendered Chromium browser acceptance against the Vite build")
    package_parser = sub.add_parser(
        "package", help="build the deterministic source ZIP from the exact manifest member set"
    )
    package_parser.add_argument("archive", type=Path, help="output ZIP path outside the source tree")
    archive_parser = sub.add_parser(
        "certify-archive", help="certify an exact release ZIP from an automatic short-root extraction"
    )
    archive_parser.add_argument("archive", type=Path, help="release ZIP to extract and certify")
    archive_parser.add_argument("--security", action="store_true", help="also require cargo-audit and run Cargo/npm vulnerability audits")
    archive_parser.add_argument("--browser", action="store_true", help="also run rendered Chromium browser acceptance against the Vite build")
    args = parser.parse_args()

    try:
        if args.command == "bootstrap-locks":
            bootstrap_locks()
        elif args.command == "preflight":
            preflight()
        elif args.command == "certify":
            certify(security=args.security, browser=args.browser)
        elif args.command == "package":
            package_release_archive(args.archive)
        elif args.command == "certify-archive":
            certify_release_archive(
                args.archive, security=args.security, browser=args.browser
            )
        else:
            raise GateError(f"unknown command: {args.command}")
    except (
        GateError,
        OSError,
        subprocess.CalledProcessError,
        json.JSONDecodeError,
        zipfile.BadZipFile,
    ) as exc:
        print(f"CERTIFICATION FAIL: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
