import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { readContentSnapshot } from "../apps/studio/content-host.mjs";
import { decodeContent } from "../apps/studio/dist/studio-content.js";
import { resolveSelectedBrain } from "../apps/studio/dist/studio-model.js";
import { compilerSourceEntries, WasmCompilerBackend } from "../apps/studio/dist/compiler-wasm.js";
import { GvyaRuntime, unsignedDevelopmentOpenOptions, WasmRuntimeBackend } from "../packages/runtime-sdk/dist/index.js";

const HELP_PACKAGE_ID = "gvya-project.gvya-bot.bot";
const normalizeLanguage = (value) => value.toLowerCase();
const qualityOnly = process.argv.includes("--quality-only");

const contentRoot = fileURLToPath(new URL("../content", import.meta.url));
const snapshot = await readContentSnapshot(contentRoot);
const studio = decodeContent(snapshot.entries).workspace;
const brain = resolveSelectedBrain(studio);
const helpPackage = brain.packages.find((row) => row.manifest.id === HELP_PACKAGE_ID);
assert.ok(helpPackage, "resolved GVYA Bot must include its Bot Package");
assert.ok(
  brain.enabled_languages.length > 0,
  "the help acceptance target must derive at least one enabled language from Project Matcher Profiles",
);
assert.deepEqual(
  brain.packages.map((row) => row.manifest.id),
  ["core.smalltalk.formal", HELP_PACKAGE_ID, "gvya-project.fallback.formal"],
  "the help acceptance target must include the authored Smalltalk and Fallback graph",
);

const fixture = JSON.parse(await readFile(new URL("./fixtures/gvya-help-blind.json", import.meta.url), "utf8"));
assert.equal(fixture.format, "gvya.validation.blind-conversation");
assert.equal(fixture.version, 1);
assert.equal(fixture.bot_id, "gvya-bot");
const secondFixture = JSON.parse(await readFile(new URL("./fixtures/gvya-help-blind-2.json", import.meta.url), "utf8"));
assert.equal(secondFixture.format, "gvya.validation.blind-conversation");
assert.equal(secondFixture.version, 1);
assert.equal(secondFixture.bot_id, "gvya-bot");
const thirdFixture = JSON.parse(await readFile(new URL("./fixtures/gvya-help-blind-3.json", import.meta.url), "utf8"));
assert.equal(thirdFixture.format, "gvya.validation.blind-conversation");
assert.equal(thirdFixture.version, 1);
assert.equal(thirdFixture.bot_id, "gvya-bot");
const qualityFixture = JSON.parse(await readFile(new URL("./fixtures/gvya-help-authoring-quality.json", import.meta.url), "utf8"));
assert.equal(qualityFixture.format, "gvya.validation.authoring-quality");
assert.equal(qualityFixture.version, 1);
assert.equal(qualityFixture.bot_id, "gvya-bot");
assert.ok(qualityFixture.domain_smoke.length >= 8, "authoring quality fixture must keep a domain-noun smoke surface");
assert.ok(qualityFixture.boundary_cases.length >= 4, "authoring quality fixture must keep close-boundary/confounder coverage");
assert.ok(qualityFixture.off_domain_precision.length >= 4, "authoring quality fixture must keep obvious off-domain precision coverage");
assert.ok(qualityFixture.sessions.length >= 3, "authoring quality fixture must keep multi-turn recovery/promise/repeat coverage");
const languageCalibrationFixture = JSON.parse(await readFile(new URL("./fixtures/gvya-help-language-calibration.json", import.meta.url), "utf8"));
assert.equal(languageCalibrationFixture.format, "gvya.validation.language-calibration");
assert.equal(languageCalibrationFixture.version, 1);
assert.equal(languageCalibrationFixture.bot_id, "gvya-bot");
assert.ok(languageCalibrationFixture.useful.length >= 24, "language calibration must keep balanced bilingual recall coverage");
assert.ok(languageCalibrationFixture.confounders.length >= 24, "language calibration must keep balanced bilingual precision coverage");
const reportedFixture = JSON.parse(await readFile(new URL("./fixtures/gvya-help-reported-blind.json", import.meta.url), "utf8"));
assert.equal(reportedFixture.format, "gvya.validation.reported-blind-conversation");
assert.equal(reportedFixture.version, 1);
assert.equal(reportedFixture.bot_id, "gvya-bot");

