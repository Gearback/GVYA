# GVYA Machine Authoring Architecture

## Product boundary

GVYA is a machine-authorable conversational-bot format and deterministic toolchain. Its transparent source is analogous to the editable document behind a compiled product: humans may edit it in Studio, while capable external agents may create or edit it directly. The compiled `.gvya` artifact is produced only by the canonical Rust compiler.

Studio is one optional human client. It is not an agent host and is not the API an agent must automate.

## Canonical authoring flow

```text
PDF / specification / dataset / conversation brief
                     ↓
external agent extracts, reasons, and authors
                     ↓
gvya.source.project/1 + gvya.source.package/1 + declared assets
                     ↓
canonical CLI: schema / init / check-package / check-change / author-step / check / inspect / analysis / audit / test / turn
                     ↓
canonical Rust compiler
                     ↓
portable .gvya artifact
```

PDF parsing, OCR, web access, model inference, prompt construction, tool selection, memory, retries, token/cost accounting, and human approval belong to the external agent environment. Only the resulting source and intentionally declared assets enter GVYA.

## Machine interface

The complete supported machine interface is:

- the compiler-owned machine-readable source contract exposed by `gvya schema --json` / `gvya schema --kind KIND --json`, with prose detail in `SOURCE_FORMAT.md` and `SCHEMA_PROFILE.md`;
- ordinary filesystem operations over a source tree;
- bounded structured diagnostics and observations from the canonical `gvya` CLI;
- the deterministic compiler output defined in `COMPILER_PIPELINE.md` and `ARTIFACT_FORMAT.md`.

There is no separate AI request schema, response schema, patch dialect, model registry, provider transport, or authoring-session format. An agent edits normal source transactionally using its own host tools, then asks the canonical implementation to validate semantics. Source control or the agent host supplies review and rollback when desired.

Agents designing Meaning boundaries, samples, negative evidence, responses, and blind conversation evaluations follow `PACKAGE_AUTHORING_RECIPE.md`. That recipe explains how the canonical matcher treats evidence; it does not introduce an alternate authoring format.

### Editing a portable Studio Project

The portable `content/` tree does not hide Package source behind a Studio wrapper. Shared and Project Packages use this shape:

```text
packages/
  standard/<package-id>/
    package.json       # canonical gvya.source.package/1 manifest + fragment index
    fragments/         # explicitly declared one-contribution JSON files
    authoring.json     # optional human-only authoring-language metadata
    assets/...
  fallback/<package-id>/
    package.json       # canonical gvya.source.package/1 manifest + fragment index
    fragments/         # explicitly declared one-contribution JSON files
    authoring.json     # optional human-only authoring-language metadata
    assets/...
```

The containing Shared or Project scope also has paired `language-profiles/<normalized-language>.json` and `matcher-profiles/<normalized-language>.json`. Language Profiles own normalization/morphology/lexical policy; Matcher Profiles own structural `pattern_sets`. These standalone pairs are the scope's complete available-language catalog. An external agent may create or edit either profile or `package.json` directly according to `SOURCE_FORMAT.md`; Studio discovers and validates the pair on reload. `authoring.json` is optional and is not compiler input. `project.json` describes Project identity only. Each `bots/<bot-id>/bot.json` explicitly stores the enabled/default languages, installed Package root IDs, selected Fallback Package, debug flag, and complete semantic/conversation configuration. Bots never reference Shared scope directly: selecting Shared content installs a complete copy into the Project first, so copying one Project folder preserves the source and profile pairs the agent inspected.

Removing either half of a Language/Matcher Profile pair is deliberately non-destructive. Studio does not rewrite Package text or Bot language settings: affected Packages and Bots remain readable as Disabled objects whose Overview names the missing languages. A disabled Package is not offered in Standard or Fallback Bot selection lists.

For a new drop-in Package, the agent may point the canonical CLI directly at the intended folder:

```text
gvya init package content/shared/packages/standard/example.answers --kind standard --authoring-language en-US
gvya check-package content/shared/packages/standard/example.answers
```

