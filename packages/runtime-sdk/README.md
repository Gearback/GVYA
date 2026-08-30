# @gvya/runtime-sdk

Thin browser/Node transport SDK for the canonical GVYA Rust runtime. It does not implement semantic matching, conversation state, response selection, capability admission, policy or Why logic in TypeScript.

Use `WasmRuntimeBackend` for browser/Node WebAssembly integration or bind `include/gvya.h` directly from native hosts.

## Browser quickstart

```js
import {
  GvyaRuntime,
  WasmRuntimeBackend,
  requireSignedArtifactOptions,
} from "@gvya/runtime-sdk";

const [wasm, artifact] = await Promise.all([
  fetch("/gvya-runtime-v1.wasm").then((response) => response.arrayBuffer()),
  fetch("/support.gvya").then((response) => response.arrayBuffer()),
]);

const backend = await WasmRuntimeBackend.instantiate(wasm);
const runtime = await GvyaRuntime.open(
  new Uint8Array(artifact),
  backend,
  requireSignedArtifactOptions(),
);

const result = await runtime.turn({
  format: "gvya.runtime.turn",
  version: 1,
  utterance: { text: "hello" },
  context: {
    values: {},
    visible_references: [],
    available_capabilities: [],
  },
  seed: 1,
});
```

The artifact's `runtime.info()` includes its declared `enabled_languages` and required `default_language`. A turn carries text, not a host-selected language. Runtime evaluates that text with every enabled Language/Matcher Profile and uses the language of the winning localized sample or structural pattern for both the response and `state.conversation.active_language`. An unresolved turn keeps the active language, or uses the compiled Bot default in a fresh session. The broader Project authoring catalog is not shipped or consulted, and `und` participates only when explicitly enabled.

`GvyaRuntime.open` deliberately requires an explicit trust policy. `requireSignedArtifactOptions()` rejects unsigned artifacts; the host remains responsible for cryptographically verifying the signature/content root. `unsignedDevelopmentOpenOptions()` exists only for local authoring, Studio simulation, and unsigned development builds.

GVYA Studio’s **Export Web runtime** action produces a tar handoff containing the compiled Brain, canonical single Engine WASM, SDK modules, a minimal browser bootstrap, and deployment notes.

The authoritative runtime/adapter contract is `../../docs/RUNTIME_ARCHITECTURE.md`; wire shapes and budgets are defined only in `../../docs/RUNTIME_WIRE_PROTOCOL.md`.
