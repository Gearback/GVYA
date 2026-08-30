# GVYA Source Format

GVYA source is transparent authoring input. It is **not** a `.gvya` artifact.

## Project root

A source tree contains exactly one root project manifest named:

`gvya.project.json`

This root is one **Brain compile target**, even when a Studio Project owns multiple Bots. Studio resolves Shared → Project → Bot authoring scopes before source export; compiler source never contains Studio scope or precedence metadata.

Minimum shape:

```json
{
  "format": "gvya.source.project",
  "version": 1,
  "project_id": "hotel-assistant",
  "brain_id": "room-voice",
  "languages": ["en-US", "fa-IR", "es"],
  "enabled_languages": ["en-US", "fa-IR"],
  "default_language": "fa-IR",
  "language_profiles": [
    "language-profiles/en-us.json",
    "language-profiles/fa-ir.json"
  ],
  "matcher_profiles": [
    "matcher-profiles/en-us.json",
    "matcher-profiles/fa-ir.json"
  ],
  "packages": [
    "packages/conversation/package.json",
    "packages/hotel/package.json"
  ],
  "fallback_package": "packages/fallback/package.json"
}
```

`languages` is a required, ordered, non-empty authoring catalog of at most 32 unique BCP-47-shaped tags. Every Meaning structural pattern and positive/negative/retrieval sample, response text/extra message, and explicit regression/scenario language must use one of these tags. Tags are not rewritten (`en` does not silently become `en-US`). This catalog lets selected multilingual Packages validate without making every authored language active for a Bot.

`enabled_languages` is a required, ordered, non-empty subset of `languages`. It is the Bot's runtime language boundary. `default_language` is required and must name one member of `enabled_languages` after locale normalization. One source root represents one resolved Bot compile target, so this is the Bot default—not a Project-wide default. The compiler embeds `enabled_languages` and `default_language` into `gvya.program`; it does not embed the broader authoring catalog, and changing array order does not change the selected default.

`language_profiles` and `matcher_profiles` are paired optional catalogs for enabled languages. A language-neutral Brain may omit both; otherwise the two arrays must select the same normalized language tags. Language Profile paths end in `language-profiles/<normalized-language>.json`; Matcher Profile paths end in `matcher-profiles/<normalized-language>.json`. Root and nested portable Project forms are canonical source paths. Profiles are not Packages, do not participate in dependency/override composition, and never contain Meaning, Behavior, Response, or Capability content.

Matcher Profiles may also declare named `pattern_sets`. A set maps normalized author aliases to the canonical String value returned by a structural set capture. Enabled Matcher Profiles compose these sets by set name and alias; identical aliases may repeat only when their canonical values agree. Invalid set names, aliases that normalize to no tokens, blank canonical values, or normalized alias collisions with different canonical values fail compilation.

```json
{
  "format": "gvya.source.matcher-profile",
  "version": 1,
  "language": "en-US",
  "profile": {
    "pattern_sets": {
      "devices": {
        "bedroom light": "device.bedroom-light",
        "desk lamp": "device.desk-lamp"
      }
    }
  }
}
```

`fallback_package` is optional and names the single selected Fallback Package for this Brain. It is separate from the ordinary `packages` graph and is never a dependency or override target. Optional `semantic` and `conversation` objects override bounded runtime settings. `conversation.repair_candidate_min_score` is a separate 0..1 floor for explicitly repair-eligible normal Behaviors and never changes the normal semantic `resolution_threshold`. `conversation.author_numbers` is an optional bounded list of `{path, default, min, max}` numeric author-state declarations; paths must be unique, non-overlapping parent/child paths, and `min <= default <= max`. `semantic.candidate_limit` is canonically `2..=256`; Studio and compiler use the same range so an authoring-valid workspace cannot become compiler-invalid on this setting. Optional `emit_debug_map` controls a non-essential source/provenance map in the artifact. The `packages` array is enumeration only: package identity/dependency semantics come from package manifests, and the compiler sorts resolved packages canonically before composition. Reordering equivalent package paths cannot create authority or alter unsigned artifact bytes. There is deliberately no `createdAt`, build timestamp, random build ID, absolute path, hostname, or machine identity.

## Package source: manifest + explicit fragments

Every Package path listed by the Project names a small root `package.json`. The Package root is the authoritative manifest and explicit fragment index; contribution content is not embedded in one large document.