`init` refuses every existing output directory. Studio discovers the resulting canonical `package.json`; the optional sidecar remains human-only. `check-package` infers the Package's authored language catalog, constructs an isolated in-memory compile target, and runs the same resolver, composition, audit, compiler, runtime, and authored tests used for a complete Bot. Dependency-bearing Packages must be checked through a complete Bot source root so their explicit dependency graph is present.

For a new canonical one-Bot source target:

```text
gvya init bot ./support-bot --project-id support --bot-id assistant --languages en-US,fa-IR --enabled-languages en-US --default-language en-US
gvya check ./support-bot
```

The scaffold contains `gvya.project.json`, a complete empty core Package, and explicit deterministic Bot settings. One canonical source root remains one resolved Bot compile target; this does not introduce a second Studio workspace format.

The repository's portable reference Project follows the same rule: `content/gvya.project.json` is the canonical CLI root and declares the actual nested Studio Project/Shared Package and Language/Matcher Profile paths. `gvya inspect content --json`, `gvya analysis content --json`, and `gvya turn content --request REQUEST.json` therefore operate on the reference Bot without a Studio-only reader, copied Package graph, or alternate manifest shape.

Studio persists that root as an owned canonical file derived from the currently selected Bot, including its exact Package, Fallback Package, Language/Matcher Profile pair, language, and effective configuration paths/values. Loading fails when the root and selected Bot disagree, and saving rewrites the same canonical target instead of silently dropping it. This is one source contract shared by Studio and CLI, not a compatibility reader for the Studio manifests.

`gvya.cli.analysis/2` separates authored expectation coverage from natural-language discoverability. `expectation_coverage` reports which Meanings, Behaviors, and Capabilities are named by authored Regression Cases and Scenarios. `discoverability` deterministically removes identity terms shared by `project_id` and `brain_id` from bounded semantic samples, reruns those name-free probes through the canonical semantic kernel, and lists Meanings that are no longer reachable. These generated probes are diagnostic only: they reveal likely user-language gaps but never satisfy manual mechanic proof. `repair_boundaries` evaluates manual `repair_continuation` expectations against the configured repair floor and normal resolution threshold and flags observations within the configured fragile margin. Ranked score rows carry stable Meaning IDs, so an agent does not need to reconstruct `pattern_index` ownership.

## Expected agent loop

1. Discover the compiler-owned source shape with `gvya schema --json` and `gvya schema --kind KIND --json`; read the prose contracts only for semantic/domain detail not represented by the shallow field inventory.
2. Inspect the existing source tree, or create one root `gvya.project.json` and its package documents.
3. Convert the user's material into one bounded vertical conversation slice in an ordinary candidate source tree. Keep the last accepted source snapshot immutable.
4. Run `gvya author-step BASE CANDIDATE --json`. For an independent Package without a complete Bot graph, use `check-package`.
5. Follow only the returned deterministic `next_actions`: repair candidate source, build/runtime failures, exact missing mechanic proof, or selected-test failures, then rerun `author-step`. The agent host owns the actual source edits and iteration budget.
6. When the state is `ready_to_promote`, the agent host may promote the candidate snapshot to the next accepted baseline. `no_change` keeps the existing baseline. A `blocked` state means the accepted baseline itself is invalid and candidate mutation is not the remedy.
7. Periodically, and always at milestone/release closure, run full `check`/`test` on the combined Bot.
8. Invoke `gvya build` to produce `.gvya`; never construct or edit artifact bytes directly.

The canonical command surface is:

