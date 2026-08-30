#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DIR = ROOT / "apps" / "studio" / "public" / "engine" / "v1"
MANIFEST = DIR / "manifest.json"

EXPECTED_EXPORTS = {
    "memory",
    "gvya_abi_version",
    "gvya_pointer_width",
    "gvya_buffer_struct_size",
    "gvya_alloc",
    "gvya_dealloc",
    "gvya_compiler_validate_source_tree",
    "gvya_compiler_build_source_tree",
    "gvya_runtime_open_with_options_json",
    "gvya_runtime_close",
    "gvya_runtime_info_json",
    "gvya_runtime_capabilities_json",
    "gvya_runtime_capability_info_json",
    "gvya_runtime_turn_json",
    "gvya_runtime_open_conversation_json",
    "gvya_runtime_capability_result_json",
    "gvya_runtime_asset_by_path",
    "gvya_runtime_asset_by_id",
    "gvya_runtime_asset_info_by_id_json",
    "gvya_buffer_free",
}


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def fail(message: str) -> None:
    raise SystemExit(message)


def read_uleb(data: bytes, offset: int) -> tuple[int, int]:
    value = 0
    shift = 0
    for _ in range(10):
        if offset >= len(data):
            fail("GVYA Engine WASM is truncated")
        byte = data[offset]
        offset += 1
        value |= (byte & 0x7F) << shift
        if byte & 0x80 == 0:
            return value, offset
        shift += 7
    fail("GVYA Engine WASM contains an invalid LEB128 integer")


def wasm_custom_sections(data: bytes) -> list[str]:
    """Names of every custom section in the module, in binary order."""
    if data[:8] != b"\x00asm\x01\x00\x00\x00":
        fail("GVYA Engine binary is not a WebAssembly v1 module")
    offset = 8
    names: list[str] = []
    while offset < len(data):
        section_id = data[offset]
        offset += 1
        size, offset = read_uleb(data, offset)
        end = offset + size
        if end > len(data):
            fail("GVYA Engine WASM section is truncated")
        if section_id == 0:
            name_len, cursor = read_uleb(data, offset)
            name_end = cursor + name_len
            if name_end > end:
                fail("GVYA Engine WASM custom section name is truncated")
            try:
                names.append(data[cursor:name_end].decode("utf-8"))
            except UnicodeDecodeError:
                fail("GVYA Engine WASM custom section name is not UTF-8")
        offset = end
    return names


def wasm_exports(data: bytes) -> set[str]:
    if data[:8] != b"\x00asm\x01\x00\x00\x00":
        fail("GVYA Engine binary is not a WebAssembly v1 module")
    offset = 8
    exports: set[str] = set()
    while offset < len(data):
        section_id = data[offset]
        offset += 1
        size, offset = read_uleb(data, offset)
        end = offset + size
        if end > len(data):
            fail("GVYA Engine WASM section is truncated")
        if section_id == 7:
            count, cursor = read_uleb(data, offset)
            for _ in range(count):
                name_len, cursor = read_uleb(data, cursor)
                name_end = cursor + name_len
                if name_end > end:
                    fail("GVYA Engine WASM export name is truncated")
                try:
                    name = data[cursor:name_end].decode("utf-8")
                except UnicodeDecodeError:
                    fail("GVYA Engine WASM export name is not UTF-8")
                cursor = name_end
                if cursor >= end:
                    fail("GVYA Engine WASM export descriptor is truncated")
                cursor += 1
                _, cursor = read_uleb(data, cursor)
                exports.add(name)
            if cursor != end:
                fail("GVYA Engine WASM export section contains trailing bytes")
        offset = end
    if offset != len(data):
        fail("GVYA Engine WASM has trailing bytes")
    return exports


def main() -> None:
    if not MANIFEST.is_file():
        fail("GVYA Engine v1 manifest is missing; run tools/build_engine_assets.py")
    try:
        value = json.loads(MANIFEST.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        fail("GVYA Engine v1 manifest is unreadable")
    if set(value) != {"format", "version", "engine", "artifact_format", "module"}:
        fail("GVYA Engine v1 manifest fields are invalid")
    if value.get("format") != "gvya.engine-assets" or value.get("version") != 1 or value.get("engine") != "v1" or value.get("artifact_format") != 1:
        fail("GVYA Engine v1 manifest identity is invalid")
    row = value.get("module")
    if not isinstance(row, dict) or set(row) != {"path", "sha256", "bytes", "abi"} or row.get("abi") != 1:
        fail("GVYA Engine module manifest is invalid")
    if row.get("path") != "gvya-ffi.wasm":
        fail("GVYA Engine module path is invalid")
    path = DIR / "gvya-ffi.wasm"
    if not path.is_file():
        fail("GVYA Engine WASM is missing")
    data = path.read_bytes()
    if path.stat().st_size != row.get("bytes") or sha256(path) != row.get("sha256"):
        fail("GVYA Engine WASM integrity mismatch")
    exports = wasm_exports(data)
    missing = sorted(EXPECTED_EXPORTS - exports)
    if missing:
        fail(f"GVYA Engine WASM ABI exports are incomplete: {missing}")
    # A production Engine ships executable bytes only. The Rust `name` symbol table and DWARF
    # sections are development metadata that no runtime or ABI consumer reads.
    debug_sections = sorted(
        name
        for name in wasm_custom_sections(data)
        if name == "name" or name.startswith(".debug")
    )
    if debug_sections:
        fail(
            "GVYA Engine WASM carries development-only custom sections "
            f"{debug_sections}; build it with tools/build_engine_assets.py"
        )
    for obsolete in (DIR / "gvya-compiler.wasm", DIR / "gvya-runtime.wasm"):
        if obsolete.exists():
            fail(f"obsolete split Engine asset remains: {obsolete.name}")
    print(
        "GVYA Engine v1 asset verified "
        "(single-module identity, integrity, wasm32 ABI exports, no debug/name sections)"
    )


if __name__ == "__main__":
    main()