```json
{
  "format": "gvya.source.package",
  "version": 1,
  "manifest": {
    "id": "hotel",
    "kind": "standard",
    "description": "Hotel-room capabilities and conversation",
    "dependencies": [
      {"id": "conversation", "reexport": false}
    ]
  },
  "fragments": {
    "meanings": [
      "fragments/meanings/0001-room-service.json"
    ],
    "behaviors": [
      "fragments/behaviors/0001-room-service.json"
    ],
    "capabilities": [
      "fragments/capabilities/0001-room-service-order.json"
    ]
  }
}
```

A declared fragment contains exactly one normal GVYA contribution envelope. Its namespace comes from the root index:

```json
{
  "id": "room-service",
  "exported": true,
  "mode": "add",
  "value": {
    "id": "room-service",
    "samples": [
      {"language": "en-US", "text": "room service"}
    ]
  }
}
```

The supported fragment namespaces are `meanings`, `behaviors`, `capability_result_behaviors`, `openings`, `fallback_behaviors`, `style_lexicons`, `capabilities`, `capability_bindings`, `capability_policies`, `capability_configs`, `types`, `assets`, `regression_cases`, and `scenarios`.

Fragment membership is **explicit**. The Package index is the only loading authority: the compiler never discovers contributions from directory contents. Every declared fragment path must be a safe package-relative `fragments/*.json` path, may be declared only once, and is loaded under the namespace that listed it. As a fail-closed integrity check, filesystem/source-tree loaders also reject any undeclared `.json` file physically present under the Package `fragments/` subtree with `source.fragment_undeclared`; an agent cannot accidentally create a fragment that is silently ignored. This keeps source authority deterministic while making a single Meaning, Behavior, Capability, test, or scenario a small independently editable file.

The canonical CLI creates an empty Package root without overwriting an existing path:

```text
gvya init package ./core.smalltalk.formal --kind standard --authoring-language en-US
gvya check-package ./core.smalltalk.formal
```

An empty Package has `"fragments": {}`. Authoring tools add files and explicit index entries as content is created.

`authoring.json` is emitted only when `--authoring-language` is supplied. It remains optional Studio metadata containing only the human authoring-language preference and is ignored by `check-package` and the compiler. Human backups are explicit dated folder ZIP downloads; Studio does not persist automatic Package revisions.

Studio's Shared/Project Package `authoring_language` is human-interface metadata: it controls which language a Package editor opens first. A Bot Package derives that preference from its Bot's `default_language`. This field is deliberately absent from canonical Package source and compiled artifacts.

Package `manifest.kind` is exactly `standard` or `fallback`. Standard Packages use the normal dependency/composition model. A Fallback Package is self-contained: it has no dependencies, cannot be depended on, cannot declare replace contributions, and its contributions are private add-only.

A Package source **does not declare its own digest**. The compiler hashes the root plus every explicitly declared fragment path/content in canonical namespace/index order, then includes referenced asset-byte digests. Editing, adding, removing, or retargeting a declared fragment therefore changes the authoritative Package digest. Asset `source` paths remain relative to the Package root, not to the fragment file that declares the asset.

## Contributions and specialization

Every contribution uses the explicit package/audit/test composition model:

```json
{
  "id": "door.open",
  "exported": true,
  "mode": "add",
  "value": {}
}
```

or an explicit whole-item replacement:

```json
{
  "id": "door.open",
  "exported": true,
  "mode": {
    "type": "replace",
    "target_package": "device.base",
    "target_id": "door.open"
  },
  "value": {}
}
```

There is no load-order override and no hidden partial merge.

## Structural Meaning patterns

A Meaning may declare deterministic whole-utterance `patterns` in addition to semantic samples. Structural patterns are **author rules**, not semantic evidence: eligible structural rules run before semantic candidate retrieval/scoring, and a structural winner cannot be overridden by a semantic score. If no structural rule wins, the ordinary bounded semantic matcher runs unchanged.

```json
{
  "id": "device.search",
  "patterns": [
    {"language": "en-US", "text": "SEARCH FOR *{query}", "priority": 0},
    {"language": "en-US", "text": "TURN ON <set:devices>{device}", "priority": 10}
  ],
  "slots": [
    {"name": "query", "type": "string", "required": true},
    {"name": "device", "type": "string", "required": false}
  ],
  "samples": [
    {"language": "en-US", "text": "can you search for something for me"}
  ]
}
```