```text
gvya schema [--kind KIND] --json
gvya inspect [PROJECT] [--kind KIND [--id ID]] --json
gvya capabilities [PROJECT] --json
gvya capability [PROJECT] --id CAPABILITY --json
gvya analysis [PROJECT] --json
gvya audit [PROJECT] --json
gvya test [PROJECT] --json
gvya init bot OUTPUT_DIR [OPTIONS]
gvya init package OUTPUT_DIR [OPTIONS]
gvya check-package PACKAGE [--policy POLICY.json]
gvya check-change BASE_PROJECT CANDIDATE_PROJECT --json
gvya author-step BASE_PROJECT CANDIDATE_PROJECT --json
gvya check [PROJECT] [--policy POLICY.json]
gvya turn [PROJECT] --request REQUEST.json
gvya capability-result [PROJECT] --request REQUEST.json
gvya build [PROJECT] --output FILE.gvya
gvya signing-root ARTIFACT.gvya
gvya attach-signature ARTIFACT.gvya --envelope ENVELOPE.json --output SIGNED.gvya
```

This block is the complete canonical command surface; other documents reference it rather than repeating a partial list. The signing commands keep private-key custody external and are specified in `COMPILER_PIPELINE.md`.

`gvya schema --json` lists the available source object kinds; `--kind` returns one exact closed-key field inventory with requiredness, value type, enum/default hints, identity field, and nested item-kind references. The compiler source parser owns this registry, and `gvya check` / `check-package` remain authoritative for cross-field, graph, resource-budget, and semantic constraints.

`gvya inspect PROJECT --kind KIND --json` is the canonical authored-source inventory. It returns exact raw source values together with `file`, JSON `pointer`, Package, namespace, contribution, and nested-owner provenance. Add `--id ID` for an exact object lookup. This is intentionally distinct from the default composed-project `inspect` overview: an authoring agent needs the physical source location it must edit, not only the post-composition runtime view. Nested responses and capability contracts are first-class inspectable objects.

`PROJECT` is `gvya.project.json` or its containing directory. `PACKAGE` is `package.json` or its containing directory. `init`, `check-package`, and `check` emit bounded structured JSON by default; there is no redundant `--json` switch. `author-step` is machine-only and requires `--json`. Inspection and execution commands emit bounded structured JSON from canonical Rust composition/runtime paths, so agents do not need to scrape Studio or recreate GVYA semantics.

## Incremental vertical-slice gate

`gvya check-change BASE_PROJECT CANDIDATE_PROJECT --json` is the canonical inner-loop acceptance boundary for a human or external agent growing an existing Bot/Package graph. `BASE_PROJECT` is an immutable accepted source snapshot. `CANDIDATE_PROJECT` is an ordinary GVYA source tree after one proposed **vertical conversation slice**. GVYA defines no patch dialect, merge file, AI-session record, or hidden authoring state: the semantic diff between the two normal source snapshots is the slice.

The gate compares the two **composed** projects and the runtime-relevant project source surface. It classifies added/modified/removed Meanings, Behaviors, capability-result handlers, openings, fallback Behaviors, style lexicon, capabilities, bindings, policies, capability config, types, assets, Regression Cases, and Conversation Scenarios. Package order, composed Matcher Profile data, language enablement/defaults, semantic configuration, conversation configuration, project identity, and debug-map policy are tracked explicitly rather than disappearing behind composition.

The `ChangeTestPlan` carries a second authority: **mechanic coverage**. Runtime-affecting contributions are mapped to the observable mechanics that the candidate now depends on: semantic resolution, behavior response, topic scope/activation, follow-up scope/open, repair continuation, repeat ladders, state eligibility/response/effect, openings, fallback recovery, capability proposal/policy/confirmation/result, and global runtime contracts. Each requirement is keyed by mechanic + source kind + subject and records exactly which added/modified manual tests prove it.

Positive-state mechanic obligations are candidate-oriented. If a Behavior removes a follow-up/topic/repeat/state mechanic, the gate does not demand a positive postcondition for state that no longer exists; the mandatory Behavior Response proof plus the old/new blast radius protects that removal. Removal of a fallback contribution can be proven with direct negative response evidence without pretending the candidate entered fallback mode.

