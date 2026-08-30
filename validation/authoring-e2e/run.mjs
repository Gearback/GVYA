import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../..");
const fixtures = join(here, "fixtures");
const wasmPath = join(root, "apps/studio/public/engine/v1/gvya-ffi.wasm");
const encoder = new TextEncoder();
const decoder = new TextDecoder();
const sourceMagic = encoder.encode("GVYASRC1");

const args = process.argv.slice(2);
let gvyaBin = process.env.GVYA_BIN || null;
let processProofOnly = false;
for (let index = 0; index < args.length; index += 1) {
  if (args[index] === "--gvya") {
    gvyaBin = args[index + 1] ?? null;
    index += 1;
  } else if (args[index] === "--source-proof") {
    // Source/runtime proof is the default when Engine WASM is available.
  } else if (args[index] === "--process-proof-only") {
    processProofOnly = true;
  } else {
    throw new Error(`unknown argument: ${args[index]}`);
  }
}

async function runSourceAndRuntimeProof(engineModule) {
  const compiler = await EngineCompiler.create(engineModule);
  const valid = [
    "00-base",
    "01-meaning-behavior-missing-proof",
    "01-meaning-behavior-accepted",
    "02-stateful-missing-proof",
    "02-stateful-accepted",
    "03-capability-missing-proof",
    "03-capability-accepted",
    "04-removal-missing-proof",
    "04-removal-accepted",
    "05-repaired",
    "06-failing-regression",
    "06-fixed-regression",
    "07-global-change",
    "08-sequential-a",
    "08-sequential-b",
  ];
  for (const name of valid) {
    const entries = sourceEntries(join(fixtures, name));
    compiler.validate(entries);
    const artifact = compiler.compile(entries);
    assert.ok(artifact.byteLength > 0, `${name}: compiler emitted empty artifact`);
  }

  const malformed = sourceEntries(join(fixtures, "05-malformed"));
  let malformedError = null;
  try { compiler.validate(malformed); } catch (error) { malformedError = error; }
  assert.ok(malformedError, "malformed candidate must be rejected by the canonical compiler");
  assert.match(String(malformedError), /source_invalid|invalid JSON|source\.invalid_json/i);

  await runtimeSmoke(engineModule, compiler, "01-meaning-behavior-accepted", async (runtime) => {
    const result = await runtime.turn(turnRequest("weather please", {}));
    assert.equal(result.meaning?.id, "weather.ask");
    assert.equal(result.behavior, "weather.ask.behavior");
    assert.deepEqual(responseIds(result), ["weather.ask.behavior.answer"]);
  });

  await runtimeSmoke(engineModule, compiler, "02-stateful-accepted", async (runtime) => {
    const first = await runtime.turn(turnRequest("start order", {}));
    assert.equal(first.behavior, "order.start.behavior");
    assert.equal(first.state?.author?.order?.started, true);
    assert.equal(first.state?.conversation?.active_topic?.id, "order.flow");
    assert.equal(first.state?.conversation?.active_followup?.id, "order.confirm.followup");
    const second = await runtime.turn(turnRequest("yes confirm", first.state));
    assert.equal(second.behavior, "order.confirm.behavior");
    assert.deepEqual(responseIds(second), ["order.confirm.behavior.answer"]);
  });

  await runtimeSmoke(engineModule, compiler, "03-capability-accepted", async (runtime) => {
    const context = { available_capabilities: [{ id: "lamp.set", version: "1" }] };
    const request = turnRequest("turn lamp on", {}, context);
    const proposed = await runtime.turn(request);
    const firstDecision = proposed.capabilities?.decisions?.[0];
    assert.equal(firstDecision?.capability, "lamp.set");
    assert.equal(firstDecision?.outcome?.type, "needs_confirmation");
    assert.equal(firstDecision?.outcome?.reason_code, "lamp.confirm");
    const proposal = firstDecision?.proposal;
    assert.ok(proposal?.id && proposal?.fingerprint, "capability proposal must expose id/fingerprint");

    // Confirmation replays the proposal against its pre-turn state, which is the canonical
    // test-driver behavior. This runner consumes Runtime behavior; it does not recreate policy.
    const confirmed = await runtime.turn({
      ...request,
      state: {},
      confirmations: [{
        id: "confirm-1",
        proposal_id: proposal.id,
        fingerprint: proposal.fingerprint,
        confirmed: true,
      }],
    });
    const admitted = confirmed.capabilities?.decisions?.find((row) => row?.outcome?.type === "admitted");
    assert.equal(admitted?.capability, "lamp.set");
    assert.ok(admitted?.proposal, "confirmed capability must be admitted with its proposal");

    const result = await runtime.capabilityResult({
      format: "gvya.runtime.capability-result",
      version: 1,
      proposal: admitted.proposal,
      result: { proposal_id: admitted.proposal.id, succeeded: true, output: { ok: true } },
      context: {},
      state: confirmed.state,
      seed: 1,
      confirmations: [],
    });
    assert.deepEqual(responseIds(result.interaction), ["lamp.set.result.answer"]);
  });

  await runtimeSmoke(engineModule, compiler, "04-removal-accepted", async (runtime) => {
    const first = await runtime.turn(turnRequest("start order", {}));
    assert.equal(first.behavior, "order.start.behavior");
    assert.equal(first.state?.conversation?.active_followup ?? null, null);
    const second = await runtime.turn(turnRequest("yes confirm", first.state));
    assert.equal(second.behavior, "order.confirm.behavior");
  });

  await runtimeSmoke(engineModule, compiler, "05-repaired", async (runtime) => {
    const result = await runtime.turn(turnRequest("help me", {}));
    assert.equal(result.behavior, "support.help.behavior");
  });

  console.log(`Authoring source/runtime proof: PASS (${valid.length} source snapshots validate+build, malformed source rejected, conversation/state/capability/removal/repaired runtime paths exercised)`);
}

