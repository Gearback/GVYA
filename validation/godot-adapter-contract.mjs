import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const gd = await readFile(resolve(here, "../adapters/godot/GVYARuntime.gd"), "utf8");
const js = await readFile(resolve(here, "../adapters/godot/web/gvya-godot-web.js"), "utf8");
const readme = await readFile(resolve(here, "../adapters/godot/README.md"), "utf8");

for (const method of [
  "open_file",
  "info",
  "capabilities",
  "capability_info",
  "turn",
  "open_conversation",
  "capability_result",
  "asset_by_path",
  "asset_by_id",
  "asset_info_by_id",
  "close",
]) {
  assert.match(gd, new RegExp(`func ${method}\\(`), `Godot adapter is missing ${method}`);
}

for (const abiExport of [
  "gvya_runtime_open_with_options_json",
  "gvya_runtime_info_json",
  "gvya_runtime_capabilities_json",
  "gvya_runtime_capability_info_json",
  "gvya_runtime_turn_json",
  "gvya_runtime_open_conversation_json",
  "gvya_runtime_capability_result_json",
  "gvya_runtime_asset_by_path",
  "gvya_runtime_asset_by_id",
  "gvya_runtime_asset_info_by_id_json",
  "gvya_runtime_close",
]) {
  assert(js.includes(abiExport), `Godot Web bridge is missing canonical ABI export ${abiExport}`);
}

assert(js.includes("GVYAGodotWebRuntime"), "Godot Web bridge must publish one explicit JavaScriptBridge interface");
assert(gd.includes('JavaScriptBridge.get_interface("GVYAGodotWebRuntime")'), "GDScript must bind the shipped Web bridge explicitly");
assert(gd.includes("JavaScriptBridge.js_buffer_to_packed_byte_array"), "Godot adapter must preserve raw asset bytes");
assert(gd.includes('"gvya.runtime.open-options"'), "Godot adapter must use the canonical open-options wire contract");
assert(gd.includes("explicit artifact trust/open policy"), "Godot adapter must require an explicit open/trust policy");
assert(gd.includes("unsigned_development_open_options"), "Godot adapter must expose an explicit development policy helper");
assert(gd.includes("require_signed_artifact_options"), "Godot adapter must expose an explicit signed-artifact policy helper");
assert(js.includes("MAX_RUNTIME_REQUEST_BYTES = 1024 * 1024"), "Godot Web bridge must enforce the canonical adapter request budget");
assert(js.includes("DEFAULT_MAX_ARTIFACT_BYTES = 512 * 1024 * 1024"), "Godot Web bridge must preserve the canonical artifact ceiling");

const forbiddenRuntimeAuthority = [
  /semantic[ _-]?match(?:er|ing)?\s*=/i,
  /response[ _-]?select(?:or|ion)?\s*=/i,
  /conversation[ _-]?(?:kernel|state machine)\s*=/i,
  /capability[ _-]?(?:admission|policy)\s*=/i,
];
for (const pattern of forbiddenRuntimeAuthority) {
  assert(!pattern.test(gd), `GDScript must not implement runtime authority: ${pattern}`);
  assert(!pattern.test(js), `Web bridge must not implement runtime authority: ${pattern}`);
}

assert.match(readme, /supported GVYA SDK target through the Web\/WASM bridge/i);
assert.match(readme, /No native GDExtension is shipped/i);
console.log("Godot adapter contract: PASS");