This closes a weakness in contribution-level proof: one changed test for one changed contribution can no longer make a multi-mechanic or multi-subject slice look complete. For example, changing Meaning A and Meaning B requires direct semantic proof for both; changing a Behavior to open a follow-up requires follow-up evidence, not merely a test that still resolves the Behavior's Meaning. A single changed test may cover several requirements only when its expectations actually observe all of them. Generated tests, unrelated changed tests, semantic-neighbor tests, and sentinels never substitute for missing direct mechanic proof.

For ordinary localized changes, test selection remains conservative and bounded:

1. every added/modified Regression Case or Conversation Scenario in the candidate;
2. tests that directly depend on changed Meaning/Behavior/response/topic/follow-up/state/context/capability surfaces;
3. tests for semantic-neighbor Meanings computed from both the **old and new** sides of each changed Meaning;
4. a bounded deterministic sentinel slice of existing manual tests.

Modified contributions retain the union of old and new dependency surfaces for blast-radius selection, so moving a Behavior/binding/policy cannot silently drop tests from its previous neighborhood. The direct-proof question is separate: the candidate slice is mergeable only when every required mechanic has at least one changed, manual, non-generated test that directly proves it.

If package composition order, Matcher Profile semantics, project language/semantic/conversation settings, style lexicon, global capability/type/asset contracts change, or semantic-neighbor analysis exhausts its safety budget, the plan sets `full_suite_required=true` and selects the complete candidate suite instead of pretending the change is local. Full-suite escalation does not transfer acceptance authority out of `ChangeTestPlan`.

The candidate is compiled once, loaded through the canonical Runtime, and only the selected suite is executed. `gvya.cli.check-change/1` exposes the semantic change set, selected tests/reasons, neighbor analysis, and `impact.mechanic_coverage` (`required`, `covered`, `missing`, and `requirements[]` with mechanic/source/subject/covered-by evidence). `mechanic_proof_missing=true` is an acceptance blocker. The CLI consumes this plan; it does not recompute mechanic policy independently.

The promotion loop is therefore:

```text
accepted BASE
    -> candidate vertical slice + changed manual proof
    -> check-change/1
       -> reject: repair candidate without mutating BASE
       -> accept: promote candidate as next BASE
    -> next slice
```

This keeps normal authoring cost proportional to the current slice and its blast radius while preserving full-target validation at checkpoints. A zero exit code for `check-change` proves one candidate slice; periodic and release-time `gvya check` / `gvya test` remain the complete-target gate. The detailed content-design workflow is `PACKAGE_AUTHORING_RECIPE.md`.

## Deterministic autonomous-authoring step

`gvya author-step BASE_PROJECT CANDIDATE_PROJECT --json` is the agent-facing state machine layered directly on the canonical incremental gate. It does **not** infer conversation mechanics independently, edit files, call a model, store a session, or promote bytes itself. The valid-source path runs the exact same `ChangeTestPlan`, build, Runtime load, and selected test execution used by `check-change`; the resulting `gvya.cli.check-change/1` report is embedded unchanged under `gate`.

The outer machine contract is `gvya.cli.author-step/1`:

- `state` — `repair_required`, `ready_to_promote`, `no_change`, or pre-gate `blocked`;
- `accepted` — the canonical incremental acceptance result when the gate can run;
- `promotion_allowed` — true only for an accepted candidate with an actual semantic/source change;
- `base_policy` / `candidate_policy` — explicit mutation boundary for the external agent;
- `primary_action` — first action to resolve;
- `source_identity` — SHA-256 fingerprints of the exact declared BASE/CANDIDATE source trees, independent of checkout root path;
- `promotion` — `gvya.promotion/1` identity bound to the ordered BASE/CANDIDATE fingerprints, emitted only when promotion is allowed;
- `diagnostics[]` — machine-stable preflight diagnostics (`side`, `stage`, `code`, optional source/object identity, expected/actual, remediation);
- `next_actions[]` — bounded deterministic repair/rerun/promote operations;
- `gate` — the complete `check-change/1` result, or `null` only when source preflight prevented the gate from existing.

