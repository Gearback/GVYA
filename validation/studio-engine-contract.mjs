import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { readContentSnapshot } from "../apps/studio/content-host.mjs";
import { createPackage, createStarterBrainWorkspace } from "../apps/studio/dist/workspace.js";
import { decodeContent } from "../apps/studio/dist/studio-content.js";
import { addPackageToBot, botMissingMatcherLanguages, botPackageEligibility, botSelectablePackages, projectAvailableLanguages, resolvePackagePreview, resolveSelectedBrain, selectedBot, selectedProject, setBotFallbackPackage } from "../apps/studio/dist/studio-model.js";
import { GvyaRuntime, unsignedDevelopmentOpenOptions, WasmRuntimeBackend } from "../packages/runtime-sdk/dist/index.js";
import { compilerSourceEntries, packSourceArchive, WasmCompilerBackend } from "../apps/studio/dist/compiler-wasm.js";
import { STUDIO_ENGINE_VERSION } from "../apps/studio/dist/engine-assets.js";
import { StudioRuntimeSession, studioLocalSystemValues } from "../apps/studio/dist/runtime-simulator.js";

const DEFAULT_FALLBACK_PACKAGE_IDS = { formal: "gvya.fallback.formal", informal: "gvya.fallback.informal" };
const DEFAULT_SMALLTALK_PACKAGE_IDS = { formal: "core.smalltalk.formal", informal: "core.smalltalk.informal" };
const defaultContent = decodeContent((await readContentSnapshot(fileURLToPath(new URL("../content", import.meta.url)))).entries);
const createDefaultStudio = () => {
  const studio = structuredClone(defaultContent.workspace);
  const project = studio.projects.find((row) => row.id === "gvya-project");
  const bot = project?.bots.find((row) => row.id === "gvya-bot");
  assert.ok(project && bot, "physical content must include the starter Project and Bot");
  studio.projects = [project];
  project.bots = [bot];
  project.packages = [];
  bot.package_ids = [];
  bot.fallback_package_id = null;
  studio.selectedProjectId = project.id;
  studio.selectedBotId = bot.id;
  studio.selectedPackageScope = "bot";
  studio.selectedPackageId = bot.package.manifest.id;
  return studio;
};

const workspace = createStarterBrainWorkspace();
const entries = await compilerSourceEntries(workspace, []);
const packedA = packSourceArchive(entries);
const packedB = packSourceArchive([...entries].reverse());
assert.deepEqual(packedA, packedB, "compiler source archive must be deterministic independent of caller order");
assert.equal(new TextDecoder().decode(packedA.subarray(0, 8)), "GVYASRC1");
assert.equal(new DataView(packedA.buffer, packedA.byteOffset, packedA.byteLength).getUint32(8, true), entries.length);
assert.equal(STUDIO_ENGINE_VERSION, "v1");

// An empty Package is valid compiler input. Draft authoring objects, not empty Packages, caused the historical Simulate failure.
const emptyWorkspace = structuredClone(workspace);
const emptyPackage = createPackage("empty.bot");
emptyWorkspace.packages = [emptyPackage];
emptyWorkspace.selectedPackageId = emptyPackage.manifest.id;
const engineBytes = new Uint8Array(await readFile(new URL("../apps/studio/public/engine/v1/gvya-ffi.wasm", import.meta.url)));
const engineModule = await WebAssembly.compile(engineBytes);
const compiler = await WasmCompilerBackend.instantiate(engineModule);
assert.ok(compiler.compile(await compilerSourceEntries(emptyWorkspace, [])).byteLength > 0, "canonical compiler must accept a completely empty Package");

const assetWorkspace = structuredClone(workspace);
assetWorkspace.packages[0].contents.assets.push({ id: "asset.greeting", exported: true, mode: "add", value: { id: "asset.greeting", media_type: "text/plain", logical_path: "assets/greeting.txt", source: "assets/greeting.txt" } });
const assetEntries = await compilerSourceEntries(assetWorkspace, [{ owner_key: "test:conversation", package_id: "conversation", source: "assets/greeting.txt", media_type: "text/plain", blob: new Blob(["hello asset"]) }]);
assert.deepEqual(new TextDecoder().decode(assetEntries.find((entry) => entry.path === "packages/conversation/assets/greeting.txt")?.bytes), "hello asset", "Studio must materialize package-owned asset bytes at the compiler-relative path");
assert.ok(compiler.compile(assetEntries).byteLength > 0, "canonical compiler must accept a Studio Bot with binary asset bytes");

