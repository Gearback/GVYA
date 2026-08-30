# GVYA Repository Layout

GVYA is a monorepo so compiler/runtime contracts, Studio, SDK surfaces and release validation evolve under one reviewed source change while executable semantics remain canonical in Rust.

## Current layout

```text
gvya/
├─ Cargo.toml
├─ rust-toolchain.toml
├─ crates/
│  ├─ gvya-model/       # shared domain IDs, state and runtime values
│  ├─ gvya-kernel/      # semantic, conversation, capability and Why kernels
│  ├─ gvya-artifact/    # deterministic portable .gvya container
│  ├─ gvya-compiler/
│  ├─ gvya-runtime/     # canonical artifact loader and runtime facade
│  ├─ gvya-ffi/         # stable C/WASM ABI edge
│  └─ gvya-cli/         # canonical scaffold/check/build/audit/test executable
├─ packages/
│  └─ runtime-sdk/          # thin TypeScript runtime adapter over the ABI
├─ apps/
│  └─ studio/           # visual human source editor and simulator
├─ adapters/
│  └─ godot/            # supported Godot Web/WASM SDK adapter over the canonical Engine
├─ include/
│  └─ gvya.h            # public C ABI
├─ validation/          # durable contract/property fixtures and tests
│  └─ authoring-e2e/    # ordinary BASE/CANDIDATE snapshots + external-agent author-step proof runner
├─ tools/               # release/source utilities
│  └─ validate-source.py
└─ docs/                # authoritative current architecture/contracts
```

## Authority and dependency direction

```text
gvya-model
   ↑
gvya-kernel
   ↑        ↑
gvya-compiler   gvya-runtime
   ↑              ↓
 gvya CLI       gvya-ffi
                   ↓
              C / WASM / TypeScript / Godot SDK

Studio ─┐
        ├─> GVYA source -> canonical compiler
Agent  ─┘
```

No JavaScript/GDScript layer owns semantic, conversation or capability authority. Host applications execute admitted capability proposals; GVYA never performs host effects itself.

## Rust responsibilities

### `gvya-model`

Pure shared runtime/domain vocabulary: stable IDs, Values, state, host references and proposals.

### `gvya-kernel`

Canonical deterministic semantic, conversation, response, capability-admission and Why/trace behavior. Optional resolver proposals are untrusted input to this layer, not authority over it.

### `gvya-artifact`

Portable `.gvya` container parsing/emission and artifact resource limits.

### `gvya-compiler`

Compiler-owned source parsing, package resolution/composition, audit, source regression/scenario model and deterministic artifact emission.

### `gvya-runtime`

Loads validated artifacts and executes the canonical kernels. Runtime resource limits are canonical Runtime invariants; JSON/C/WASM adapters apply earlier byte/shape checks and then delegate to the same checked typed Runtime boundary.

### `gvya-ffi`

The only raw-pointer ABI edge. ABI v1 exposes bounded artifact open options and host-owned trust policy; independent runtime handles do not hold the global registry mutex during runtime execution.

### `gvya-cli`

The shipped canonical command surface, enumerated only in `MACHINE_AUTHORING_ARCHITECTURE.md`. Every command calls or prepares the Rust compiler/artifact/runtime contracts directly and does not reproduce semantics in a scripting language. `check-change` compares an immutable accepted baseline with one candidate vertical conversation slice, derives per-mechanic proof obligations, selects the conservative old/new blast radius, and executes the selected suite through the canonical compiled runtime; `author-step` consumes that exact evaluation and exposes the compiler-owned authoring-loop decision as bounded repair/rerun/promote actions; `check` aggregates full-target deterministic measurements and acceptance obligations. None of these commands hosts a model, stores an AI session, edits the candidate, or mutates the accepted baseline. Signing commands keep private-key custody external.

## TypeScript and Studio responsibilities

`packages/runtime-sdk` is a thin typed adapter over the canonical ABI. `apps/studio` is a human editor for GVYA source/workspace concepts, runs authoring-only checks, and uses bundled canonical WASM for simulation. External agents author the same source directly and consume the canonical Rust CLI contracts without a Studio bridge. Model/provider hosting, credentials, prompt/session state, source mutation, and retry execution are not Studio layers; deterministic repair/promote decisions derived from canonical gates remain compiler/CLI responsibilities.

## Adapter scope

Target adapters remain thin edges over the canonical runtime; target-specific status and implementation details belong in the adapter directory, while the runtime authority boundary is defined in `RUNTIME_ARCHITECTURE.md`. Godot Web uses the shipped JavaScriptBridge/WASM adapter and the exact canonical Engine ABI; native Godot may inject a compatible GDExtension singleton but no separate semantic runtime exists.

## Authoring-loop validation

`validation/authoring-e2e/` is validation data and orchestration only, not a runtime or authoring authority. Its fixtures are ordinary GVYA sources. `run.mjs` can validate/build/runtime-smoke them with the shipped Engine and can invoke a supplied canonical `gvya` binary for process-level `author-step` verification. Mechanic detection, blast-radius selection, test selection, and acceptance remain compiler/CLI responsibilities.

## Build and release policy

Release/freeze requirements are defined only in `RELEASE.md`. Structural source validation never substitutes for executable compiler/runtime/WASM/browser certification. Engine WASM production and reuse are defined only in `ENGINE_ASSETS.md`.