function runAuthorStepProcessProof(binary) {
  proveFromZeroAuthoring(binary);
  const cases = [
    ["00-base", "01-meaning-behavior-missing-proof", false, "repair_required", "add_direct_mechanic_proof"],
    ["00-base", "01-meaning-behavior-accepted", true, "ready_to_promote", "promote_candidate"],
    ["01-meaning-behavior-accepted", "02-stateful-missing-proof", false, "repair_required", "add_direct_mechanic_proof"],
    ["01-meaning-behavior-accepted", "02-stateful-accepted", true, "ready_to_promote", "promote_candidate"],
    ["02-stateful-accepted", "03-capability-missing-proof", false, "repair_required", "add_direct_mechanic_proof"],
    ["02-stateful-accepted", "03-capability-accepted", true, "ready_to_promote", "promote_candidate"],
    ["03-capability-accepted", "04-removal-missing-proof", false, "repair_required", null],
    ["03-capability-accepted", "04-removal-accepted", true, "ready_to_promote", "promote_candidate"],
    ["04-removal-accepted", "05-malformed", false, "repair_required", "resolve_candidate_source_failure"],
    ["04-removal-accepted", "05-repaired", true, "ready_to_promote", "promote_candidate"],
    ["05-repaired", "06-failing-regression", false, "repair_required", "resolve_regression_failure"],
    ["05-repaired", "06-fixed-regression", true, "ready_to_promote", "promote_candidate"],
    ["06-fixed-regression", "07-global-change", true, "ready_to_promote", "promote_candidate"],
  ];

  for (const [base, candidate, accepted, state, requiredAction] of cases) {
    const report = authorStep(binary, fixture(base), fixture(candidate), accepted);
    assert.equal(report.format, "gvya.cli.author-step", `${base} -> ${candidate}: outer format`);
    assert.equal(report.version, 1, `${base} -> ${candidate}: outer version`);
    assert.equal(report.accepted, accepted, `${base} -> ${candidate}: accepted`);
    assert.equal(report.state, state, `${base} -> ${candidate}: state`);
    assert.equal(report.promotion_allowed, accepted, `${base} -> ${candidate}: promotion_allowed`);
    if (requiredAction) {
      assert.ok(actionKinds(report).includes(requiredAction), `${base} -> ${candidate}: missing ${requiredAction}`);
    }
    if (candidate === "05-malformed") {
      assert.equal(report.gate, null);
      assert.equal(report.candidate_policy, "repair_candidate_only");
      continue;
    }

    assert.equal(report.gate?.format, "gvya.cli.check-change");
    assert.equal(report.gate?.version, 1);
    if (candidate !== "07-global-change") {
      assert.equal(report.gate?.impact?.full_suite_required, false, `${base} -> ${candidate}: bounded local change must stay incremental`);
    }
    if (candidate.includes("missing-proof")) {
      assert.equal(report.gate?.impact?.mechanic_proof_missing, true, `${base} -> ${candidate}: direct mechanic proof must be required`);
      assert.ok((report.gate?.impact?.mechanic_coverage?.missing ?? 0) > 0, `${base} -> ${candidate}: missing proof count must be visible`);
    }
    if (accepted) {
      assert.equal(report.gate?.impact?.mechanic_proof_missing, false, `${base} -> ${candidate}: accepted candidate cannot have missing mechanic proof`);
      assert.equal(report.gate?.impact?.mechanic_coverage?.missing, 0, `${base} -> ${candidate}: accepted candidate mechanic coverage must be complete`);
    }
  }

  const global = authorStep(binary, fixture("06-fixed-regression"), fixture("07-global-change"), true);
  assert.equal(global.gate?.impact?.full_suite_required, true, "global semantic config change must escalate to full suite");
  assert.ok(global.gate?.impact?.full_suite_reasons?.includes("semantic_config_changed"));

  proveHostOwnedSequentialPromotion(binary);
  console.log("Authoring process proof: PASS (from-zero scaffold, local incremental repair/proof, global escalation, immutable BASE, host-owned promotion)");
}

