# GVYA Godot adapter

Godot is a supported GVYA SDK target through the Web/WASM bridge in this directory. The adapter stays transport-only: semantic matching, conversation state, response selection, capability admission/policy, artifact parsing, resource policy and Why logic remain in the canonical Rust runtime.

## Godot Web

Ship these two files with the Godot Web export:

- `GVYARuntime.gd` in the project scripts;
- `web/gvya-godot-web.js` loaded by the Web export page before the game starts.

Also make the canonical Engine v1 `gvya-ffi.wasm` and the compiled `.gvya` Brain available to the Godot project. `GVYARuntime.open_file(...)` reads both through `FileAccess`; the default Engine path is `res://gvya-ffi.wasm` and can be changed through `web_engine_path`.

The Web bridge performs only ABI transport work. It validates ABI v1/layout, preserves strict `gvya.runtime.open-options/1`, applies the same canonical adapter-side byte ceilings as the TypeScript WASM SDK, and delegates every runtime operation to the same Engine WASM. Binary asset results return as typed-array buffers and are converted through `JavaScriptBridge.js_buffer_to_packed_byte_array(...)`.

The official Godot Web export templates include `JavaScriptBridge` by default. A custom Godot build that disables JavaScriptBridge cannot use this adapter.

Example page integration:

```html
<script src="gvya-godot-web.js"></script>
```

Example GDScript:

```gdscript
var gvya := GVYARuntime.new()
gvya.web_engine_path = "res://gvya-ffi.wasm"
if gvya.open_file("res://assistant.gvya", GVYARuntime.unsigned_development_open_options()):
    var result := gvya.turn({
        "format": "gvya.runtime.turn",
        "version": 1,
        "utterance": {"text": "hello"},
        "context": {},
        "seed": 1
    })
```

Opening always requires an explicit trust policy. `unsigned_development_open_options()` is for local/dev artifacts only; production hosts should use `require_signed_artifact_options()` or pass a stricter `require_verified` policy with host-owned preverification attestation.

## Native Godot

`GVYARuntime.gd` also preserves the native injection boundary: a host can inject an object or register a `GVYANativeRuntime` singleton exposing the same JSON/byte methods. No native GDExtension is shipped in this baseline; the supported built-in Godot path is Web/WASM.

The authoritative runtime boundary is `../../docs/RUNTIME_ARCHITECTURE.md`; wire shapes and budgets are defined only in `../../docs/RUNTIME_WIRE_PROTOCOL.md`.