const invalidWorkspace = structuredClone(workspace);
invalidWorkspace.packages[0].contents.meanings[0].value.samples = [];
await assert.rejects(
  async () => compiler.compile(await compilerSourceEntries(invalidWorkspace, [])),
  /conversation\/meaning\/greeting\.hello: Meaning has no positive matching evidence \[semantic\.positive_evidence_missing\]/u,
  "compiler audit failures must expose a precise path and stable audit code to Studio",
);

// Prove the actual Studio composition path: Bot + managed Shared Package -> compiler -> runtime -> chat.
let studioWorkspace = createDefaultStudio();
studioWorkspace = addPackageToBot(studioWorkspace, DEFAULT_SMALLTALK_PACKAGE_IDS.formal);
studioWorkspace = setBotFallbackPackage(studioWorkspace, DEFAULT_FALLBACK_PACKAGE_IDS.formal);
const selectedBrain = resolveSelectedBrain(studioWorkspace);
assert.deepEqual(selectedBrain.matcher_profiles.map((row) => row.language), ["en-US", "fa-IR"], "active Bot languages must select matcher profiles without Package dependencies");
const formalFallback = selectedBrain.packages.find((pkg) => pkg.manifest.id === DEFAULT_FALLBACK_PACKAGE_IDS.formal);
assert.ok(formalFallback, "the selected default formal Fallback Package must resolve into the Bot compile target");
const formalEnglish = new Set(formalFallback.contents.fallback_behaviors.flatMap((behavior) => behavior.value.responses.flatMap((response) => response.texts.find((row) => row.language === "en-US")?.variants ?? [])));
const formalPersian = new Set(formalFallback.contents.fallback_behaviors.flatMap((behavior) => behavior.value.responses.flatMap((response) => response.texts.find((row) => row.language === "fa-IR")?.variants ?? [])));
const artifact = compiler.compile(await compilerSourceEntries(selectedBrain, []));
const runtime = await GvyaRuntime.open(artifact, await WasmRuntimeBackend.instantiate(engineModule), unsignedDevelopmentOpenOptions());
try {
  const info = await runtime.info();
  assert.deepEqual(info.enabled_languages, ["en-US", "fa-IR"]);
  assert.equal(info.default_language, "en-US");
  const turn = await runtime.turn({ format: "gvya.runtime.turn", version: 1, utterance: { text: "hello" }, context: {}, seed: 1 });
  assert.equal(turn.meaning?.id, `${DEFAULT_SMALLTALK_PACKAGE_IDS.formal}.hello`);
  assert.equal(turn.behavior, `${DEFAULT_SMALLTALK_PACKAGE_IDS.formal}.hello.behavior`);
  assert.equal(turn.response.messages[0].items[0].language, "en-us", "the English sample match must select an English response");
  const neutral = await runtime.turn({ format: "gvya.runtime.turn", version: 1, utterance: { text: "--#--" }, context: {}, seed: 2 });
  assert.equal(neutral.behavior, `${DEFAULT_FALLBACK_PACKAGE_IDS.formal}.unresolved.stage-1`);
  assert.ok(formalEnglish.has(neutral.response.messages[0].items[0].text));
  assert.equal(neutral.state.conversation.active_language, "en-us");
  const repeated = await runtime.turn({ format: "gvya.runtime.turn", version: 1, utterance: { text: "--#--" }, state: neutral.state, context: {}, seed: 3 });
  assert.equal(repeated.behavior, `${DEFAULT_FALLBACK_PACKAGE_IDS.formal}.repeat`);
  assert.ok(formalEnglish.has(repeated.response.messages[0].items[0].text));
  const secondUnresolved = await runtime.turn({ format: "gvya.runtime.turn", version: 1, utterance: { text: "%%" }, state: neutral.state, context: {}, seed: 4 });
  assert.equal(secondUnresolved.behavior, `${DEFAULT_FALLBACK_PACKAGE_IDS.formal}.unresolved.stage-2`);
  assert.ok(formalEnglish.has(secondUnresolved.response.messages[0].items[0].text));
  const thirdUnresolved = await runtime.turn({ format: "gvya.runtime.turn", version: 1, utterance: { text: "@@" }, state: secondUnresolved.state, context: {}, seed: 5 });
  assert.equal(thirdUnresolved.behavior, `${DEFAULT_FALLBACK_PACKAGE_IDS.formal}.unresolved.stage-3`);
  assert.ok(formalEnglish.has(thirdUnresolved.response.messages[0].items[0].text));
  const persianMatch = await runtime.turn({ format: "gvya.runtime.turn", version: 1, utterance: { text: "\u0633\u0644\u0627\u0645" }, context: {}, seed: 3 });
  assert.equal(persianMatch.meaning?.id, `${DEFAULT_SMALLTALK_PACKAGE_IDS.formal}.hello`);
  assert.equal(persianMatch.response.messages[0].items[0].language, "fa-ir", "the Persian sample match must select a Persian response");
  assert.equal(persianMatch.state.conversation.active_language, "fa-ir", "the matched Persian evidence must switch the session language");
  const persian = await runtime.turn({ format: "gvya.runtime.turn", version: 1, utterance: { text: "--#--" }, state: persianMatch.state, context: {}, seed: 4 });
  assert.ok(formalPersian.has(persian.response.messages[0].items[0].text), "fallback must retain the language of the last resolved sample match");
  assert.equal(persian.state.conversation.active_language, "fa-ir");
  const continued = await runtime.turn({ format: "gvya.runtime.turn", version: 1, utterance: { text: "%%" }, state: persian.state, context: {}, seed: 4 });
  assert.equal(continued.behavior, `${DEFAULT_FALLBACK_PACKAGE_IDS.formal}.unresolved.stage-2`);
  assert.ok(formalPersian.has(continued.response.messages[0].items[0].text));
  assert.equal(continued.state.conversation.active_language, "fa-ir");
} finally {
  await runtime.close();
}