function proveFromZeroAuthoring(binary) {
  const work = mkdtempSync(join(tmpdir(), "gvya-authoring-zero-"));
  try {
    const base = join(work, "base");
    const init = spawnSync(binary, [
      "init", "bot", base,
      "--project-id", "zero-project",
      "--bot-id", "zero-bot",
      "--package-id", "zero.core",
      "--languages", "en",
      "--enabled-languages", "en",
      "--default-language", "en",
    ], { encoding: "utf8", maxBuffer: 4 * 1024 * 1024 });
    assert.equal(init.status, 0, `from-zero init failed: ${init.stderr}`);
    const initReport = JSON.parse(init.stdout);
    assert.equal(initReport.format, "gvya.cli.init");
    assert.equal(initReport.version, 1);

    const check = spawnSync(binary, ["check", base], { encoding: "utf8", maxBuffer: 8 * 1024 * 1024 });
    assert.equal(check.status, 0, `fresh scaffold did not pass canonical check: ${check.stderr}`);
    const checkReport = JSON.parse(check.stdout);
    assert.equal(checkReport.format, "gvya.cli.check");
    assert.equal(checkReport.accepted, true);

    const baseBefore = treeHash(base);
    const candidate = join(work, "candidate");
    cpSync(base, candidate, { recursive: true });
    const packagePath = join(candidate, "packages", "standard", "zero.core", "package.json");
    const packageDocument = JSON.parse(readFileSync(packagePath, "utf8"));
    const fragmentRoot = join(dirname(packagePath), "fragments");
    const meaningRelative = "fragments/meanings/zero.hello.json";
    const behaviorRelative = "fragments/behaviors/zero.hello.behavior.json";
    const regressionRelative = "fragments/regression_cases/zero.hello.proof.json";
    mkdirSync(join(fragmentRoot, "meanings"), { recursive: true });
    mkdirSync(join(fragmentRoot, "behaviors"), { recursive: true });
    mkdirSync(join(fragmentRoot, "regression_cases"), { recursive: true });
    writeFileSync(join(dirname(packagePath), meaningRelative), `${JSON.stringify({ id: "zero.hello", value: { id: "zero.hello", samples: [{ language: "en", text: "hello zero" }] } }, null, 2)}\n`);
    writeFileSync(join(dirname(packagePath), behaviorRelative), `${JSON.stringify({ id: "zero.hello.behavior", value: { id: "zero.hello.behavior", meaning: "zero.hello", responses: [{ id: "zero.hello.answer", texts: [{ language: "en", variants: ["Hello from zero"] }] }] } }, null, 2)}\n`);
    writeFileSync(join(dirname(packagePath), regressionRelative), `${JSON.stringify({ id: "zero.hello.proof", value: { id: "zero.hello.proof", description: "Direct first-slice proof", input: "hello zero", language: "en", context: {}, initial_state: {}, seed: 1, expectation: { meaning: "zero.hello", response_ids: ["zero.hello.answer"] }, generated: false } }, null, 2)}\n`);
    packageDocument.fragments.meanings = [...(packageDocument.fragments.meanings ?? []), meaningRelative];
    packageDocument.fragments.behaviors = [...(packageDocument.fragments.behaviors ?? []), behaviorRelative];
    packageDocument.fragments.regression_cases = [...(packageDocument.fragments.regression_cases ?? []), regressionRelative];
    writeFileSync(packagePath, `${JSON.stringify(packageDocument, null, 2)}\n`);

    const firstSlice = authorStep(binary, base, candidate, true);
    assert.equal(firstSlice.state, "ready_to_promote");
    assert.equal(firstSlice.promotion_allowed, true);
    assert.equal(firstSlice.gate?.impact?.full_suite_required, false, "first bounded slice from an empty scaffold must stay incremental");
    assert.equal(firstSlice.gate?.impact?.mechanic_proof_missing, false);
    assert.equal(firstSlice.gate?.impact?.mechanic_coverage?.missing, 0);
    assert.ok((firstSlice.gate?.impact?.mechanic_coverage?.required ?? 0) >= 2, "first slice must prove semantic resolution and behavior response");
    assert.equal(treeHash(base), baseBefore, "author-step mutated the fresh accepted scaffold");
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

function proveHostOwnedSequentialPromotion(binary) {
  const work = mkdtempSync(join(tmpdir(), "gvya-authoring-promotion-"));
  try {
    const accepted0 = join(work, "accepted-0");
    cpSync(fixture("06-fixed-regression"), accepted0, { recursive: true });
    const accepted0Before = treeHash(accepted0);

    const firstCandidate = fixture("08-sequential-a");
    const first = authorStep(binary, accepted0, firstCandidate, true);
    assert.equal(first.state, "ready_to_promote");
    assert.equal(treeHash(accepted0), accepted0Before, "author-step mutated accepted BASE before promotion");

    const accepted1 = join(work, "accepted-1");
    cpSync(firstCandidate, accepted1, { recursive: true }); // host-owned promotion
    const accepted1Before = treeHash(accepted1);
    const secondCandidate = fixture("08-sequential-b");
    const second = authorStep(binary, accepted1, secondCandidate, true);
    assert.equal(second.state, "ready_to_promote");
    assert.equal(treeHash(accepted1), accepted1Before, "second author-step mutated promoted BASE before next promotion");

    const accepted2 = join(work, "accepted-2");
    cpSync(secondCandidate, accepted2, { recursive: true });
    const noChange = authorStep(binary, accepted2, accepted2, true);
    assert.equal(noChange.state, "no_change");
    assert.equal(noChange.promotion_allowed, false);
    assert.ok(actionKinds(noChange).includes("keep_baseline"));
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

function authorStep(binary, base, candidate, expectSuccess) {
  const run = spawnSync(binary, ["author-step", base, candidate, "--json"], {
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
  });
  const stdout = run.stdout?.trim() ?? "";
  assert.ok(stdout, `${basename(base)} -> ${basename(candidate)}: author-step emitted no JSON`);
  let report;
  try { report = JSON.parse(stdout); }
  catch (error) { throw new Error(`author-step emitted invalid JSON: ${stdout}\n${error}`); }
  if (expectSuccess) {
    assert.equal(run.status, 0, `expected accepted candidate; stderr=${run.stderr}`);
  } else {
    assert.notEqual(run.status, 0, "expected repair-required candidate to return non-zero");
  }
  return report;
}

function actionKinds(report) {
  return (report.next_actions ?? []).map((row) => row?.kind).filter((value) => typeof value === "string");
}

function fixture(name) { return join(fixtures, name); }

function treeHash(directory) {
  const hash = createHash("sha256");
  for (const file of walkFiles(directory)) {
    hash.update(relative(directory, file).split(sep).join("/"));
    hash.update("\0");
    hash.update(readFileSync(file));
    hash.update("\0");
  }
  return hash.digest("hex");
}

function sourceEntries(directory) {
  return walkFiles(directory).map((file) => ({
    path: relative(directory, file).split(sep).join("/"),
    bytes: new Uint8Array(readFileSync(file)),
  }));
}

function walkFiles(directory) {
  const output = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const target = join(directory, entry.name);
    if (entry.isDirectory()) output.push(...walkFiles(target));
    else if (entry.isFile()) output.push(target);
  }
  return output.sort((left, right) => Buffer.from(relative(directory, left)).compare(Buffer.from(relative(directory, right))));
}

function packSourceArchive(entries) {
  const sorted = [...entries].sort((left, right) => Buffer.from(left.path).compare(Buffer.from(right.path)));
  let length = 12;
  for (const entry of sorted) {
    entry.pathBytes = encoder.encode(entry.path);
    length += 8 + entry.pathBytes.length + entry.bytes.length;
  }
  const archive = new Uint8Array(length);
  archive.set(sourceMagic, 0);
  const view = new DataView(archive.buffer);
  let offset = 8;
  view.setUint32(offset, sorted.length, true); offset += 4;
  for (const entry of sorted) {
    view.setUint32(offset, entry.pathBytes.length, true); offset += 4;
    view.setUint32(offset, entry.bytes.length, true); offset += 4;
    archive.set(entry.pathBytes, offset); offset += entry.pathBytes.length;
    archive.set(entry.bytes, offset); offset += entry.bytes.length;
  }
  return archive;
}

class WasmAbi {
  constructor(exports) {
    this.exports = exports;
    assert.equal(exports.gvya_abi_version(), 1);
    assert.equal(exports.gvya_pointer_width(), 32);
    assert.equal(exports.gvya_buffer_struct_size(), 12);
  }
  alloc(length) {
    const ptr = this.exports.gvya_alloc(length);
    if (length > 0 && ptr === 0) throw new Error("GVYA WASM allocation failed");
    return ptr;
  }
  copyIn(bytes) {
    const ptr = this.alloc(bytes.length);
    if (bytes.length) new Uint8Array(this.exports.memory.buffer, ptr, bytes.length).set(bytes);
    return ptr;
  }
  takeBuffer(outPtr) {
    const view = new DataView(this.exports.memory.buffer);
    const ptr = view.getUint32(outPtr, true);
    const len = view.getUint32(outPtr + 4, true);
    const capacity = view.getUint32(outPtr + 8, true);
    if (len > capacity) throw new Error("GVYA returned corrupt buffer metadata");
    const bytes = ptr === 0 || len === 0 ? new Uint8Array() : new Uint8Array(new Uint8Array(this.exports.memory.buffer, ptr, len));
    this.exports.gvya_buffer_free(outPtr);
    return bytes;
  }
}

class EngineCompiler extends WasmAbi {
  static async create(module) {
    const instance = await WebAssembly.instantiate(module, {});
    return new EngineCompiler(instance.exports);
  }
  validate(entries) { this.#call(this.exports.gvya_compiler_validate_source_tree, packSourceArchive(entries), false); }
  compile(entries) { return this.#call(this.exports.gvya_compiler_build_source_tree, packSourceArchive(entries), true); }
  #call(fn, archive, returnBytes) {
    const archivePtr = this.copyIn(archive);
    const outPtr = this.alloc(12);
    try {
      const code = fn(archivePtr, archive.length, outPtr);
      const output = this.takeBuffer(outPtr);
      if (code !== 0) throw new Error(`GVYA compiler code ${code}: ${decoder.decode(output)}`);
      return returnBytes ? output : undefined;
    } finally {
      this.exports.gvya_dealloc(archivePtr, archive.length);
      this.exports.gvya_dealloc(outPtr, 12);
    }
  }
}

class EngineRuntime extends WasmAbi {
  static async open(module, artifact) {
    const instance = await WebAssembly.instantiate(module, {});
    const runtime = new EngineRuntime(instance.exports);
    const options = encoder.encode(JSON.stringify({ format: "gvya.runtime.open-options", version: 1, signature: { mode: "allow_unsigned" } }));
    const artifactPtr = runtime.copyIn(artifact);
    const optionsPtr = runtime.copyIn(options);
    const handlePtr = runtime.alloc(8);
    const outPtr = runtime.alloc(12);
    try {
      const code = runtime.exports.gvya_runtime_open_with_options_json(artifactPtr, artifact.length, optionsPtr, options.length, handlePtr, outPtr);
      const output = runtime.takeBuffer(outPtr);
      if (code !== 0) throw new Error(`GVYA runtime open code ${code}: ${decoder.decode(output)}`);
      runtime.handle = new DataView(runtime.exports.memory.buffer).getBigUint64(handlePtr, true);
      assert.notEqual(runtime.handle, 0n);
      return runtime;
    } finally {
      runtime.exports.gvya_dealloc(artifactPtr, artifact.length);
      runtime.exports.gvya_dealloc(optionsPtr, options.length);
      runtime.exports.gvya_dealloc(handlePtr, 8);
      runtime.exports.gvya_dealloc(outPtr, 12);
    }
  }
  turn(request) { return this.#jsonCall(this.exports.gvya_runtime_turn_json, request); }
  capabilityResult(request) { return this.#jsonCall(this.exports.gvya_runtime_capability_result_json, request); }
  close() {
    if (this.handle !== null) {
      assert.equal(this.exports.gvya_runtime_close(this.handle), 0);
      this.handle = null;
    }
  }
  #jsonCall(fn, value) {
    const input = encoder.encode(JSON.stringify(value));
    const inputPtr = this.copyIn(input);
    const outPtr = this.alloc(12);
    try {
      const code = fn(this.handle, inputPtr, input.length, outPtr);
      const output = this.takeBuffer(outPtr);
      if (code !== 0) throw new Error(`GVYA runtime code ${code}: ${decoder.decode(output)}`);
      return JSON.parse(decoder.decode(output));
    } finally {
      this.exports.gvya_dealloc(inputPtr, input.length);
      this.exports.gvya_dealloc(outPtr, 12);
    }
  }
}

async function runtimeSmoke(module, compiler, fixtureName, callback) {
  const artifact = compiler.compile(sourceEntries(fixture(fixtureName)));
  const runtime = await EngineRuntime.open(module, artifact);
  try { await callback(runtime); }
  finally { runtime.close(); }
}

function turnRequest(text, state, context = {}) {
  return {
    format: "gvya.runtime.turn",
    version: 1,
    utterance: { text },
    context,
    state,
    seed: 1,
    confirmations: [],
  };
}

function responseIds(result) {
  return (result.response?.messages ?? []).map((message) => message?.source_response).filter(Boolean);
}

if (!processProofOnly) {
  const wasmBytes = readFileSync(wasmPath);
  const engineModule = await WebAssembly.compile(wasmBytes);
  await runSourceAndRuntimeProof(engineModule);
}
if (gvyaBin) {
  runAuthorStepProcessProof(resolve(gvyaBin));
} else if (processProofOnly) {
  throw new Error("--process-proof-only requires --gvya <path> or GVYA_BIN");
} else {
  console.log("Authoring process proof: SKIP (pass --gvya <path> or GVYA_BIN to execute canonical CLI transitions)");
}