The v1 structural grammar is deliberately small and AIML-inspired:

- literal tokens match in authored order and the rule is anchored to the whole utterance;
- `*` matches one or more tokens;
- `^` matches zero or more tokens;
- `*{slot}` and `^{slot}` capture the wildcard span into an explicitly declared `string` slot;
- `<set:name>` matches one alias from Matcher Profile `pattern_sets`;
- `<set:name>{slot}` also captures that alias's authored canonical String value;
- every structural rule must contain at least one literal or set anchor; wildcard-only catch-all rules are rejected because GVYA owns fallback separately.

Literals and set aliases use the selected Language Profile's deterministic text normalization and canonical-token mapping; set vocabulary itself comes from the paired Matcher Profile. Wildcard captures preserve the **normalized surface token text** rather than replacing it with canonical-token aliases; set captures intentionally return the set's authored canonical value. Structural matching does not apply semantic glue removal, typo similarity, sample scoring, or embedding inference.

Specificity is deterministic: more literal anchors win first, then more set anchors/set tokens, then fewer wildcard atoms and shorter wildcard spans, then explicit pattern priority and Meaning priority. If equally specific rules from different Meanings remain tied, GVYA returns structural ambiguity. If one rule has equally good wildcard partitions that produce different captures, GVYA returns `structural_captures_tied` rather than guessing. Structural work is globally bounded per analysis; budget exhaustion fails closed and never falls through to semantic scoring.

Changing structural patterns is conservatively treated as a full-suite change by the incremental authoring gate because an authoritative wildcard rule can alter the matching boundary beyond sample-neighbor analysis. Changing Matcher Profile data, including `pattern_sets`, already has the same full-suite treatment.

## Required declarations and elicitation

A Meaning `slots[]` row is `{name, type, required, elicitation}`; a `references[]` row is `{kind, required, elicitation}`. `type` is one of `string`, `number`, `boolean`, `entity` (with `entity_kind`) or `reference` (with `reference_kind`). Unknown keys are build errors like every other compiler-owned source object.

`elicitation` is an ordinary localized `{language,text}` list — the same shape as `samples` — and it is the authored question GVYA asks while that value is still missing:

```json
{
  "id": "order.create",
  "patterns": [{"language": "en-US", "text": "create order", "priority": 5}],
  "slots": [
    {
      "name": "count",
      "type": "number",
      "required": true,
      "elicitation": [
        {"language": "en-US", "text": "How many items?"},
        {"language": "fa-IR", "text": "چند تا؟"}
      ]
    }
  ]
}
```

Every `required: true` slot and reference must carry at least one non-empty localized prompt, and each declaration's prompt languages must be unique; both rules are enforced when the semantic catalog is built, so a required declaration without elicitation fails composition rather than producing a Bot that can select a Meaning it can never complete. The rule is *at least one* prompt, not one per enabled language: a turn whose language has no renderable prompt fails closed into the ordinary Fallback path instead of holding a mute collection, so authoring a prompt per enabled language is the practical expectation.

Prompts are author data only. The runtime never synthesizes elicitation text, and it never introduces a form/reprompt framework, validation DSL or cardinality system around them — see the collection lifecycle in `CONVERSATION_ARCHITECTURE.md`.

## Multilingual Meaning samples and responses

A Meaning owns one semantic intent/behavior and may contain structural patterns and positive samples in any language declared by the resolved source catalog (derived from Project Matcher Profile documents in Studio):

```json
{
  "id": "greeting.hello",
  "samples": [
    {"language": "en-US", "text": "hi"},
    {"language": "fa-IR", "text": "سلام"},
    {"language": "es", "text": "hola"}
  ]
}
```

These are not three language-specific Behaviors. They are explicit multilingual evidence for the same Meaning. Response text rows likewise carry an explicit `language`. The host may supply an explicit request language; otherwise the runtime uses an enabled active conversation language and then the compiled Bot default. Negative samples and `retrieval_terms` use the same `{language,text}` shape. Semantic retrieval is language-bucketed: it tries the enabled primary language and its base tag when distinct, followed by explicit enabled policy fallbacks. `und` participates only when the Bot explicitly enables it; unrelated or disabled language buckets never compete merely because their normalized text is identical.

