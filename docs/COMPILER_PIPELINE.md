# GVYA Compiler Pipeline

## Authority flow

```text
SourceTree snapshot
    ↓ strict JSON/path/bounds parsing
derived package + asset digests
    ↓
PackageDefinition[]
    ↓ package/audit/test layer package graph
compose_packages
    ↓ package/audit/test layer human/machine auditor
Auditor (errors fail build)
    ↓
ComposedProject
    ↓
Canonical runtime IR
    ↓
program.json + verified asset bytes
    ↓
integrity.json + manifest.json
    ↓
deterministic GVYA container
    ↓ optional signer(content_root)
*.gvya
```

There is no fallback branch that skips audit, silently repairs malformed source, fetches data, invokes an LLM, or lets a renderer compile its own interpretation.

## Canonical IR

`program.json` is the clean-break v1 compiler IR (`gvya.program/1`). During pre-publication development its contract is updated in place; there are no legacy readers or migrations. It is the **minimal executable program**: every section is either executed by the runtime or cross-validated by runtime load against the manifest/integrity rows. Nothing is serialized merely because the compiler computed it.

Executed by the runtime:

- semantic config
- language-scoped `semantic.profiles`, each containing one Language Profile (including Boolean/custom deterministic entity catalogs) plus only its same-language Matcher Profile `pattern_sets`
- resolved meanings, including compiled-contract structural pattern source rows, typed slot/reference declarations, and localized elicitation prompts for required values
- conversation config/catalog
- responses, conditions, effects, topic/follow-up behavior
- capability definitions, compiled shapes, bindings, policies and limits
- asset metadata

Cross-validated by runtime load:

- `source_packages` (id -> canonical source digest) must equal the manifest and integrity package rows
- `packages` (composed package order) must be a canonical permutation of that same package set

Explicitly **not** in the executable program:

- the derived semantic matcher index, which runtime rebuilds from the canonical patterns/profiles/config
- named types, which are an authoring/audit concept with no runtime consumer
- composition provenance and the regression/scenario corpus, which are debug/authoring data

Named types are validated by the auditor at build time. Regression/scenario definitions are build-time data and never enter runtime execution. If debug-map emission is requested, their IDs plus detailed composition provenance (owner package, exported flag, replaced package) are emitted in the non-essential `debug/source-map.json` entry.

Before IR emission, audit validates the composed structural matcher contract against each composed same-language Language/Matcher Profile pair: grammar/capture-slot validity, total rule bounds, required literal/set anchors, referenced sets, custom-entity virtual sets, set/entity names/aliases/canonical values, normalized alias collisions and profile-dependent literal normalization. Required slots/references must each have at least one valid localized elicitation prompt. The runtime performs the same structural-matcher construction again when hydrating `gvya.program/1`, so malformed structural/entity data cannot become compiler/runtime drift. Structural pattern, Language Profile and Matcher Profile changes conservatively force the incremental change gate to the full suite.

## Determinism contract

The build result depends only on:

- exact SourceTree bytes
- compiler version/format rules
- explicit build options that are part of semantics
- optional signer output, for the signed artifact variant

The unsigned artifact cannot depend on wall clock, randomness, locale, filesystem enumeration order, absolute path, process environment, hostname, network, or thread scheduling.

Object keys, package graph, entry paths and debug provenance rows have explicit deterministic ordering.

## Signing boundary

`ArtifactSigner` receives only the 32-byte content root. It does not receive SourceTree authority. It returns an opaque algorithm/key-id/signature envelope. GVYA validates envelope bounds but does not invent a trust store. runtime/SDK layer owns signature verification integration and host trust policy.

The canonical CLI keeps key custody external:

```text
gvya signing-root ./brain.gvya
gvya attach-signature ./brain.gvya --envelope ./signature.json --output ./brain.signed.gvya
```

`signing-root` emits the exact content root. The external signer returns a bounded `gvya.signature.input/1` document containing that root plus `algorithm`, `key_id` and the opaque signature. `attach-signature` refuses a root mismatch and refuses to replace an existing signature envelope. Private keys never enter the deterministic compiler core.

