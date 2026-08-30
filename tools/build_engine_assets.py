#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ENGINE = "v1"
TARGET = "wasm32-unknown-unknown"
# One named production profile owns the shipped Engine bytes: whole-program LTO, a single codegen
# unit, and no debug/name symbol sections. Ordinary `cargo build --release` is unaffected.
PROFILE = "engine-wasm"
OUT = ROOT / "apps" / "studio" / "public" / "engine" / ENGINE


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def cargo() -> str:
    explicit = os.environ.get("CARGO")
    if explicit:
        return explicit
    found = shutil.which("cargo")
    if not found:
        raise SystemExit("cargo not found; set CARGO to the Rust 1.85 cargo executable")
    return found


def rustc(cargo_path: str) -> str:
    explicit = os.environ.get("RUSTC")
    if explicit:
        return explicit
    sibling = str(Path(cargo_path).with_name("rustc"))
    if Path(sibling).is_file():
        return sibling
    found = shutil.which("rustc")
    if not found:
        raise SystemExit("rustc not found; set RUSTC to the Rust 1.85 rustc executable")
    return found


def verify_toolchain(rustc_path: str) -> None:
    version = subprocess.run([rustc_path, "--version"], check=True, text=True, capture_output=True).stdout.strip()
    if not version.startswith("rustc 1.85.0 "):
        raise SystemExit(f"GVYA Engine v1 must be built with Rust 1.85.0; found {version}")
    target_libdir = Path(subprocess.run(
        [rustc_path, "--print", "target-libdir", "--target", TARGET],
        check=True, text=True, capture_output=True,
    ).stdout.strip())
    if not target_libdir.is_dir() or not any(target_libdir.glob("libcore-*.rlib")):
        raise SystemExit(
            "Rust 1.85.0 target wasm32-unknown-unknown is not installed. "
            "Install the matching standard-library target before building GVYA Engine v1."
        )


def run() -> None:
    cargo_path = cargo()
    rustc_path = rustc(cargo_path)
    verify_toolchain(rustc_path)
    command = [cargo_path, "build", "--locked", "--profile", PROFILE, "--target", TARGET, "-p", "gvya-ffi"]
    env = dict(os.environ)
    env["RUSTC"] = rustc_path
    # A handoff ZIP can be extracted over a directory that still contains newer Cargo artifacts.
    # Cargo's normal freshness checks are allowed to reuse those files, which can silently ship an
    # Engine compiled from older source. Build the canonical Engine in a disposable target root so
    # the shipped WASM is always produced from this checkout, regardless of ambient target state.
    with tempfile.TemporaryDirectory(prefix="gvya-engine-v1-") as target_root:
        env["CARGO_TARGET_DIR"] = target_root
        try:
            subprocess.run(command, cwd=ROOT, check=True, env=env)
        except subprocess.CalledProcessError as exc:
            raise SystemExit(
                "GVYA Engine v1 Rust build failed. Ensure the workspace Cargo dependencies are available "
                "for the pinned Rust 1.85.0 toolchain."
            ) from exc
        engine_src = Path(target_root) / TARGET / PROFILE / "gvya_ffi.wasm"
        if not engine_src.is_file():
            raise SystemExit("canonical Rust Engine WASM output was not produced")
        OUT.mkdir(parents=True, exist_ok=True)
        engine_out = OUT / "gvya-ffi.wasm"
        shutil.copyfile(engine_src, engine_out)
    # Clean-break: Studio carries one canonical module, never a compiler/runtime pair.
    for obsolete in (OUT / "gvya-compiler.wasm", OUT / "gvya-runtime.wasm"):
        obsolete.unlink(missing_ok=True)
    manifest = {
        "format": "gvya.engine-assets",
        "version": 1,
        "engine": ENGINE,
        "artifact_format": 1,
        "module": {"path": engine_out.name, "sha256": sha256(engine_out), "bytes": engine_out.stat().st_size, "abi": 1},
    }
    (OUT / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"built GVYA Engine {ENGINE}")
    print(f"module {manifest['module']['sha256']} {manifest['module']['bytes']} bytes")


if __name__ == "__main__":
    run()