## Dynamic response boundary

Response templates are deterministic source, not ambient host access. `{{system.time}}` reads only a `system.time` value explicitly supplied by the host request; GVYA never reads the machine clock. `{{rnd(1, 6)}}` and `{{pick('a', 'b')}}` use the explicit turn seed. The runtime may derive bounded pure values such as `system.mathResult` from the utterance. Network, device, database, filesystem, irreversible, or otherwise host-executed work belongs in an explicit Capability contract instead of a template. This boundary lets an agent route dynamic requirements without inventing hidden runtime authority.

## Conversation Behavior eligibility

Behavior-level conversation gates are authored on the Behavior itself, not duplicated across its responses. The relevant source fields are:

```json
{
  "id": "device.status.behavior",
  "meaning": "device.status",
  "followup_scope": "device.confirm",
  "requires_values": [
    {"namespace": "context", "path": "device.connected", "value": true}
  ],
  "forbidden_values": [
    {"namespace": "author", "path": "device.disabled", "value": true}
  ],
  "responses": []
}
```

`requires_values` uses exact typed equality and every row must match. `forbidden_values` also uses exact typed equality, but any matching row blocks the Behavior; a missing forbidden path is therefore allowed. Supported Behavior value namespaces are `author`, `conversation`, `context`, `meaning`, and `system`. Response-level `conditions` remain a separate mechanism for choosing among responses after the Behavior itself is eligible.

Behavior uniqueness is scoped rather than globally tied to a Meaning. A Meaning may have one unscoped default Behavior and one additional Behavior for each distinct `followup_scope`. The runtime selects a scoped Behavior only for that exact active follow-up and selects the default only outside a follow-up scope. Two Behaviors for the same Meaning and the same scopeâ€”including two unscoped defaultsâ€”are invalid. This lets reusable acknowledgements such as a shared yes/no Meaning answer normally while also driving several explicit confirmation flows without inventing duplicate semantic Meanings.


## Fallback Behavior

Fallback is not a semantic Meaning and is never considered during normal Meaning matching. Only a selected Fallback Package may contribute `fallback_behaviors`. A Fallback Behavior is condition-aware conversation behavior selected after normal resolution fails (or when the repeat trigger is active):

```json
{
  "id": "angry.unresolved",
  "trigger": "unresolved",
  "priority": 100,
  "conditions": [
    {"namespace": "author", "path": "mood.anger", "op": "greater_or_equal", "value": 70}
  ],
  "responses": []
}
```

`trigger` is `unresolved` or `repeat`. Conditions use the ordinary typed conversation condition model. Among eligible authored Fallback Behaviors for the trigger, only the highest-priority tier participates in deterministic response selection. If the Bot has no selected Fallback Package, or no authored Fallback Behavior is eligible, the language-neutral runtime returns a Silent fallback outcome. User-facing recovery prose must therefore be authored explicitly in a Fallback Package; the engine never invents hidden natural-language fallback text.

A normal Behavior may additionally declare `repair_continuation_candidate: true`. This is not a new Meaning class and does not make the Behavior a catch-all. It only permits the conversation kernel to use that Behavior after normal semantic resolution stays unresolved and the candidate crosses the separate repair floor, or to continue a recorded prior repair candidate through an explicit generic follow-up. Required slots/references and ordinary Behavior/response eligibility remain authoritative. Optional `repeat_same_input_after` and `repeat_same_meaning_after` values are bounded to 2..20 and override repeat-stage entry for that Behavior only.

## Language and Matcher Profile documents

Semantic language mechanics are explicit standalone source data. The kernel default is language-neutral and there is no built-in language/transliteration profile selector. A Language Profile owns normalization, morphology, phrase rewrites, weighting, continuation vocabulary, and lexical entity data:

```json
{
  "format": "gvya.source.language-profile",
  "version": 1,
  "language": "en-US",
  "profile": {
    "canonical_tokens": {"dogs": "dog"},
    "canonical_suffixes": {"ies": "y", "s": ""},
    "canonical_suffix_exceptions": ["news", "status"],
    "detached_suffixes": [],
    "normalization_rewrites": {"’": "'"},
    "colloquial": {
      "hiya": ["hello"]
    },
    "pure_glue": ["the", "a"],
    "negations": ["not", "never"],
    "weak_numeric_ignore": ["set", "to"],
    "number_words": {"one": 1},
    "relative_dates": {"tomorrow": "tomorrow"}
  }
}
```