const engineBytes = new Uint8Array(await readFile(new URL("../apps/studio/public/engine/v1/gvya-ffi.wasm", import.meta.url)));
const engineModule = await WebAssembly.compile(engineBytes);
const compiler = await WasmCompilerBackend.instantiate(engineModule);
const artifact = compiler.compile(await compilerSourceEntries(brain, []));
const runtime = await GvyaRuntime.open(artifact, await WasmRuntimeBackend.instantiate(engineModule), unsignedDevelopmentOpenOptions());

const failures = [];
const localizedHelpResponseIds = new Map();
for (const contribution of helpPackage.contents.behaviors) {
  const behavior = contribution.value;
  for (const response of behavior.responses) {
    const languages = new Set();
    for (const row of response.texts) languages.add(normalizeLanguage(row.language));
    for (const extra of response.extra_messages ?? []) {
      for (const row of extra.texts) languages.add(normalizeLanguage(row.language));
    }
    for (const language of languages) {
      const key = `${behavior.meaning}|${language}`;
      const values = localizedHelpResponseIds.get(key) ?? new Set();
      values.add(response.id);
      localizedHelpResponseIds.set(key, values);
    }
  }
}

const summarizeSemantic = (turn) => ({
  decision: turn.semantic?.decision ?? null,
  scores: (turn.semantic?.scores ?? []).slice(0, 3).map((row) => ({
    meaning: row.meaning,
    pattern_index: row.pattern_index,
    score: row.score,
    evidence_tier: row.evidence_tier,
    match_kind: row.match_kind,
    rejected_reason: row.rejected_reason,
  })),
});

const responseTexts = (turn) => (turn.response?.messages ?? [])
  .flatMap((message) => message.items ?? [])
  .map((item) => item.text)
  .filter((text) => typeof text === "string");

const responseIds = (turn) => [...new Set(
  (turn.response?.messages ?? [])
    .map((message) => message.source_response)
    .filter((id) => typeof id === "string"),
)];

async function evaluate(row, state, group) {
  const turn = await runtime.turn({
    format: "gvya.runtime.turn",
    version: 1,
    utterance: { text: row.input },
    context: row.context ?? {},
    state,
    seed: row.seed ?? 1,
  });
  const actualMeaning = turn.meaning?.id ?? null;
  const actualBehavior = turn.behavior ?? null;
  const actualFollowup = turn.state?.conversation?.active_followup?.id ?? null;
  let reason = null;
  if (row.meaning !== undefined && actualMeaning !== row.meaning) {
    reason = `expected meaning ${row.meaning ?? "none"}, got ${actualMeaning ?? "none"}`;
  } else if (row.forbidden_meanings?.includes(actualMeaning)) {
    reason = `resolved forbidden meaning ${actualMeaning}`;
  } else if (row.behavior && actualBehavior !== row.behavior) {
    reason = `expected behavior ${row.behavior}, got ${actualBehavior ?? "none"}`;
  } else if (row.conversation_mode && turn.mode !== row.conversation_mode) {
    reason = `expected conversation mode ${row.conversation_mode}, got ${turn.mode ?? "none"}`;
  } else if (row.active_followup !== undefined && actualFollowup !== row.active_followup) {
    reason = `expected active follow-up ${row.active_followup ?? "none"}, got ${actualFollowup ?? "none"}`;
  } else if (row.response_ids?.length > 0 && JSON.stringify(responseIds(turn)) !== JSON.stringify(row.response_ids)) {
    reason = `expected response ids ${JSON.stringify(row.response_ids)}, got ${JSON.stringify(responseIds(turn))}`;
  } else if (row.meaning?.startsWith("gvya.help.")) {
    const language = normalizeLanguage(row.language);
    const expectedResponseIds = localizedHelpResponseIds.get(`${row.meaning}|${language}`);
    const actualIds = responseIds(turn);
    if (!expectedResponseIds || actualIds.length === 0 || actualIds.some((id) => !expectedResponseIds.has(id))) {
      reason = `response provenance was not authored for ${row.language}`;
    }
    // Fixture language selects the authored localization expected to exist; it is not host input.
    // Runtime v1 owns conversation-language detection and state.
  }
  if (reason) {
    failures.push({
      group,
      id: row.id,
      input: row.input,
      language: row.language,
      reason,
      actual_behavior: actualBehavior,
      actual_followup: actualFollowup,
      semantic: summarizeSemantic(turn),
    });
  }
  return turn.state;
}

