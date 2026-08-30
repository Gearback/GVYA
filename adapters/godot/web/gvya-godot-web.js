(function (root) {
  "use strict";

  const encoder = new TextEncoder();
  const decoder = new TextDecoder();
  const DEFAULT_MAX_ARTIFACT_BYTES = 512 * 1024 * 1024;
  const MAX_OPEN_OPTIONS_BYTES = 64 * 1024;
  const MAX_RUNTIME_REQUEST_BYTES = 1024 * 1024;
  const CANONICAL_ARTIFACT_LIMITS = Object.freeze({
    max_entries: 16_384,
    max_path_bytes: 512,
    max_entry_bytes: 64 * 1024 * 1024,
    max_total_bytes: DEFAULT_MAX_ARTIFACT_BYTES,
    max_manifest_bytes: 8 * 1024 * 1024,
    max_integrity_bytes: 8 * 1024 * 1024,
    max_signature_bytes: 256 * 1024,
    max_debug_map_bytes: 16 * 1024 * 1024,
  });

  class GvyaGodotWasmRuntime {
    #exports = null;
    #handle = null;
    #lastError = null;

    open_base64(wasmBase64, artifactBase64, optionsJson) {
      try {
        return this.open_bytes(decodeBase64(wasmBase64), decodeBase64(artifactBase64), optionsJson);
      } catch (error) {
        this.#setError(error);
        return false;
      }
    }

    open_bytes(wasmBytes, artifactBytes, optionsJson) {
      try {
        if (this.#handle !== null) throw new Error("GVYA runtime is already open");
        const wasm = asUint8Array(wasmBytes, "engine WASM");
        const artifact = asUint8Array(artifactBytes, "artifact");
        const options = parseJsonObject(optionsJson, "runtime open options");
        validateOpenOptions(options);
        const maxArtifactBytes = options.artifact_limits?.max_total_bytes ?? DEFAULT_MAX_ARTIFACT_BYTES;
        if (artifact.byteLength > maxArtifactBytes) {
          throw new Error("GVYA artifact exceeds configured total byte limit before WASM copy");
        }
        const optionsBytes = encoder.encode(JSON.stringify(options));
        if (optionsBytes.byteLength > MAX_OPEN_OPTIONS_BYTES) {
          throw new Error("GVYA runtime open options exceed byte limit");
        }

        const module = new WebAssembly.Module(wasm);
        const instance = new WebAssembly.Instance(module, {});
        const exports = instance.exports;
        validateAbi(exports);
        this.#exports = exports;

        const artifactPtr = this.#copyIn(artifact);
        const optionsPtr = this.#copyIn(optionsBytes);
        const handlePtr = this.#alloc(8);
        const outPtr = this.#alloc(12);
        try {
          const code = exports.gvya_runtime_open_with_options_json(
            artifactPtr,
            artifact.length,
            optionsPtr,
            optionsBytes.length,
            handlePtr,
            outPtr,
          );
          const message = this.#takeBuffer(outPtr);
          if (code !== 0) throw new Error(this.#errorMessage(code, message));
          this.#handle = new DataView(exports.memory.buffer).getBigUint64(handlePtr, true);
          if (this.#handle === 0n) throw new Error("GVYA runtime returned an invalid handle");
        } finally {
          exports.gvya_dealloc(artifactPtr, artifact.length);
          exports.gvya_dealloc(optionsPtr, optionsBytes.length);
          exports.gvya_dealloc(handlePtr, 8);
          exports.gvya_dealloc(outPtr, 12);
        }
        this.#lastError = null;
        return true;
      } catch (error) {
        this.#setError(error);
        this.#handle = null;
        this.#exports = null;
        return false;
      }
    }

    is_open() {
      return this.#handle !== null;
    }

    last_error_json() {
      return JSON.stringify(this.#lastError ?? { error: null });
    }

    info_json() {
      return this.#jsonNoInput("runtime info", "gvya.runtime.info", "gvya_runtime_info_json");
    }

    capabilities_json() {
      return this.#jsonNoInput("capabilities", "gvya.runtime.capabilities", "gvya_runtime_capabilities_json");
    }

    capability_info_json(id) {
      return this.#jsonInputBytes("capability info", "gvya.runtime.capability-info", "gvya_runtime_capability_info_json", encoder.encode(String(id)));
    }

    turn_json(requestJson) {
      return this.#jsonInput("turn", "gvya.runtime.turn-result", "gvya_runtime_turn_json", requestJson);
    }

    open_conversation_json(requestJson) {
      return this.#jsonInput("conversation open", "gvya.runtime.turn-result", "gvya_runtime_open_conversation_json", requestJson);
    }

    capability_result_json(requestJson) {
      return this.#jsonInput("capability result", "gvya.runtime.capability-result-result", "gvya_runtime_capability_result_json", requestJson);
    }

    asset_by_path(path) {
      return this.#bytesInput("asset path", "gvya_runtime_asset_by_path", encoder.encode(String(path)));
    }

    asset_by_id(id) {
      return this.#bytesInput("asset id", "gvya_runtime_asset_by_id", encoder.encode(String(id)));
    }

    asset_info_by_id_json(id) {
      return this.#jsonInputBytes("asset info", "gvya.runtime.asset-info", "gvya_runtime_asset_info_by_id_json", encoder.encode(String(id)));
    }

    close() {
      if (this.#handle === null) return true;
      try {
        const code = this.#exports.gvya_runtime_close(this.#handle);
        if (code !== 0) throw new Error(`GVYA close failed with code ${code}`);
        this.#handle = null;
        this.#lastError = null;
        return true;
      } catch (error) {
        this.#setError(error);
        return false;
      }
    }

    #jsonNoInput(label, format, exportName) {
      try {
        const handle = this.#requiredHandle();
        const outPtr = this.#alloc(12);
        try {
          const code = this.#exports[exportName](handle, outPtr);
          const response = this.#takeBuffer(outPtr);
          if (code !== 0) throw new Error(this.#errorMessage(code, response));
          const json = decoder.decode(response);
          assertWireFormat(json, format, label);
          this.#lastError = null;
          return json;
        } finally {
          this.#exports.gvya_dealloc(outPtr, 12);
        }
      } catch (error) {
        return this.#errorJson(error);
      }
    }

    #jsonInput(label, format, exportName, requestJson) {
      try {
        const requestBytes = encoder.encode(String(requestJson));
        if (requestBytes.byteLength > MAX_RUNTIME_REQUEST_BYTES) {
          throw new Error("GVYA runtime input exceeds canonical request byte limit before WASM copy");
        }
        return this.#jsonInputBytes(label, format, exportName, requestBytes);
      } catch (error) {
        return this.#errorJson(error);
      }
    }

    #jsonInputBytes(label, format, exportName, bytes) {
      try {
        const response = this.#callInput(exportName, bytes);
        const json = decoder.decode(response);
        assertWireFormat(json, format, label);
        this.#lastError = null;
        return json;
      } catch (error) {
        return this.#errorJson(error);
      }
    }

    #bytesInput(label, exportName, bytes) {
      try {
        const response = this.#callInput(exportName, bytes);
        this.#lastError = null;
        return response;
      } catch (error) {
        this.#setError(new Error(`GVYA ${label} failed: ${messageOf(error)}`));
        return new Uint8Array();
      }
    }

    #callInput(exportName, bytes) {
      const handle = this.#requiredHandle();
      if (bytes.byteLength > MAX_RUNTIME_REQUEST_BYTES) {
        throw new Error("GVYA runtime input exceeds canonical request byte limit before WASM copy");
      }
      const ptr = this.#copyIn(bytes);
      const outPtr = this.#alloc(12);
      try {
        const code = this.#exports[exportName](handle, ptr, bytes.length, outPtr);
        const response = this.#takeBuffer(outPtr);
        if (code !== 0) throw new Error(this.#errorMessage(code, response));
        return response;
      } finally {
        this.#exports.gvya_dealloc(ptr, bytes.length);
        this.#exports.gvya_dealloc(outPtr, 12);
      }
    }

    #requiredHandle() {
      if (this.#handle === null || this.#exports === null) throw new Error("GVYA runtime is not open");
      return this.#handle;
    }

    #alloc(length) {
      const ptr = this.#exports.gvya_alloc(length);
      if (length > 0 && ptr === 0) throw new Error("GVYA WASM allocation failed");
      return ptr;
    }

    #copyIn(bytes) {
      const ptr = this.#alloc(bytes.length);
      if (bytes.length > 0) new Uint8Array(this.#exports.memory.buffer, ptr, bytes.length).set(bytes);
      return ptr;
    }

    #takeBuffer(outPtr) {
      const view = new DataView(this.#exports.memory.buffer);
      const ptr = view.getUint32(outPtr, true);
      const len = view.getUint32(outPtr + 4, true);
      const capacity = view.getUint32(outPtr + 8, true);
      if (len > capacity) {
        this.#exports.gvya_buffer_free(outPtr);
        throw new Error("GVYA returned a corrupt output buffer");
      }
      const copy = ptr === 0 || len === 0
        ? new Uint8Array()
        : new Uint8Array(new Uint8Array(this.#exports.memory.buffer, ptr, len));
      this.#exports.gvya_buffer_free(outPtr);
      return copy;
    }

    #errorMessage(code, bytes) {
      if (bytes.length === 0) return `GVYA call failed with code ${code}`;
      try {
        const value = JSON.parse(decoder.decode(bytes));
        if (typeof value.error === "string") return `GVYA ${code}: ${value.error}`;
      } catch (_) {
        // raw fallback below
      }
      return `GVYA ${code}: ${decoder.decode(bytes)}`;
    }

    #setError(error) {
      this.#lastError = { error: messageOf(error) };
    }

    #errorJson(error) {
      this.#setError(error);
      return JSON.stringify(this.#lastError);
    }
  }

  function asUint8Array(value, label) {
    if (value instanceof Uint8Array) return value;
    if (value instanceof ArrayBuffer) return new Uint8Array(value);
    if (ArrayBuffer.isView(value)) return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
    throw new Error(`GVYA ${label} must be a byte buffer`);
  }

  function decodeBase64(value) {
    if (typeof value !== "string") throw new Error("GVYA base64 transport requires a string");
    if (typeof Buffer !== "undefined") return new Uint8Array(Buffer.from(value, "base64"));
    const binary = root.atob(value);
    const out = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) out[index] = binary.charCodeAt(index);
    return out;
  }

  function parseJsonObject(value, label) {
    let parsed;
    try {
      parsed = typeof value === "string" ? JSON.parse(value) : value;
    } catch (_) {
      throw new Error(`GVYA ${label} must be valid JSON`);
    }
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
      throw new Error(`GVYA ${label} must be a JSON object`);
    }
    return parsed;
  }

  function validateOpenOptions(options) {
    if (options.format !== "gvya.runtime.open-options" || options.version !== 1) {
      throw new Error("GVYA runtime requires gvya.runtime.open-options/1");
    }
    if (options.signature !== undefined) {
      const mode = options.signature?.mode;
      if (!["allow_unsigned", "require_present", "require_verified"].includes(mode)) {
        throw new Error("GVYA runtime open signature mode is invalid");
      }
    }
    const limits = options.artifact_limits;
    if (limits !== undefined) {
      if (typeof limits !== "object" || limits === null || Array.isArray(limits)) {
        throw new Error("GVYA artifact_limits must be an object");
      }
      for (const [name, ceiling] of Object.entries(CANONICAL_ARTIFACT_LIMITS)) {
        const value = limits[name];
        if (value === undefined) continue;
        if (!Number.isSafeInteger(value) || value <= 0) throw new Error(`GVYA ${name} must be a positive safe integer`);
        if (value > ceiling) throw new Error(`GVYA ${name} cannot exceed the canonical ceiling`);
      }
    }
  }

  function validateAbi(exports) {
    const required = [
      "memory",
      "gvya_abi_version",
      "gvya_pointer_width",
      "gvya_buffer_struct_size",
      "gvya_alloc",
      "gvya_dealloc",
      "gvya_runtime_open_with_options_json",
      "gvya_runtime_close",
      "gvya_runtime_info_json",
      "gvya_runtime_capabilities_json",
      "gvya_runtime_capability_info_json",
      "gvya_runtime_turn_json",
      "gvya_runtime_open_conversation_json",
      "gvya_runtime_capability_result_json",
      "gvya_runtime_asset_by_path",
      "gvya_runtime_asset_by_id",
      "gvya_runtime_asset_info_by_id_json",
      "gvya_buffer_free",
    ];
    for (const name of required) {
      if (!(name in exports)) throw new Error(`GVYA WASM is missing required export ${name}`);
    }
    if (exports.gvya_abi_version() !== 1) throw new Error("Unsupported GVYA ABI version");
    if (exports.gvya_pointer_width() !== 32 || exports.gvya_buffer_struct_size() !== 12) {
      throw new Error("GVYA Godot Web adapter requires the wasm32 ABI layout");
    }
  }

  function assertWireFormat(json, format, label) {
    let value;
    try {
      value = JSON.parse(json);
    } catch (_) {
      throw new Error(`GVYA returned invalid JSON for ${label}`);
    }
    if (typeof value !== "object" || value === null || value.format !== format || value.version !== 1) {
      throw new Error(`GVYA returned an unsupported ${label}`);
    }
  }

  function messageOf(error) {
    return error instanceof Error ? error.message : String(error);
  }

  const runtime = new GvyaGodotWasmRuntime();
  root.GVYAGodotWebRuntime = runtime;
  if (typeof module !== "undefined" && module.exports) module.exports = runtime;
})(typeof globalThis !== "undefined" ? globalThis : window);
