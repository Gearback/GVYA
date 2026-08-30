#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
import json
import os
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

sys.dont_write_bytecode = True

ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "tools" / "release_gate.py"
SPEC = importlib.util.spec_from_file_location("gvya_release_gate", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"could not load {MODULE_PATH}")
release_gate = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release_gate)


class EngineBuildScriptTests(unittest.TestCase):
    def test_builds_and_verifies_engine_before_source_validation(self) -> None:
        script = (ROOT / "tools" / "build-engine-v1.ps1").read_text(encoding="utf-8")
        markers = (
            'Invoke-Python "tools/build_engine_assets.py"',
            'Invoke-Python "tools/verify_engine_assets.py"',
            "& npm run test:source",
        )
        for marker in markers:
            self.assertEqual(script.count(marker), 1, f"expected one {marker!r}")
        self.assertLess(script.index(markers[0]), script.index(markers[1]))
        self.assertLess(script.index(markers[1]), script.index(markers[2]))


class CommandResolutionTests(unittest.TestCase):
    def test_resolves_windows_cmd_launcher_to_concrete_path(self) -> None:
        with patch.object(
            release_gate.shutil, "which", return_value=r"C:\Program Files\nodejs\npm.CMD"
        ):
            resolved = release_gate.resolve_argv(["npm", "--version"])
        self.assertEqual(
            resolved, [r"C:\Program Files\nodejs\npm.CMD", "--version"]
        )

    def test_missing_command_fails_closed(self) -> None:
        with patch.object(release_gate.shutil, "which", return_value=None):
            with self.assertRaisesRegex(release_gate.GateError, "missing required command"):
                release_gate.resolve_argv(["not-installed"])

    @unittest.skipUnless(os.name == "nt", "real .CMD launch is Windows-specific")
    def test_executes_cmd_launcher_without_shell_interpolation(self) -> None:
        with tempfile.TemporaryDirectory(prefix="gvya-cmd-test-") as temp_name:
            launcher = Path(temp_name) / "gvya-release-probe.CMD"
            launcher.write_bytes(b"@echo off\r\necho CMD_RESOLUTION_OK\r\n")
            env = {
                "PATH": f"{temp_name}{os.pathsep}{os.environ.get('PATH', '')}",
                "PATHEXT": ".COM;.EXE;.BAT;.CMD",
            }
            with patch.dict(os.environ, env):
                self.assertEqual(
                    release_gate.output(["gvya-release-probe"]), "CMD_RESOLUTION_OK"
                )


class SourceGateCoverageTests(unittest.TestCase):
    def test_source_gate_keeps_authoring_e2e_but_not_full_help_suite(self) -> None:
        package = json.loads((ROOT / "package.json").read_text(encoding="utf-8"))
        scripts = package["scripts"]
        self.assertEqual(
            scripts.get("test:authoring-e2e"),
            "node validation/authoring-e2e/run.mjs --source-proof",
        )
        source_gate = scripts.get("test:source", "")
        self.assertIn("npm run test:authoring-e2e", source_gate)
        self.assertNotIn("npm run test:gvya-help", source_gate)

    def test_full_help_behavioral_suite_is_release_gated(self) -> None:
        script = MODULE_PATH.read_text(encoding="utf-8")
        source_marker = 'run(["npm", "run", "test:source"])'
        help_marker = 'run(["npm", "run", "test:gvya-help"])'
        self.assertEqual(script.count(help_marker), 1)
        self.assertLess(script.index(source_marker), script.index(help_marker))



class ArchiveBoundaryTests(unittest.TestCase):
    def test_rejects_parent_traversal_member(self) -> None:
        with tempfile.TemporaryDirectory(prefix="gvya-zip-test-") as temp_name:
            archive = Path(temp_name) / "bad.zip"
            with zipfile.ZipFile(archive, "w") as zipped:
                zipped.writestr("gvya/../escape", b"bad")
            with self.assertRaisesRegex(release_gate.GateError, "unsafe"):
                release_gate.inspect_release_archive(archive)

    def test_rejects_case_colliding_members(self) -> None:
        with tempfile.TemporaryDirectory(prefix="gvya-zip-test-") as temp_name:
            archive = Path(temp_name) / "bad.zip"
            with zipfile.ZipFile(archive, "w") as zipped:
                zipped.writestr("gvya/SOURCE_MANIFEST.tsv", b"manifest")
                zipped.writestr("gvya/source_manifest.tsv", b"collision")
                zipped.writestr("gvya/INTEGRITY.sha256", b"integrity")
                zipped.writestr("gvya/tools/release_gate.py", b"gate")
            with self.assertRaisesRegex(release_gate.GateError, "duplicate"):
                release_gate.inspect_release_archive(archive)


class AuthoringReleaseGateTests(unittest.TestCase):
    def test_canonical_cli_process_proof_is_release_gated(self) -> None:
        script = MODULE_PATH.read_text(encoding="utf-8")
        build_marker = 'run(["cargo", "build", "-p", "gvya-cli", "--locked"])'
        proof_marker = 'run(["node", "validation/authoring-e2e/run.mjs", "--process-proof-only", "--gvya", str(gvya_binary)])'
        self.assertEqual(script.count(build_marker), 1)
        self.assertEqual(script.count(proof_marker), 1)
        self.assertLess(script.index(build_marker), script.index(proof_marker))



class GodotReleaseGateTests(unittest.TestCase):
    def test_fresh_wasm_is_exercised_through_godot_bridge(self) -> None:
        script = MODULE_PATH.read_text(encoding="utf-8")
        runtime_marker = 'run(["node", "validation/runtime-sdk-wasm.mjs", str(wasm_artifact)])'
        godot_marker = 'run(["node", "validation/godot-web-wasm.mjs", str(wasm_artifact)])'
        self.assertEqual(script.count(runtime_marker), 1)
        self.assertEqual(script.count(godot_marker), 1)
        self.assertLess(script.index(runtime_marker), script.index(godot_marker))


if __name__ == "__main__":
    unittest.main()