const packageStudio = createDefaultStudio();
packageStudio.selectedPackageScope = "shared";
packageStudio.selectedPackageId = DEFAULT_SMALLTALK_PACKAGE_IDS.formal;
const packagePreview = resolvePackagePreview(packageStudio);
assert.deepEqual(packagePreview.packages.map((pkg) => pkg.manifest.id), [DEFAULT_SMALLTALK_PACKAGE_IDS.formal]);
assert.ok(!packagePreview.packages.some((pkg) => pkg.manifest.id === packageStudio.projects[0].bots[0].package.manifest.id));
const packageArtifact = compiler.compile(await compilerSourceEntries(packagePreview, []));
const packageRuntime = await GvyaRuntime.open(packageArtifact, await WasmRuntimeBackend.instantiate(engineModule), unsignedDevelopmentOpenOptions());
try {
  const turn = await packageRuntime.turn({ format: "gvya.runtime.turn", version: 1, utterance: { text: "hello" }, context: {}, seed: 1 });
  assert.equal(turn.behavior, `${DEFAULT_SMALLTALK_PACKAGE_IDS.formal}.hello.behavior`);
} finally {
  await packageRuntime.close();
}

const smalltalkStudio = createDefaultStudio();
smalltalkStudio.selectedPackageScope = "shared";
smalltalkStudio.selectedPackageId = DEFAULT_SMALLTALK_PACKAGE_IDS.formal;
const smalltalkPreview = resolvePackagePreview(smalltalkStudio);
assert.deepEqual(smalltalkPreview.packages.map((pkg) => pkg.manifest.id), [DEFAULT_SMALLTALK_PACKAGE_IDS.formal]);
const smalltalkArtifact = compiler.compile(await compilerSourceEntries(smalltalkPreview, []));
const fixedStudioTime = new Date(2026, 7, 27, 18, 5, 0, 0);
const fixedStudioFacts = studioLocalSystemValues(fixedStudioTime, "en-US");
assert.deepEqual(Object.keys(fixedStudioFacts).sort(), ["dateLong", "dayOfWeek", "hour", "minute", "month", "monthName", "partOfDay", "season", "time", "time12", "unix_time_ms", "year"]);
assert.equal(fixedStudioFacts.dayOfWeek, "Thursday");
assert.equal(fixedStudioFacts.monthName, "August");
assert.equal(fixedStudioFacts.season, "summer");
assert.equal(fixedStudioFacts.partOfDay, "evening");
assert.equal(fixedStudioFacts.hour, "18");
assert.equal(fixedStudioFacts.minute, "05");
const smalltalkRuntime = await GvyaRuntime.open(smalltalkArtifact, await WasmRuntimeBackend.instantiate(engineModule), unsignedDevelopmentOpenOptions());
try {
  const english = await smalltalkRuntime.turn({ format: "gvya.runtime.turn", version: 1, utterance: { text: "thanks" }, context: {}, seed: 1 });
  assert.equal(english.meaning?.id, `${DEFAULT_SMALLTALK_PACKAGE_IDS.formal}.thanks`);
  assert.equal(english.behavior, `${DEFAULT_SMALLTALK_PACKAGE_IDS.formal}.thanks.behavior`);
  const persian = await smalltalkRuntime.turn({ format: "gvya.runtime.turn", version: 1, utterance: { text: "ممنون" }, state: english.state, context: {}, seed: 2 });
  assert.equal(persian.meaning?.id, `${DEFAULT_SMALLTALK_PACKAGE_IDS.formal}.thanks`);
  assert.equal(persian.behavior, `${DEFAULT_SMALLTALK_PACKAGE_IDS.formal}.thanks.behavior`);
  assert.equal(persian.state.conversation.active_language, "fa-ir");
  const colloquialEnglish = await smalltalkRuntime.turn({ format: "gvya.runtime.turn", version: 1, utterance: { text: "hows it going" }, state: persian.state, context: {}, seed: 3 });
  assert.equal(colloquialEnglish.meaning?.id, `${DEFAULT_SMALLTALK_PACKAGE_IDS.formal}.how-are-you`);
  assert.equal(colloquialEnglish.state.conversation.active_language, "en-us");
  const colloquialPersian = await smalltalkRuntime.turn({ format: "gvya.runtime.turn", version: 1, utterance: { text: "\u062e\u0648\u0628\u064a" }, state: colloquialEnglish.state, context: {}, seed: 4 });
  assert.equal(colloquialPersian.meaning?.id, `${DEFAULT_SMALLTALK_PACKAGE_IDS.formal}.how-are-you`);
  assert.equal(colloquialPersian.state.conversation.active_language, "fa-ir");
} finally {
  await smalltalkRuntime.close();
}