`canonical_suffixes` applies the longest authored suffix rewrite only after exact `canonical_tokens`; at least three stem characters must remain. `canonical_suffix_exceptions` is an exact normalized-token guard evaluated before suffix rewriting, so productive rules such as English plural `s` can stay authored without corrupting known non-plural confounders. `detached_suffixes` removes an authored suffix token only when it follows a stem, covering forms such as Persian plural markers separated by a zero-width joiner. `colloquial` keys may contain bounded multi-token phrases; longest matching phrase wins deterministically.

Matcher Profiles contain only structural `pattern_sets`, using the document shape shown near the Project example. For each enabled language, the selected Language Profile is combined only with the Matcher Profile carrying the same normalized language tag. Compiler IR preserves these as `semantic.profiles[language]`; runtime hydration requires the profile-key set to exactly cover `enabled_languages`. Profiles from different languages never merge. Conflicting mappings inside one same-language pair fail closed. Studio stores paired reusable documents under `shared/language-profiles/` and `shared/matcher-profiles/`, and paired Project-local copies under the corresponding `projects/<project>/` directories. The paired entry remains the single Studio language catalog. Removing either half preserves affected Bot or Package data but makes the pair invalid until restored. Both profile types are JSON/AI authoring surfaces and intentionally have no Package card or human visual editor.

## Assets

Source asset contribution:

```json
{
  "id": "merchant.portrait",
  "value": {
    "id": "merchant.portrait",
    "media_type": "image/webp",
    "logical_path": "assets/merchant/portrait.webp",
    "source": "assets/portrait.webp"
  }
}
```

`source` is resolved relative to the Package root `package.json`, even when the asset contribution lives in a fragment. It may not be absolute, contain backslashes, `.`/`..` segments, empty segments, NUL, or control characters. `logical_path` must be below `assets/`. The compiler derives the SHA-256 from bytes; authors never supply a trusted asset digest.

Two packages may legitimately contain the same `AssetId` while an explicit specialization replaces one. Source bytes are therefore tracked by content digest, not globally by asset ID.

## Capability contracts

Capability source uses ordinary JSON Schema objects directly in `input_schema` / `output_schema`; authors do not double-encode schema JSON inside strings. The compiler canonicalizes those documents into the contract and also keeps the compiler-owned bounded `ValueSchema` shape used by the deterministic runtime. Future schema-profile expansion remains compiler-owned and does not require changing the host execution boundary.

## Paths and authority

Compilation consumes an explicit in-memory/file-tree snapshot. Source documents cannot request network access, environment variables, current time, random values, callbacks, shell commands, absolute files, or host capabilities. Those are outside the source language.


## Fail-closed parsing

External machine authors do not need to reverse-engineer these closed key sets from prose. `gvya schema --json` returns the compiler-owned recursive source-authoring index and `gvya schema --kind KIND --json` resolves both top-level authoring objects and exposed nested kinds such as `value-requirement`, `capability-trigger`, `binding-source`, `conversation-effect`, `turn-expectation`, and `scenario-step`. Scalar/dynamic kinds identify their shape explicitly; discriminated unions expose their variants. The schema report is `gvya.cli.source-schema/1` and reports the current Project, Package, and Matcher Profile document versions rather than pretending the whole source tree has one version. The schema surface is introspection, not a second validator: `gvya check` / `check-package` still own cross-field, graph, resource-budget, and semantic validation.

For source navigation, use `gvya inspect PROJECT --kind KIND --json`; use `--id ID` to retrieve an exact authored object. The report preserves raw authored JSON and identifies its declared file, JSON pointer, Package, contribution namespace, contribution id, and nested owner when applicable. Agents therefore do not need to scan a Package tree to locate a Behavior, Meaning, Response, Capability, binding, policy, type, asset, test, scenario, Matcher Profile, or other first-class source object.

Compiler-owned source objects have closed key sets in format v1. Unknown keys, wrong JSON types, unsupported schema assertions, and missing required fields are build errors. This rule applies to runtime behavior **and to regression/scenario definitions**: a malformed test expectation must never be silently weakened or dropped. Maps explicitly documented as author/host data (`values`, state maps, literal argument maps, slot expectation maps) remain open data maps rather than schema namespaces.
