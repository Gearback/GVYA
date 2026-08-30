import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createStarterBrainWorkspace } from "../apps/studio/dist/workspace.js";
import { compilerSourceEntries, WasmCompilerBackend } from "../apps/studio/dist/compiler-wasm.js";
import { GvyaRuntime, unsignedDevelopmentOpenOptions, WasmRuntimeBackend } from "../packages/runtime-sdk/dist/index.js";

const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
const engineDir = resolve(root, "apps/studio/public/engine/v1");
const engineBytes = new Uint8Array(await readFile(resolve(engineDir, "gvya-ffi.wasm")));
const engineModule = await WebAssembly.compile(engineBytes);

async function runTurn(workspace, text) {
  const compiler = await WasmCompilerBackend.instantiate(engineModule);
  const artifact = compiler.compile(await compilerSourceEntries(workspace, []));
  assert.ok(artifact.byteLength > 0, "canonical compiler must emit a non-empty transient artifact");

  const backend = await WasmRuntimeBackend.instantiate(engineModule);
  const runtime = await GvyaRuntime.open(artifact, backend, unsignedDevelopmentOpenOptions());
  try {
    const result = await runtime.turn({
      format: "gvya.runtime.turn",
      version: 1,
      utterance: { text },
      context: {},
      seed: 1,
    });
    return { result, info: await runtime.info() };
  } finally {
    await runtime.close();
  }
}

function retargetStarter(workspace, meaningId, sample, language = "en-US") {
  const pkg = workspace.packages[0];
  const meaning = pkg.contents.meanings[0];
  meaning.id = meaningId;
  meaning.value.id = meaningId;
  meaning.value.samples = [{ language, text: sample }];
  meaning.value.negative_samples = [];
  meaning.value.retrieval_terms = [];

  const behavior = pkg.contents.behaviors[0];
  behavior.id = `${meaningId}.behavior`;
  behavior.value.id = behavior.id;
  behavior.value.meaning = meaningId;
  behavior.value.responses[0].id = `${meaningId}.response`;
  behavior.value.responses[0].texts = [{ language, variants: ["OK"] }];
  pkg.contents.regression_cases = [];
}

// Canonical smoke path.
{
  const workspace = createStarterBrainWorkspace();
  const { result, info } = await runTurn(workspace, "hello");
  assert.equal(info.project_id, workspace.project_id);
  assert.equal(info.brain_id, workspace.brain_id);
  assert.equal(result.format, "gvya.runtime.turn-result");
  assert.equal(result.meaning?.id, "greeting.hello");
  assert.equal(result.behavior, "greeting.hello.behavior");
  const texts = (result.response?.messages ?? [])
    .flatMap((message) => message.items ?? [])
    .filter((item) => item.type === "text")
    .map((item) => item.text);
  assert.ok(texts.includes("Hello.") || texts.includes("Hi. How can I help?"), "starter Bot must answer through the canonical runtime");
}

// Gen3 matcher acceptance: whole-phrase rescue must reach and score short multi-token typos.
{
  const workspace = createStarterBrainWorkspace();
  retargetStarter(workspace, "praise", "nice job");
  const { result } = await runTurn(workspace, "nic jbo");
  assert.equal(result.meaning?.id, "praise", "whole-phrase typo rescue must resolve nic jbo -> nice job");
}

// Three-character substitutions are intentionally not typo corrections.
{
  const workspace = createStarterBrainWorkspace();
  retargetStarter(workspace, "good.bot", "good bot");
  const { result } = await runTurn(workspace, "good boy");
  assert.notEqual(result.meaning?.id, "good.bot", "three-character semantic substitutions must not typo-match");
}

