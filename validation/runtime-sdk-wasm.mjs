import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { GvyaRuntime, unsignedDevelopmentOpenOptions, WasmRuntimeBackend } from "../packages/runtime-sdk/dist/index.js";

const here = dirname(fileURLToPath(import.meta.url));
const wasmPath = process.argv[2] ?? resolve(here, "../target/wasm32-unknown-unknown/debug/gvya_ffi.wasm");
const artifactPath = resolve(here, "fixtures/runtime-action.gvya");
const wasmBytes = await readFile(wasmPath);
const artifact = new Uint8Array(await readFile(artifactPath));
const backend = await WasmRuntimeBackend.instantiate(await WebAssembly.compile(wasmBytes));
const runtime = await GvyaRuntime.open(artifact, backend, unsignedDevelopmentOpenOptions());
try {
  const info = await runtime.info();
  assert.equal(info.format, "gvya.runtime.info");
  assert.equal(info.project_id, "runtime-action");

  const result = await runtime.turn({
    format: "gvya.runtime.turn",
    version: 1,
    utterance: { text: "hello" },
    context: { available_capabilities: [{ id: "demo.wave", version: "1" }] },
    seed: 7,
  });
  assert.equal(result.format, "gvya.runtime.turn-result");
  assert.equal(result.mode, "answer");
  assert.equal(result.meaning?.id, "hello");
  assert.equal(result.semantic?.scores?.[0]?.meaning, "hello", "semantic score rows must expose stable Meaning IDs");
  assert.equal(typeof result.semantic?.scores?.[0]?.pattern_index, "number", "pattern_index remains an implementation diagnostic beside the Meaning ID");
  const messages = result.response?.messages ?? [];
  assert(messages.some((message) => message.items?.some((item) => item.type === "text" && item.text === "Hello from GVYA.")));
  const decisions = result.capabilities?.decisions ?? [];
  assert.equal(decisions.length, 1);
  assert.equal(decisions[0].outcome?.type, "admitted");
  const proposal = decisions[0].proposal;
  assert.equal(proposal.capability, "demo.wave");

  const capabilityResult = await runtime.capabilityResult({
    format: "gvya.runtime.capability-result",
    version: 1,
    proposal,
    result: { proposal_id: proposal.id, succeeded: true },
    state: result.state,
    context: { available_capabilities: [{ id: "demo.wave", version: "1" }] },
    seed: 11,
  });
  assert.equal(capabilityResult.validation.accepted, true);
  assert.equal(capabilityResult.interaction?.mode, "capability_result");
  assert.equal(capabilityResult.interaction?.behavior, "wave.result");
  const resultMessages = capabilityResult.interaction?.response?.messages ?? [];
  assert(resultMessages.some((message) => message.items?.some((item) => item.type === "text" && item.text === "Wave completed.")));

  const asset = await runtime.assetById("tone");
  assert.deepEqual(asset, new TextEncoder().encode("GVYA runtime fixture asset\n"));
  const assetInfo = await runtime.assetInfoById("tone");
  assert.equal(assetInfo.logical_path, "assets/tone.bin");
  console.log("PASS runtime/SDK layer WASM/TS canonical runtime fixture");
} finally {
  await runtime.close();
}