let authoredRegressionPassed = 0;
let authoredScenarioPassed = 0;
let blindPassed = 0;
let secondBlindPassed = 0;
let thirdBlindPassed = 0;
let confounderPassed = 0;
let blindSessionPassed = 0;
let qualityDomainPassed = 0;
let qualityBoundaryPassed = 0;
let qualityOffDomainPassed = 0;
let qualitySessionPassed = 0;
let languageCalibrationUsefulPassed = 0;
let languageCalibrationConfounderPassed = 0;
let reportedCasePassed = 0;
let reportedOffDomainPassed = 0;
let reportedSessionPassed = 0;

try {
  if (!qualityOnly) {
  for (const contribution of helpPackage.contents.regression_cases) {
    const test = contribution.value;
    const before = failures.length;
    await evaluate({
      id: test.id,
      input: test.input,
      language: test.language,
      meaning: test.expectation.meaning || undefined,
      forbidden_meanings: test.expectation.forbidden_meanings,
      conversation_mode: test.expectation.conversation_mode,
      active_followup: test.expectation.active_followup || undefined,
      response_ids: test.expectation.response_ids,
      context: test.context,
      seed: test.seed,
    }, test.initial_state, "authored-regression");
    if (failures.length === before) authoredRegressionPassed += 1;
  }

  for (const contribution of helpPackage.contents.scenarios) {
    const scenario = contribution.value;
    const before = failures.length;
    let state = scenario.initial_state;
    for (let index = 0; index < scenario.steps.length; index += 1) {
      const step = scenario.steps[index];
      assert.equal(step.type, "turn", `GVYA help authored scenario ${scenario.id} currently uses turn interactions only`);
      state = await evaluate({
        id: `${scenario.id}.step.${index + 1}`,
        input: step.say,
        language: step.language,
        meaning: step.expectation.meaning || undefined,
        forbidden_meanings: step.expectation.forbidden_meanings,
        conversation_mode: step.expectation.conversation_mode,
        active_followup: step.expectation.active_followup || undefined,
        response_ids: step.expectation.response_ids,
        context: step.context ?? scenario.context,
        seed: step.seed,
      }, state, "authored-scenario");
      if (failures.length > before) break;
    }
    if (failures.length === before) authoredScenarioPassed += 1;
  }

  for (const row of fixture.cases) {
    const before = failures.length;
    await evaluate(row, undefined, "blind");
    if (failures.length === before) blindPassed += 1;
  }

  for (const row of secondFixture.cases) {
    const before = failures.length;
    await evaluate(row, undefined, "blind-2");
    if (failures.length === before) secondBlindPassed += 1;
  }

  for (const row of thirdFixture.cases) {
    const before = failures.length;
    await evaluate(row, undefined, "blind-3");
    if (failures.length === before) thirdBlindPassed += 1;
  }

  for (const row of fixture.confounders) {
    const before = failures.length;
    await evaluate(row, undefined, "confounder");
    if (failures.length === before) confounderPassed += 1;
  }

  for (const session of fixture.sessions) {
    const before = failures.length;
    let state;
    for (let index = 0; index < session.turns.length; index += 1) {
      state = await evaluate({ ...session.turns[index], id: `${session.id}.turn.${index + 1}` }, state, "blind-session");
      if (failures.length > before) break;
    }
    if (failures.length === before) blindSessionPassed += 1;
  }
  }

  for (const row of qualityFixture.domain_smoke) {
    const before = failures.length;
    await evaluate(row, undefined, "quality-domain-smoke");
    if (failures.length === before) qualityDomainPassed += 1;
  }

  for (const row of qualityFixture.boundary_cases) {
    const before = failures.length;
    await evaluate(row, undefined, "quality-boundary");
    if (failures.length === before) qualityBoundaryPassed += 1;
  }

  for (const row of qualityFixture.off_domain_precision) {
    const before = failures.length;
    await evaluate(row, undefined, "quality-off-domain");
    if (failures.length === before) qualityOffDomainPassed += 1;
  }

  for (const session of qualityFixture.sessions) {
    const before = failures.length;
    let state;
    for (let index = 0; index < session.turns.length; index += 1) {
      state = await evaluate({ ...session.turns[index], id: `${session.id}.turn.${index + 1}` }, state, "quality-session");
      if (failures.length > before) break;
    }
    if (failures.length === before) qualitySessionPassed += 1;
  }

  for (const row of languageCalibrationFixture.useful) {
    const before = failures.length;
    await evaluate(row, undefined, "language-calibration-useful");
    if (failures.length === before) languageCalibrationUsefulPassed += 1;
  }

  for (const row of languageCalibrationFixture.confounders) {
    const before = failures.length;
    await evaluate(row, undefined, "language-calibration-confounder");
    if (failures.length === before) languageCalibrationConfounderPassed += 1;
  }

  for (const row of reportedFixture.cases) {
    const before = failures.length;
    await evaluate(row, undefined, "reported-blind");
    if (failures.length === before) reportedCasePassed += 1;
  }

  for (const row of reportedFixture.off_domain) {
    const before = failures.length;
    await evaluate(row, undefined, "reported-off-domain");
    if (failures.length === before) reportedOffDomainPassed += 1;
  }

  for (const session of reportedFixture.sessions) {
    const before = failures.length;
    let state;
    for (let index = 0; index < session.turns.length; index += 1) {
      state = await evaluate({ ...session.turns[index], id: `${session.id}.turn.${index + 1}` }, state, "reported-session");
      if (failures.length > before) break;
    }
    if (failures.length === before) reportedSessionPassed += 1;
  }
} finally {
  await runtime.close();
}

