# Packages, Auditor, Why and Test Architecture

## 1. Package graph

A GVYA package is a build-time unit. It is not a runtime plug-in and load order is not authority.

Each resolved package manifest has:

- Package ID;
- package kind: `standard` or `fallback`;
- SHA-256-shaped source digest;
- description;
- exact dependencies;
- per-dependency `reexport` visibility.

Composition is a deterministic dependency-first topological order with lexical tie-breaking.

### Package kinds

`standard` is the ordinary reusable/package-composition unit. Standard Packages may declare dependencies and may use explicit whole-item `Replace` when the target is visible and exported.

`fallback` is a separate, self-contained conversation safety/personality unit. A Brain selects at most one Fallback Package, outside the ordinary package dependency graph. Fallback Packages:

- have no dependencies;
- cannot be dependencies of Standard Packages;
- cannot contribute Meanings, normal Behaviors, Capabilities, bindings, policies, semantic profiles, openings, style lexicons, or compiled types;
- may contribute `fallback_behaviors` plus their own private assets/tests/scenarios;
- require every contribution to be private (`exported=false`) and add-only;
- cannot `Replace` any contribution and cannot be overridden by a Bot Package or any other Package.

This makes the selected Fallback Package one explicit root, not another precedence layer.

### Visibility

A package sees every direct dependency. A transitive dependency is visible only through an explicit re-export chain. This matters for specialization: a package cannot reach through an encapsulation boundary and replace a contribution it did not explicitly receive.

### Contribution semantics

Every contribution is either:

- `Add`; or
- `Replace { target_package, target_id }`.

`Replace` is valid only when:

1. target package is visible;
2. target item already exists in the same contribution namespace;
3. current owner exactly equals `target_package`;
4. target is exported;
5. replacement ID exactly equals target ID.

Replacement is whole-item. There is no partial hidden merge. Duplicate `Add` is an error.

Use local replacement for local specialization, not as an implicit patch layer. A Bot or Project that replaces a reusable Shared contribution owns the complete replacement value: canonical samples/responses that should remain available must be carried forward explicitly. This is the preferred boundary when a broadly reusable Shared Smalltalk contribution is correct globally but too aggressive for one Bot; do not edit Shared source to solve a Bot-local semantic collision.

### Contribution namespaces

package/audit/test layer supports package composition for:

- Meaning patterns;
- semantic-profile patches;
- conversation behaviors;
- openings;
- first-class fallback behaviors (Fallback Packages only);
- style lexicon patches (Standard Packages only);
- capabilities, bindings, policies, capability config;
- named compiled types;
- assets;
- regression cases;
- conversation scenarios.

The result is an in-memory composed project containing canonical runtime catalogs plus provenance. compiler/artifact layer owns source syntax, resolution from filesystem/project files and `.gvya` emission.

## 2. Auditor

Auditing is compiler-owned and authoring-facing. It has two simultaneous consumers:

- humans need compact summaries and progressive drill-in;
- machine clients and CI need stable codes, locations and related identities.

`AuditIssue` therefore contains stable code, severity, category, summary, structured location, related items, optional remediation and details. Reports sort deterministically into Error → Warning → Info, then category and code.

The built-in audit covers package composition, source/value identity, semantic overlap, multilingual sample/response coverage, exact and near-duplicate response text, conversation/follow-up integrity, capability-catalog findings, types, assets and tests. `AuditRule` allows later domain packages or Studio tooling to add rules without changing the core report model.

Expensive semantic and response-overlap work is bounded and reports when its configured work limit truncates analysis.

## 3. Why

Why is not logging. Canonical kernels emit stable traces; package/audit/test layer projects them into a human-oriented `WhyReport`.

Human order is:

1. Summary;
2. Rejected / blocked paths;
3. Understanding;
4. Conversation;
5. Capabilities;
6. Context;
7. Selected response;
8. Other diagnostics.

Summary is curated from terminal/decision-significant events and bounded to avoid overload. Raw traces remain separately available for deep inspection and machine analysis.

## 4. Regression and scenario model

Tests are source/build-time contracts, not renderer tests.

A turn can assert:

- selected or forbidden Meaning IDs;
- exact typed Meaning slots/references;
- minimum semantic score;
- conversation mode;
- selected/forbidden Response IDs;
- response contains/not-contains;
- author-state paths;
- typed conversation-state projections;
- active topic/follow-up;
- expected capability ID/version/exact arguments;
- forbidden capabilities;
- required/forbidden Why codes.