For a valid composed candidate, the **compiler** owns the authoring-loop decision. It can emit:

- `resolve_build_failure`;
- `resolve_runtime_load_failure`;
- `add_direct_mechanic_proof` with exact mechanic/source-kind/subject;
- `resolve_regression_failure` with failed test and failure codes;
- `resolve_scenario_failure` with failed steps and failure codes;
- `resolve_incomplete_test_execution`;
- `rerun_author_step`;
- `promote_candidate`; or
- `keep_baseline`.

`check-change/1.source_identity` repeats the exact fingerprints inside the gate so the outer authoring decision and embedded acceptance authority can be cross-checked without trusting directory labels. `gvya.promotion/1` is domain-separated and binds the ordered BASE/CANDIDATE pair; any authored byte/path change produces a different candidate fingerprint and therefore a different promotion identity.

`check-change/1.execution` carries structured `build_diagnostics[]` and `runtime_diagnostics[]`; external agents must not parse Rust Debug strings to classify these failures. Source/composition diagnostics preserve compiler codes and authored provenance when available.

Source preflight is necessarily outside `ChangeTestPlan` because no composed candidate exists yet. `author-step` therefore converts candidate load/resolution/composition failures into `resolve_candidate_source_failure` plus `rerun_author_step`. An invalid accepted BASE produces `state=blocked` and `restore_valid_accepted_baseline`; the command explicitly tells the agent **not** to mutate the candidate to compensate for a broken baseline.

The host loop is therefore deterministic but externally executed:

```text
immutable accepted BASE
        +
ordinary candidate source
        |
        v
gvya author-step/1
        |
        +-- repair_required -> apply only candidate repairs -> rerun
        +-- no_change       -> keep BASE
        +-- ready_to_promote -> host promotes candidate as next BASE
        +-- blocked         -> restore a valid accepted BASE
```

Promotion remains an external filesystem/source-control operation so the validation command cannot silently overwrite the accepted snapshot it is supposed to protect.

### End-to-end external-agent proof surface

`validation/authoring-e2e/` keeps ordinary GVYA source snapshots that exercise the same BASE/CANDIDATE workflow an external agent uses. The durable runner validates/builds those snapshots through the shipped Engine compiler, exercises representative Runtime conversation/state/capability paths, and—when given a real `gvya` binary—launches `author-step` as a separate process to verify repair, acceptance, global escalation, baseline immutability, host-owned promotion, and sequential accepted slices.

The validation runner does not classify mechanics or derive blast radius. Its only authoring authority is the returned `gvya.cli.author-step/1` report and embedded `gvya.cli.check-change/1` gate. This keeps end-to-end validation from becoming a second implementation of the authoring policy it is supposed to test.

## Deterministic authoring control loop

`gvya check` is the full-target sensor and checkpoint acceptance gate for AI-first authoring. `author-step` owns the incremental repair/promote decision, while `check` remains the broad checkpoint/release measurement. Neither command is a model client or source-editing autonomous agent:

```text
user goal and source material
            |
external agent drafts or edits GVYA source
            |
gvya check
            |
quality_vector + obligations + canonical reports
            |
external agent makes a bounded repair and repeats
```

Source loading, validation, and composition failures are returned first as structured repair obligations. After source composes, the command attempts canonical build, runtime loading, audit, analysis, and authored tests. Its default invariant gate rejects build/runtime failures, audit errors, and failed or incomplete authored tests. Zero authored tests remain valid unless the supplied acceptance policy sets `require_tests`.

An optional versioned policy adds goal-specific thresholds without hiding them in a prompt or universal score:

```json
{
  "format": "gvya.authoring.acceptance",
  "version": 1,
  "max_audit_warnings": 0,
  "require_tests": true,
  "min_meaning_expectation_coverage": 0.8,
  "min_behavior_expectation_coverage": 0.8,
  "min_capability_expectation_coverage": 1.0,
  "max_ambiguity_pairs": 2,
  "max_fallback_observation_ratio": 0.15
}
```