const report = {
  mode: qualityOnly ? "quality-only" : "full",
  authored_regressions: { passed: authoredRegressionPassed, total: qualityOnly ? 0 : helpPackage.contents.regression_cases.length },
  authored_scenarios: { passed: authoredScenarioPassed, total: qualityOnly ? 0 : helpPackage.contents.scenarios.length },
  blind_cases: { passed: blindPassed, total: qualityOnly ? 0 : fixture.cases.length },
  blind_cases_2: { passed: secondBlindPassed, total: qualityOnly ? 0 : secondFixture.cases.length },
  blind_cases_3: { passed: thirdBlindPassed, total: qualityOnly ? 0 : thirdFixture.cases.length },
  confounders: { passed: confounderPassed, total: qualityOnly ? 0 : fixture.confounders.length },
  blind_sessions: { passed: blindSessionPassed, total: qualityOnly ? 0 : fixture.sessions.length },
  authoring_quality: {
    domain_smoke: { passed: qualityDomainPassed, total: qualityFixture.domain_smoke.length },
    boundary_cases: { passed: qualityBoundaryPassed, total: qualityFixture.boundary_cases.length },
    off_domain_precision: { passed: qualityOffDomainPassed, total: qualityFixture.off_domain_precision.length },
    sessions: { passed: qualitySessionPassed, total: qualityFixture.sessions.length },
  },
  language_calibration: {
    useful: { passed: languageCalibrationUsefulPassed, total: languageCalibrationFixture.useful.length },
    confounders: { passed: languageCalibrationConfounderPassed, total: languageCalibrationFixture.confounders.length },
  },
  reported_blind: {
    cases: { passed: reportedCasePassed, total: reportedFixture.cases.length },
    off_domain: { passed: reportedOffDomainPassed, total: reportedFixture.off_domain.length },
    sessions: { passed: reportedSessionPassed, total: reportedFixture.sessions.length },
  },
  failures,
};

if (failures.length > 0) {
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  process.exitCode = 1;
} else if (qualityOnly) {
  console.log(`GVYA help authoring quality: PASS (${qualityDomainPassed}/${qualityFixture.domain_smoke.length} domain + ${qualityBoundaryPassed}/${qualityFixture.boundary_cases.length} boundary + ${qualityOffDomainPassed}/${qualityFixture.off_domain_precision.length} off-domain + ${qualitySessionPassed}/${qualityFixture.sessions.length} sessions + calibration ${languageCalibrationUsefulPassed}/${languageCalibrationFixture.useful.length} useful + ${languageCalibrationConfounderPassed}/${languageCalibrationFixture.confounders.length} confounders + reported ${reportedCasePassed}/${reportedFixture.cases.length} cases + ${reportedOffDomainPassed}/${reportedFixture.off_domain.length} off-domain + ${reportedSessionPassed}/${reportedFixture.sessions.length} sessions)`);
} else {
  console.log(`GVYA help Bot contract: PASS (${authoredRegressionPassed} authored regressions, ${authoredScenarioPassed} authored scenarios, ${blindPassed} first blind cases, ${secondBlindPassed} second blind cases, ${thirdBlindPassed} third blind cases, ${confounderPassed} confounders, ${blindSessionPassed} blind sessions, quality ${qualityDomainPassed}/${qualityFixture.domain_smoke.length} domain + ${qualityBoundaryPassed}/${qualityFixture.boundary_cases.length} boundary + ${qualityOffDomainPassed}/${qualityFixture.off_domain_precision.length} off-domain + ${qualitySessionPassed}/${qualityFixture.sessions.length} sessions)`);
}
