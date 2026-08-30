import { GvyaRuntime, requireSignedArtifactOptions, unsignedDevelopmentOpenOptions } from "../packages/runtime-sdk/dist/index.js";

function assert(condition, message) { if (!condition) throw new Error(message); }

const development = unsignedDevelopmentOpenOptions();
assert(development.signature.mode === "allow_unsigned", "development policy must explicitly permit unsigned artifacts");
const signed = requireSignedArtifactOptions();
assert(signed.signature.mode === "require_present", "production helper must reject unsigned artifacts");

let seen = null;
let seenTurn = null;
const backend = {
  async open(_artifact, options) { seen = options; },
  async info() { throw new Error("unused"); },
  async capabilities() { return { format: "gvya.runtime.capabilities", version: 1, capabilities: [] }; },
  async capabilityInfo(id) { return { format: "gvya.runtime.capability-info", version: 1, capability: { id } }; },
  async turn(request) { seenTurn = request; return { format: "gvya.runtime.turn-result", version: 1, state: {}, capabilities: {} }; },
  async openConversation() { throw new Error("unused"); },
  async capabilityResult() { throw new Error("unused"); },
  async assetByPath() { throw new Error("unused"); },
  async assetById() { throw new Error("unused"); },
  async assetInfoById() { throw new Error("unused"); },
  async close() {},
};
const policy = {
  format: "gvya.runtime.open-options",
  version: 1,
  artifact_limits: { max_total_bytes: 1024 },
  program_limits: { max_program_bytes: 4096, max_depth: 8, max_nodes: 256 },
  signature: {
    mode: "require_verified",
    preverified: {
      content_root: "0".repeat(64),
      algorithm: "test-v1",
      key_id: "test-key",
      signature: "opaque-signature",
    },
  },
};
const runtime = await GvyaRuntime.open(new Uint8Array([1, 2, 3]), backend, policy);
assert(seen === policy, "runtime SDK did not preserve explicit open policy");
assert((await runtime.capabilities()).format === "gvya.runtime.capabilities", "runtime capability catalog passthrough drifted");
assert((await runtime.capabilityInfo("demo.wave")).capability.id === "demo.wave", "runtime capability detail passthrough drifted");
const originalTurn = { format: "gvya.runtime.turn", version: 1, utterance: { text: "wave" }, seed: 7, context: { available_capabilities: [{ id: "demo.wave", version: "1" }] } };
const proposal = { id: "p-1", capability: "demo.wave", capability_version: "1", arguments: {}, fingerprint: "fp-1", trace_id: "trace-1" };
await runtime.confirmTurn(originalTurn, proposal, true, "confirm-1");
assert(seenTurn.utterance.text === "wave" && seenTurn.seed === 7, "confirmation retry did not preserve the exact authored turn request");
assert(seenTurn.confirmations.length === 1 && seenTurn.confirmations[0].proposal_id === "p-1" && seenTurn.confirmations[0].fingerprint === "fp-1" && seenTurn.confirmations[0].confirmed === true, "confirmation retry did not bind the exact proposal fingerprint");
await runtime.close();
console.log("Runtime SDK open-policy contract: PASS");