// fa-IR profile must fold Persian/Arabic digits to the authored ASCII form.
{
  const workspace = createStarterBrainWorkspace();
  workspace.languages = ["fa-IR"];
  workspace.enabled_languages = ["fa-IR"];
  workspace.default_language = "fa-IR";
  workspace.authoring_language = "fa-IR";
  workspace.packages[0].authoring_language = "fa-IR";

  // Starter workspaces deliberately carry no implicit lexical policy. This acceptance must
  // hydrate the real authored fa-IR Language/Matcher Profile; otherwise the compiler correctly
  // creates an empty profile and there is no Persian-digit rewrite to exercise.
  const languageProfile = JSON.parse(
    await readFile(resolve(root, "content/shared/language-profiles/fa-ir.json"), "utf8"),
  );
  const matcherProfile = JSON.parse(
    await readFile(resolve(root, "content/shared/matcher-profiles/fa-ir.json"), "utf8"),
  );
  workspace.matcher_profiles = [{
    language: "fa-IR",
    language_profile: languageProfile.profile,
    profile: matcherProfile.profile,
  }];

  retargetStarter(workspace, "version.two", "نسخه 2", "fa-IR");
  const { result } = await runTurn(workspace, "نسخه ۲");
  assert.equal(result.meaning?.id, "version.two", "fa-IR digit folding must resolve Persian digits against ASCII-authored samples");
}

// Retrieval is only an optimization: bounded exhaustive rescue must recover a strong sample hidden
// behind candidate_limit, without elevating the weak metadata-only distractors.
{
  const workspace = createStarterBrainWorkspace();
  const pkg = workspace.packages[0];
  workspace.semantic.candidate_limit = 32;
  pkg.contents.regression_cases = [];
  const meaningTemplate = structuredClone(pkg.contents.meanings[0]);
  const behaviorTemplate = structuredClone(pkg.contents.behaviors[0]);
  pkg.contents.meanings = [];
  pkg.contents.behaviors = [];

  for (let index = 0; index < 40; index += 1) {
    const id = `retrieval.distractor.${String(index).padStart(2, "0")}`;
    const meaning = structuredClone(meaningTemplate);
    meaning.id = id;
    meaning.value.id = id;
    meaning.value.samples = [{ language: "en-US", text: `alpha beta gamma delta distractor${index}` }];
    meaning.value.negative_samples = [{ language: "en-US", text: "alpha beta gamma delta" }];
    meaning.value.retrieval_terms = [];
    pkg.contents.meanings.push(meaning);

    const behavior = structuredClone(behaviorTemplate);
    behavior.id = `${id}.behavior`;
    behavior.value.id = behavior.id;
    behavior.value.meaning = id;
    behavior.value.responses[0].id = `${id}.response`;
    behavior.value.responses[0].texts = [{ language: "en-US", variants: ["distractor"] }];
    pkg.contents.behaviors.push(behavior);
  }

  const targetMeaning = structuredClone(meaningTemplate);
  targetMeaning.id = "sample.target";
  targetMeaning.value.id = "sample.target";
  targetMeaning.value.samples = [{ language: "en-US", text: "alpha gamma" }];
  targetMeaning.value.negative_samples = [];
  targetMeaning.value.retrieval_terms = [];
  pkg.contents.meanings.push(targetMeaning);

  const targetBehavior = structuredClone(behaviorTemplate);
  targetBehavior.id = "sample.target.behavior";
  targetBehavior.value.id = targetBehavior.id;
  targetBehavior.value.meaning = "sample.target";
  targetBehavior.value.responses[0].id = "sample.target.response";
  targetBehavior.value.responses[0].texts = [{ language: "en-US", variants: ["target"] }];
  pkg.contents.behaviors.push(targetBehavior);

  const { result } = await runTurn(workspace, "alpha beta gamma delta");
  assert.equal(result.meaning?.id, "sample.target", "bounded exhaustive sample rescue must recover the hidden strong sample");
  assert.equal(result.semantic?.candidate_pruning_reason, "exhaustive_sample_rescue");
}

console.log("Engine v1 acceptance: PASS (single Engine + Gen3 matcher rescue gates)");