`RegressionCase` is one turn. `ConversationScenario` is an ordered interaction program: `open`, `turn`, `confirm`, and `capability_result` steps carry explicit runtime state and stop at the first failed step. A `turn` may also supply hint requests, resolver context and reference candidates. `confirm` references an earlier proposal and replays the exact originating interaction with a typed confirmation grant; `capability_result` references a real proposal emitted by an earlier step rather than inventing a proposal ID. Step expectations can assert proposal receipts directly, including `admitted`, `needs_confirmation`, or `rejected` outcomes and stable reason codes. When one interaction emits multiple proposals for the same capability, `proposal_ordinal` selects the intended one; ambiguous selectors fail closed. Step references are one-based and must point backward. Explicit context/seed/time are inputs; no hidden session store/time/random source is introduced.

The runner depends on `SimulationDriver::run_interaction`, so authored tests exercise the same host interaction boundaries as Runtime without adding a test-only session protocol.

## 5. Coverage and ambiguity

Coverage reports which composed Meanings, Behaviors and Capabilities are positively asserted by the regression/scenario corpus, and separates generated from manual tests.

Ambiguity analysis performs bounded cross-Meaning sample comparison, ranks exact/near overlaps deterministically and reports truncation explicitly if limits are reached.

## 6. Explicit package/audit/test layer non-scope

package/audit/test layer does not define:

- GVYA source-file syntax;
- dependency download/registry semantics;
- canonical source serialization;
- `.gvya` IR, file encoding, hashing or signing;
- runtime ABI/SDK;
- Studio screens;
- external authoring-client workflows.

Those belong to other explicit architecture layers.

## Canonical package, audit, Why and test inventory

This inventory is normative for the current GVYA architecture; implementation and executable tests must agree with it.

| Concern | Canonical representation |
|---|---|
| reusable shared packs | stable-ID package dependency graph; explicit dated source-folder ZIPs are human backups, not dependencies |
| per-local specialization of shared content | explicit whole-item `Replace` against exact exported visible dependency contribution |
| later-load duplicate silently overriding in local path | collision is error unless exact `Replace` is declared |
| partial field merge of override intent | whole logical item replacement only; no hidden inheritance of omitted fields |
| package provenance | owner/export/replaced provenance plus canonical source digest |
| selected Fallback Package | one optional self-contained Fallback Package root per Brain |
| fallback override/replacement | forbidden structurally; Fallback Package contributions are private add-only |
| fallback personality/state routing | `FallbackBehavior { trigger, priority, conditions, responses }` |
| emergency fallback | engine-owned last resort when no authored fallback is eligible |
| openings from reusable Standard Packages | opening contributions |
| package identity/description | package manifest id/digest/description/dependencies |
| package integrity identity | required SHA-256-shaped source digest; compiler/artifact layer owns canonical bytes and hashing |
| duplicate/missing sample audit | machine-coded semantic audit |
| exact sample collision across meanings | error |
| high-overlap samples | bounded Jaccard warning + separate ambiguity analysis |
| missing/duplicate response identity/content | catalog validation + authoring audit |
| near-duplicate answer/response review | bounded normalized response overlap audit with stable issue codes |
| >240-character response review | warning, configurable limit |
| follow-up opener/consumer consistency | warnings for orphan opener/consumer |
| follow-up transition cycles | bounded graph cycle warning |
| conditions/effects validity | typed catalog plus audit of paths/finite increments |
| raw legacy action payload validation | typed capability kernel capability catalog/binding/schema/policy audit |
| attachment path/type checks | typed asset table, digest/media/path validation and response reference integrity |
| URL safety | catalog fail-closed URL validation |
| extra-message chance/text checks | catalog validation |
| error/warning/info counts | `AuditSummary` |
| grouped human issue display | deterministic severity+category groups with locations and related IDs |
| raw JSON Why wall | raw trace remains drill-down only |
| Why compact summary | curated Summary section first |
| Why answer rejection reasons | rejected/blocked paths promoted from stable trace codes |
| Why score/context/selection details | semantic/conversation/capability/context/selected sections |
| one-turn regression tests | typed `RegressionCase` expectations |
| generated/manual test distinction | `generated` flag and origin coverage counts |
| interaction scenario state carry | `ConversationScenario.steps` + explicit `GvyaState` carry |
| open / turn / confirmation / capability-result flow | typed `ScenarioStep` interactions using real prior proposals |
| stop scenario on first failed step | runner behavior |
| expected intent | full Meaning + slots/references |
| expected action type | exact capability/version/arguments |
| expected memory/stats | author-state + typed conversation projection |
| expected answer substring | contains/not-contains plus exact Response IDs |
| confidence floor | minimum semantic score |
| coverage review | meaning/behavior/capability test coverage |
| ambiguity review | bounded exact/near sample-pair report |