const studioSmalltalkSession = new StudioRuntimeSession();
await studioSmalltalkSession.open(engineModule, smalltalkArtifact);
try {
  const context = { values: {}, availableCapabilities: [], visibleReferences: [] };
  const temporalCases = [
    ["what time is it", "what-time-is-it"],
    ["what day is it", "what-day-is-it"],
    ["what date is it", "what-date-is-it"],
    ["what month is it", "what-month-is-it"],
    ["what year is it", "what-year-is-it"],
    ["is it morning", "is-it-morning"],
    ["what season is it", "what-season-is-it"],
    ["what hour is it", "what-hour-is-it"],
  ];
  for (const [utterance, meaning] of temporalCases) {
    const turn = await studioSmalltalkSession.turn(utterance, context, 1);
    assert.equal(turn.meaning?.id, `${DEFAULT_SMALLTALK_PACKAGE_IDS.formal}.${meaning}`);
    const responseText = turn.response.messages.flatMap((message) => message.items.map((item) => item.text ?? "")).join(" ");
    assert.ok(responseText.length > 0 && !responseText.includes("unavailable"), `Studio Simulate must provide local system facts for ${meaning}`);
  }
  const persianDay = await studioSmalltalkSession.turn("امروز چه روزیه", context, 1);
  assert.equal(persianDay.meaning?.id, `${DEFAULT_SMALLTALK_PACKAGE_IDS.formal}.what-day-is-it`);
  assert.equal(persianDay.state.conversation.active_language, "fa-ir", "Studio must infer Persian from the matched sample without a language control");
  const persianTime = await studioSmalltalkSession.turn("ساعت چنده", context, 1);
  assert.equal(persianTime.meaning?.id, `${DEFAULT_SMALLTALK_PACKAGE_IDS.formal}.what-time-is-it`);
  const persianTimeText = persianTime.response.messages.flatMap((message) => message.items.map((item) => item.text ?? "")).join(" ");
  assert.ok(persianTimeText.length > 0 && !persianTimeText.includes("نامشخص"), "Studio local clock facts must follow the active Persian session language");
} finally {
  await studioSmalltalkSession.close();
}