## Canonical compiler and artifact inventory

This inventory is normative for the current GVYA architecture; implementation and executable tests must agree with it.

| Concern | Canonical compiler / artifact rule | Proof / location |
|---|---|---|
| Source vs compiled form | source is transparent `gvya.source.*`; `.gvya` has exactly one compiled meaning | `SOURCE_FORMAT.md`, `ARTIFACT_FORMAT.md` |
| Package closure | package/audit/test layer deterministic dependency composition feeds compiler; no load-order override | `pipeline.rs`, package/audit/test layer package model |
| Runtime indexes | derived matcher index is never serialized; compiler builds it only as a validation gate and runtime rebuilds it deterministically from the shipped canonical semantics | `ir/semantic_ir.rs`, `program.rs`, `semantic/index.rs` |
| Structural matcher contract | compiler carries Meaning `patterns` + composed Matcher Profile `pattern_sets`; auditor validates the same kernel-owned matcher build contract used at runtime | `semantic/structural.rs`, `audit/semantics.rs`, `ir/semantic_ir.rs` |
| Portable assets | explicit logical assets, source-byte hashing, bounded safe paths, essential artifact entries | `source.rs`, `artifact.rs` |
| Asset specialization | explicit package/audit/test layer Replace semantics; source byte store keyed by content digest so same AssetId may intentionally resolve to new bytes | `source.rs` |
| Runtime independence | `.gvya` contains runtime-essential program + assets; SDK/runtime packaging deferred runtime/SDK layer | `ir.rs`, `pipeline.rs` |
| Export metadata | no wall clock, random build ID, hostname, locale, absolute path or filesystem enumeration authority | `source.rs`, `pipeline.rs` |
| Archive format | small deterministic GVYA container with strict canonical table/layout and per-entry digest | `artifact.rs`, independent Python oracle |
| Multiple loader layouts | one container/version/path convention only | `artifact.rs` |
| Integrity | per-entry SHA-256 + integrity manifest + manifest digests + distribution hash + optional content-root signing | `artifact.rs`, `pipeline.rs` |
| Signing | optional signer receives only content root; trust policy remains host/runtime/SDK layer authority | `pipeline.rs` |
| JSON source ergonomics | transparent JSON source, real JSON Schema objects, no stringified schema JSON | `source.rs`, `SCHEMA_PROFILE.md` |
| Schema execution | bounded JSON Schema 2020-12 assertion profile compiled into deterministic `ValueSchema` | `schema_compile.rs` |
| Unknown source fields | compiler-owned source objects reject unknown keys and required-field loss | `source.rs` tests |
| Test definitions | typed package/audit/test layer tests are source contributions; malformed expectations fail build rather than weakening tests | `source.rs`, `testing.rs` |
| Deterministic numeric values | IR conversion rejects non-finite model values; never maps them silently to null | `ir.rs` |
| Package list order | project package list is enumeration only; resolved graph/content determines canonical order | `source.rs`, `pipeline.rs` tests |
| Debug/provenance | composition provenance and authored test IDs live only in the optional non-essential source map; the executable program keeps package identity solely because runtime load cross-validates it | `ir.rs`, `pipeline.rs` |
| Build-time tests in runtime | regression/scenario corpus remains build-time; the executable program rejects `tests`, `provenance` and `types` outright rather than parsing and discarding them | `program.rs` boundary tests |
| Runtime/demo bundle | SDK/runtime/demo packaging belongs to the runtime/SDK layer and is never part of `.gvya` meaning | `RUNTIME_ARCHITECTURE.md` |


## Browser compiler edge

`gvya-ffi` exposes transport-only compiler functions from the same single Engine WASM used by runtime operations. The compiler functions accept a bounded deterministic binary source-tree archive and return canonical artifact bytes or bounded diagnostics. They do not own source/package/matcher semantics; that authority remains in `gvya-compiler`.
