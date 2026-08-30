import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const here = dirname(fileURLToPath(import.meta.url));
const bridgePath = resolve(here, "../adapters/godot/web/gvya-godot-web.js");
const bridge = require(bridgePath);
const wasmPath = process.argv[2] ?? resolve(here, "../apps/studio/public/engine/v1/gvya-ffi.wasm");
const artifactPath = resolve(here, "fixtures/runtime-action.gvya");
const signedArtifactPath = resolve(here, "fixtures/runtime-signed.gvya");
const wasm = new Uint8Array(await readFile(wasmPath));
const artifact = new Uint8Array(await readFile(artifactPath));
const signedArtifact = new Uint8Array(await readFile(signedArtifactPath));

const developmentOptions = JSON.stringify({
  format: "gvya.runtime.open-options",
  version: 1,
  signature: { mode: "allow_unsigned" },
});
const signedOptions = JSON.stringify({
  format: "gvya.runtime.open-options",
  version: 1,
  signature: { mode: "require_present" },
});

assert.equal(bridge.open_bytes(wasm, artifact, developmentOptions), true, bridge.last_error_json());
assert.equal(bridge.is_open(), true);

const info = JSON.parse(bridge.info_json());
assert.equal(info.format, "gvya.runtime.info");
assert.equal(info.version, 1);
assert.equal(info.project_id, "runtime-action");

const capabilities = JSON.parse(bridge.capabilities_json());
assert.equal(capabilities.format, "gvya.runtime.capabilities");
assert(capabilities.capabilities.some((row) => row.id === "demo.wave" && row.version === "1"));
const capabilityInfo = JSON.parse(bridge.capability_info_json("demo.wave"));
assert.equal(capabilityInfo.format, "gvya.runtime.capability-info");
assert.equal(capabilityInfo.capability.id, "demo.wave");

const turnRequest = {
  format: "gvya.runtime.turn",
  version: 1,
  utterance: { text: "hello" },
  context: { available_capabilities: [{ id: "demo.wave", version: "1" }] },
  seed: 7,
};
const turn = JSON.parse(bridge.turn_json(JSON.stringify(turnRequest)));
assert.equal(turn.format, "gvya.runtime.turn-result");
assert.equal(turn.version, 1);
assert.equal(turn.meaning?.id, "hello");
assert.equal(turn.capabilities?.decisions?.[0]?.outcome?.type, "admitted");
const proposal = turn.capabilities.decisions[0].proposal;
assert.equal(proposal.capability, "demo.wave");

const capabilityResult = JSON.parse(bridge.capability_result_json(JSON.stringify({
  format: "gvya.runtime.capability-result",
  version: 1,
  proposal,
  result: { proposal_id: proposal.id, succeeded: true },
  state: turn.state,
  context: { available_capabilities: [{ id: "demo.wave", version: "1" }] },
  seed: 11,
})));
assert.equal(capabilityResult.format, "gvya.runtime.capability-result-result");
assert.equal(capabilityResult.validation.accepted, true);
assert.equal(capabilityResult.interaction?.behavior, "wave.result");

const assetById = bridge.asset_by_id("tone");
assert.deepEqual(assetById, new TextEncoder().encode("GVYA runtime fixture asset\n"));
const assetByPath = bridge.asset_by_path("assets/tone.bin");
assert.deepEqual(assetByPath, assetById);
const assetInfo = JSON.parse(bridge.asset_info_by_id_json("tone"));
assert.equal(assetInfo.format, "gvya.runtime.asset-info");
assert.equal(assetInfo.logical_path, "assets/tone.bin");

const badWire = JSON.parse(bridge.turn_json(JSON.stringify({ ...turnRequest, version: 2 })));
assert.equal(typeof badWire.error, "string", "invalid wire version must fail closed at the canonical runtime edge");
assert.match(badWire.error, /version|turn|unsupported|invalid/i);

assert.equal(bridge.close(), true);
assert.equal(bridge.is_open(), false);

assert.equal(bridge.open_bytes(wasm, artifact, signedOptions), false, "unsigned artifact must fail require_present policy");
assert.match(JSON.parse(bridge.last_error_json()).error, /signature|unsigned/i);

assert.equal(bridge.open_bytes(wasm, signedArtifact, signedOptions), true, bridge.last_error_json());
assert.equal(JSON.parse(bridge.info_json()).format, "gvya.runtime.info");
assert.equal(bridge.close(), true);

const boundedOptions = JSON.stringify({
  format: "gvya.runtime.open-options",
  version: 1,
  artifact_limits: { max_total_bytes: 1 },
  signature: { mode: "allow_unsigned" },
});
assert.equal(bridge.open_bytes(wasm, artifact, boundedOptions), false, "adapter must reject artifact over configured pre-copy budget");
assert.match(JSON.parse(bridge.last_error_json()).error, /total byte limit/i);

const invalidCeilingOptions = JSON.stringify({
  format: "gvya.runtime.open-options",
  version: 1,
  artifact_limits: { max_total_bytes: 512 * 1024 * 1024 + 1 },
  signature: { mode: "allow_unsigned" },
});
assert.equal(bridge.open_bytes(wasm, artifact, invalidCeilingOptions), false, "adapter must not allow limits above canonical ceilings");
assert.match(JSON.parse(bridge.last_error_json()).error, /canonical ceiling/i);

console.log("Godot Web/WASM adapter parity: PASS (runtime, wire, policy, limits, capabilities, assets)");