All threshold fields are optional. Expectation-coverage and fallback ratios range from zero through one. The explicit `expectation` name prevents authored test reachability from being mistaken for user-language discoverability. Fallback dependence measures only fallback and repeat-fallback modes observed in executed Regression Cases and Conversation Scenarios; it is not production-traffic telemetry. Ambiguity uses the canonical bounded analysis report. If a requested measurement cannot be proven, the gate emits an obligation instead of guessing.

The `gvya.cli.check/1` response contains:

- `accepted`, plus a nonzero process exit when false;
- the normalized acceptance `policy`;
- a `quality_vector` for correctness, semantic clarity, test coverage, observed fallback dependence, complexity, and deterministic artifact identity;
- machine-actionable `obligations` with stable code, repair action, summary, and details;
- the canonical source, audit, analysis, test, build, and runtime reports available at the point where the gate stopped.

There is deliberately no aggregate quality score. The compiler/runtime measures the source and the deterministic gate decides whether stated constraints pass; the authoring model does not grade itself.

The authoring loop is stateless. GVYA stores no prompt, rejected draft, provider/model performance, or repair history. The external agent host owns the iteration budget and may adapt constraints during one task. Any future empirical policy learning must remain explicit and provider-neutral rather than turning a GVYA workspace into an AI session database.

## Studio boundary

Studio is the human editor and does not consume or mirror the machine-facing authoring decision. An external agent host invokes `gvya author-step BASE CANDIDATE --json` against two ordinary source trees and consumes the exact `gvya.cli.author-step/1` report directly, including embedded `gvya.cli.check-change/1` mechanic coverage, selected-test counts, and full-suite escalation reasons.

Studio TypeScript does not define author-step transport or display contracts, enumerate mechanics, duplicate proof/blast-radius rules, infer acceptance from source changes, or translate provider output into a private patch/session format. The external agent host owns model/provider execution, source mutations, retries, report handling, and promotion. A repair-required non-zero CLI exit is expected and does not invalidate a well-formed author-step JSON report.

## Frozen authoring authority boundary

The authoring authority boundary is complete and intentionally stable. A machine author may start from `gvya init bot`, edit the same ordinary source documents a human edits, and use `author-step` / embedded `check-change` as the only incremental acceptance authority. Bounded local slices stay incremental when the canonical plan can prove their radius; global or unbounded changes escalate through that same plan. Every required conversation mechanic must have direct changed manual proof before promotion is allowed, and BASE remains immutable until an external host or human performs promotion.

Studio is a human source editor, not a presentation/review consumer of that machine decision or a second authoring engine. Future product work may improve editing UX, provider integrations outside GVYA, deployment targets, or optional runtime augmentation, but it must not add a hidden AI session, patch dialect, compatibility path, browser-side mechanic classifier, or alternate promotion authority. A change to these ownership rules is an explicit clean-break contract revision and must update the canonical compiler/CLI contracts and executable authoring validation together.

## Authority and portability

An external agent may author every source concept that a human can author, but it gains no runtime authority. The compiler remains the authority for parsing, package composition, validation, audit, tests, canonical IR, and artifact emission. The runtime remains the authority for executable semantics. The host remains the authority for capability execution.

Because GVYA owns no provider configuration, the same source workflow works with Codex, Claude, a future agent, a local model, a scripted generator, or a human editor. Provider changes do not change the GVYA workspace or artifact contract.

## Deliberate non-goals

GVYA and Studio do not own:

- model discovery or selection;
- OpenAI-compatible, Ollama, or vendor-specific transports;
- API keys and credentials;
- prompts, conversations, agent memory, retries, or token/cost budgets;
- proposal accept/reject history or an AI-specific review format;
- a duplicate JavaScript authoring/compiler/runtime authority.

Optional neural resolution during a deployed runtime is a separate host integration boundary described in `SEMANTIC_ARCHITECTURE.md`; it does not reintroduce authoring-provider configuration into Studio.