for (const packageId of [...Object.values(DEFAULT_FALLBACK_PACKAGE_IDS), DEFAULT_SMALLTALK_PACKAGE_IDS.informal]) {
  const previewStudio = createDefaultStudio();
  previewStudio.selectedPackageScope = "shared";
  previewStudio.selectedPackageId = packageId;
  const preview = resolvePackagePreview(previewStudio);
  assert.deepEqual(preview.packages.map((pkg) => pkg.manifest.id), [packageId]);
  assert.ok(compiler.compile(await compilerSourceEntries(preview, [])).byteLength > 0, `${packageId} must compile as an isolated default Package preview`);
}

const app = await readFile(new URL("../apps/studio/src/App.tsx", import.meta.url), "utf8");
const behaviorViews = await readFile(new URL("../apps/studio/src/studio-behavior-views.tsx", import.meta.url), "utf8");
assert.match(behaviorViews, /setDraftWorkspace/u, "New Behavior authoring must use an ephemeral Studio draft rather than canonical source");
assert.match(behaviorViews, /saveBlocked/u, "Behavior Save/Create must be gated by blocking in-modal validation");
assert.doesNotMatch(behaviorViews, /Changes stay in this modal until validation passes and you save\./u, "Behavior modal footers must not place descriptions beside their actions");
const runtimeViews = await readFile(new URL("../apps/studio/src/studio-runtime-views.tsx", import.meta.url), "utf8");
const managementViews = await readFile(new URL("../apps/studio/src/studio-management-views.tsx", import.meta.url), "utf8");
const navigation = await readFile(new URL("../apps/studio/src/studio-navigation.tsx", import.meta.url), "utf8");
const simulateStart = runtimeViews.indexOf("function SimulateView");
const simulateEnd = runtimeViews.indexOf("function RuntimeResult", simulateStart);
assert.ok(simulateStart >= 0 && simulateEnd > simulateStart);
const simulate = runtimeViews.slice(simulateStart, simulateEnd);
assert.doesNotMatch(simulate, /type=["']file["']/u, "Simulate must not expose file pickers");
assert.doesNotMatch(simulate, /Runtime WASM|Compiled artifact|Load runtime \+ artifact/u, "Simulate must not expose engine plumbing");
assert.match(simulate, /StudioSimulationEngine/u);
assert.match(simulate, /Send/u);
assert.match(simulate, /simulation-transcript/u, "Simulate must present a real multi-turn chat transcript");
assert.doesNotMatch(simulate, /props\.workspace\.languages\[0\]/u, "Simulate must not infer runtime preference from Project language order");
assert.doesNotMatch(simulate, /useState\(["']en["']\)/u, "Simulate must not assume a language");
assert.doesNotMatch(simulate, /Session language|setLanguage/u, "Simulate must not expose a manual conversational language control");

const buildStart = runtimeViews.indexOf("function BuildView");
assert.ok(buildStart >= 0);
const build = runtimeViews.slice(buildStart);
assert.doesNotMatch(build, /gvya build|cargo build/u, "normal Studio Build must use bundled Engine assets, not shell compilation");
assert.match(build, /loadBundledEngineAssets/u);
assert.match(build, /RUNTIME_EXPORTERS\.map/u, "Build must render runtime export actions from the drop-in registry");
assert.match(build, /Included Packages/u, "Build must summarize the human-relevant resolved Package set");
assert.match(build, /totalBehaviors/u, "Build must show the composed Behavior total");
assert.doesNotMatch(build, /Export compiler source|Source files|sourceFiles\(/u, "human Build must not expose compiler-source plumbing or a raw file inventory");

const runtimeExporterRegistry = await readFile(new URL("../apps/studio/src/runtime-exporters.ts", import.meta.url), "utf8");
const webRuntimeExporter = await readFile(new URL("../apps/studio/src/runtime-exporters/web.runtime-exporter.ts", import.meta.url), "utf8");
const godotRuntimeExporter = await readFile(new URL("../apps/studio/src/runtime-exporters/godot.runtime-exporter.ts", import.meta.url), "utf8");
assert.match(runtimeExporterRegistry, /import\.meta\.glob/u, "runtime exporters must be discovered rather than hand-registered in Build");
assert.match(runtimeExporterRegistry, /\.\/runtime-exporters\/\*\.runtime-exporter\.ts/u);
assert.match(webRuntimeExporter, /label: "Web runtime"/u);
assert.match(godotRuntimeExporter, /label: "Godot runtime"/u);

assert.doesNotMatch(app, /function PackageSelector|aria-label=["']Selected package["']/u, "Package editors must never switch packages from an inline selector");
assert.match(navigation, /\[\["bot","Overview"\],\["bot-packages","Packages"\],\["simulate","Simulate"\],\["bot-settings","Settings"\],\["build","Build"\]\]/u, "Bot context must place the final Build action after Settings");
assert.match(navigation, /\["package-simulate","Simulate"\]/u, "Every Package context must expose isolated Simulate");
assert.match(app, /resolvePackagePreview/u, "Package Simulate must resolve a Package-rooted transient compile view");
assert.match(app, /navigateWithStudio\(selectBotWorkspace\(studio, botId\), "bot", "bot"\)/u, "opening a Bot must land on its Overview rather than a package authoring tab");
assert.match(managementViews, /function BotPackagesView/u, "Bots must expose a first-class Packages page");
assert.match(app, /window\.history\.pushState/u, "Studio navigation must create real browser history entries");
assert.match(app, /popstate/u, "Studio must restore navigation on browser Back\/Forward");
assert.match(navigation, /project-packages/u, "Project Packages must be a distinct history location rather than hidden component state");
assert.match(navigation, /breadcrumb-provenance/u, "Package breadcrumbs must expose actual ownership\/override provenance");

// Studio package eligibility must predict the canonical compiler exactly: every Package it calls
// eligible compiles as this Bot, and every Package it rejects really does fail to compile.
{
  const compilerBackend = await WasmCompilerBackend.instantiate(engineModule);
  const compileAs = async (studio) => {
    const brain = resolveSelectedBrain(studio);
    try {
      compilerBackend.compile(await compilerSourceEntries(brain, defaultContent.assetFiles));
      return null;
    } catch (error) { return String(error); }
  };
  const narrowBot = (studio, languages) => {
    const next = structuredClone(studio);
    const bot = next.projects[0].bots[0];
    bot.enabled_languages = projectAvailableLanguages(next.projects[0]).filter((row) => languages.some((want) => want.toLowerCase() === row.toLowerCase()));
    bot.default_language = bot.enabled_languages[0];
    return next;
  };
  // The Bot Package is part of the same closure and obeys the same rule, so isolate the variable
  // under test by starting from a Bot whose own Package authors nothing.
  const emptyBotPackage = (studio) => {
    const next = structuredClone(studio);
    const contents = next.projects[0].bots[0].package.contents;
    for (const namespace of Object.keys(contents)) contents[namespace] = [];
    next.projects[0].bots[0].package.manifest.dependencies = [];
    return next;
  };

  const bilingual = addPackageToBot(emptyBotPackage(createDefaultStudio()), DEFAULT_SMALLTALK_PACKAGE_IDS.formal);
  const bilingualRow = botPackageEligibility(bilingual, selectedProject(bilingual), selectedBot(bilingual), "standard")
    .find((row) => row.package.manifest.id === DEFAULT_SMALLTALK_PACKAGE_IDS.formal);
  assert.equal(bilingualRow.eligible, true, "a Bot enabling both Project languages may use the bilingual Smalltalk Package");
  assert.equal(await compileAs(bilingual), null, "an eligible Package must actually compile");

  // Same source, Bot narrowed to en-US. Studio must refuse it, and the compiler must agree.
  const narrowed = narrowBot(bilingual, ["en-US"]);
  assert.ok(
    !botSelectablePackages(narrowed, selectedProject(narrowed), selectedBot(narrowed)).some((pkg) => pkg.manifest.id === DEFAULT_SMALLTALK_PACKAGE_IDS.formal),
    "Studio must refuse a Package whose matcher evidence the Bot cannot compile",
  );
  const narrowedFailure = await compileAs(narrowed);
  assert.ok(narrowedFailure, "the canonical compiler must reject the very selection Studio refuses");
  // Uncovered matcher evidence fails at whichever gate reaches it first — the structural matcher
  // audit for Meaning `patterns`, IR construction for samples. Both are the missing Semantic
  // Profile the eligibility rule names, so the rule is pinned to the contract, not to one stage.
  assert.match(
    narrowedFailure,
    /Matcher Profile|structural matcher|semantic|Canonical IR compilation failed/iu,
    "the refused selection must fail on the language/profile contract the eligibility rule checks",
  );

  // Fallback response text in a non-enabled language stays compilable, so it stays eligible.
  const fallbackNarrowed = setBotFallbackPackage(narrowBot(emptyBotPackage(createDefaultStudio()), ["en-US"]), DEFAULT_FALLBACK_PACKAGE_IDS.formal);
  const fallbackRow = botPackageEligibility(fallbackNarrowed, selectedProject(fallbackNarrowed), selectedBot(fallbackNarrowed), "fallback")
    .find((row) => row.package.manifest.id === DEFAULT_FALLBACK_PACKAGE_IDS.formal);
  assert.equal(fallbackRow.eligible, true, "fa-IR fallback response text needs no fa-IR Semantic Profile");
  assert.equal(await compileAs(fallbackNarrowed), null, "an eligible bilingual Fallback Package compiles for an EN-only Bot");

  // A Bot Package that authors fa-IR matcher evidence disables its own Bot under the same rule.
  const bilingualBotPackage = narrowBot(createDefaultStudio(), ["en-US"]);
  assert.ok(botMissingMatcherLanguages(bilingualBotPackage).includes("fa-IR"), "the Bot Package is inside the closure the rule checks");
  assert.ok(await compileAs(bilingualBotPackage), "a Bot whose own Package needs a disabled language must not compile");
}

const engine = await readFile(new URL("../apps/studio/src/engine-assets.ts", import.meta.url), "utf8");
assert.match(engine, /sha256/u);
assert.match(engine, /searchParams\.set\("sha256", row\.sha256\)/u, "Engine binary cache keys must include the manifest digest so a rebuilt pre-freeze Engine cannot reuse stale browser bytes");
assert.match(engine, /fetchBinary\(freshUrl, "no-store"\)/u, "Engine loader must bypass browser caches once when verified cached bytes do not match the bundled manifest");
assert.match(engine, /matchesBinaryManifest/u, "cached and fresh Engine bytes must both pass the same integrity check");
assert.match(engine, /cached = null/u, "a failed Engine load must be retryable without reloading the whole Studio");
assert.match(engine, /WebAssembly\.compile/u, "Studio must precompile verified Engine assets once and reuse modules");
assert.match(engine, /engineModule/u);
assert.match(engine, /engineWasm/u);
assert.match(engine, /const module = binary\(value\.module\)/u);
assert.doesNotMatch(engine, /compilerModule|runtimeModule/u, "Studio must load one canonical Engine module, not a split compiler/runtime pair");
const compilerEdge = await readFile(new URL("../crates/gvya-ffi/src/compiler.rs", import.meta.url), "utf8");
assert.match(compilerEdge, /build_source_project/u, "browser compiler edge must call the canonical Rust compiler");
assert.doesNotMatch(compilerEdge, /matching|matcher|conversation::engine/u, "compiler edge must not duplicate domain semantics");

console.log("Studio Engine contract: PASS (single v1 Engine module, deterministic source transport, zero-setup Simulate, canonical Rust compiler edge)");
