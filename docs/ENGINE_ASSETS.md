# GVYA Engine Assets

GVYA Engine assets are versioned executable implementation assets owned by the product, not by Bots or Packages.

## Engine v1

A shipped Studio build carries exactly **one** canonical Rust WASM module for Engine v1: `gvya-ffi.wasm`.

That module is produced by `gvya-ffi` and exposes both thin transport surfaces over the same canonical Rust authority graph:

- source validation / compilation through `gvya-compiler`;
- artifact loading / deterministic runtime through `gvya-runtime`.

There is no separate browser compiler module and no separate browser runtime module. The shared Engine ABI owns memory transport only; compiler/runtime semantics stay in their canonical Rust crates.

Bot/Package authoring never rebuilds the Engine. Engine WASM changes only when executable compiler/runtime semantics or the Engine ABI change. Authored content is data: Studio compiles the selected resolved source tree into a transient `.gvya` and immediately opens it through another instance of the same precompiled `WebAssembly.Module`.

`tools/build_engine_assets.py` is the explicit Engine build operation. It builds `gvya-ffi` once for `wasm32-unknown-unknown` under the dedicated `engine-wasm` Cargo profile, copies the single output into Studio's Engine v1 directory, and writes a manifest containing ABI, exact byte length and SHA-256. Normal Studio build/simulate/export never invokes Cargo.

## Production build profile

The shipped Engine is the only executable asset GVYA distributes, so it has its own named Cargo profile rather than borrowing `release`:

```toml
[profile.engine-wasm]
inherits = "release"
opt-level = 3
lto = true
codegen-units = 1
strip = "symbols"
```

`strip = "symbols"` removes the Rust `name` symbol table and the DWARF sections; those are development metadata no runtime or ABI consumer reads. `tools/verify_engine_assets.py` fails closed if a `name` or `.debug*` custom section reappears, so a development module cannot be shipped by accident.

`opt-level` deliberately stays at the release default. Measured on the real bilingual Bot (1,023 Meanings, 300 turns through the WASM runtime):

| profile | raw | gzip | brotli | ms/turn |
|---|---|---|---|---|
| plain `release` | 4,293,597 | 1,082,397 | 697,470 | 98.6 |
| `engine-wasm` | 3,530,411 | 1,025,726 | 668,316 | 95.8 |
| `engine-wasm` + `opt-level = "s"` | 2,772,207 | 715,592 | 498,507 | 126.0 |
| `engine-wasm` + `opt-level = "z"` | 2,525,889 | 641,436 | 450,891 | 228.8 |

Size-first optimization levels cost 31% and 138% per-turn latency respectively; a conversational engine pays that on every turn while the download is paid once. `panic = "abort"` was measured and rejected as well: `wasm32-unknown-unknown` already aborts on panic, so it changed nothing. External post-processing such as `wasm-opt -Oz` is not used because this repository pins exactly one toolchain (`rust-toolchain.toml`) and has no reproducible pinning mechanism for a second external optimizer; adding one is a toolchain-contract decision, not a build tweak.

## Studio rule

Simulation is source-native and zero-setup:

`selected source -> one bundled Engine module -> compiler instance -> transient .gvya -> runtime instance of same module -> turn`

The human never selects, locates, uploads, or configures Engine WASM or an intermediate `.gvya` artifact. Studio validates the internal Engine manifest and digest before execution. Verified bytes are compiled to `WebAssembly.Module` exactly once per Studio page lifetime and reused for compiler and runtime instances; authored source changes only invalidate the transient artifact/session.

Build/export uses the same asset. `.gvya` emission uses the compiler exports of the bundled Engine; Web runtime export copies those exact Engine bytes. There is no JavaScript matcher/compiler/runtime fallback.

## Versioning

The current executable Engine identity is exactly `v1`. The version is internal product state rather than an authoring setting. During pre-freeze clean-break development, v1 may be rebuilt in place when executable semantics or ABI change, provided Studio, docs, validation, manifest and Engine bytes move together in the same baseline. Once an Engine version is explicitly frozen/published, any incompatible executable semantic/ABI/artifact change requires a new Engine version.

Bot content, Package content, language data, matcher vocabulary, tests, or authored configuration do not justify an Engine-version change. GVYA currently has no backward-compatibility obligation.

## Pre-WASM source state

A clean source baseline may intentionally contain no files under `apps/studio/public/engine/v1/` before an Engine rebuild. This prevents stale executable bytes or a stale manifest from being mislabeled as current semantics. `tools/build_engine_assets.py` atomically produces `gvya-ffi.wasm` plus its manifest; `tools/verify_engine_assets.py` requires that exact single-module set and rejects obsolete split assets.

## Acceptance

A distributable Studio baseline ships the pinned Engine v1 module. `npm run engine:accept:v1` is the headless executable proof: verify the pinned asset, compile the real Studio starter Bot through one Engine instance, open the transient artifact through another instance of the same module, send `hello`, and require the canonical greeting meaning/behavior/response. Browser acceptance remains a separate rendered-UI proof.

## Browser cache integrity

Studio fetches the current Engine manifest without trusting a stale manifest cache. The WASM request is content-addressed with its SHA-256 in the URL. If the browser nevertheless returns mismatched cached bytes, Studio retries once with a cache-busting URL and `no-store`, then applies the same byte-length and SHA-256 verification. A second mismatch fails closed. Integrity verification is never skipped or weakened.
