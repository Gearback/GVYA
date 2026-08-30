class_name GVYARuntime
extends RefCounted

## Thin Godot-facing adapter over the canonical GVYA runtime.
## Native builds may inject/register GVYANativeRuntime. Godot Web builds use the shipped
## GVYAGodotWebRuntime JavaScript bridge, which executes the same canonical Engine WASM.
## No matching, conversation, capability, response, policy, or artifact semantics live here.

const DEFAULT_WEB_ENGINE_PATH := "res://gvya-ffi.wasm"

var _native: Object
var _web = null
var web_engine_path: String = DEFAULT_WEB_ENGINE_PATH

func _init(native_runtime: Object = null) -> void:
    _native = native_runtime
    if _native == null and Engine.has_singleton("GVYANativeRuntime"):
        _native = Engine.get_singleton("GVYANativeRuntime")
    if _native == null and OS.has_feature("web"):
        _web = JavaScriptBridge.get_interface("GVYAGodotWebRuntime")
    if _native == null and _web == null:
        push_error("GVYA runtime adapter is unavailable")

func open_file(path: String, options: Dictionary) -> bool:
    if options.is_empty():
        push_error("GVYA requires an explicit artifact trust/open policy")
        return false
    if _native != null:
        var bytes := FileAccess.get_file_as_bytes(path)
        return _native.open_bytes(bytes, JSON.stringify(options))
    if _web != null:
        var engine_bytes := FileAccess.get_file_as_bytes(web_engine_path)
        if engine_bytes.is_empty():
            push_error("GVYA Engine WASM was not found at %s" % web_engine_path)
            return false
        var artifact_bytes := FileAccess.get_file_as_bytes(path)
        if artifact_bytes.is_empty():
            push_error("GVYA artifact was not found or is empty at %s" % path)
            return false
        return bool(_web.open_base64(
            Marshalls.raw_to_base64(engine_bytes),
            Marshalls.raw_to_base64(artifact_bytes),
            JSON.stringify(options)
        ))
    return false

static func unsigned_development_open_options() -> Dictionary:
    return {
        "format": "gvya.runtime.open-options",
        "version": 1,
        "signature": {"mode": "allow_unsigned"}
    }

static func require_signed_artifact_options() -> Dictionary:
    return {
        "format": "gvya.runtime.open-options",
        "version": 1,
        "signature": {"mode": "require_present"}
    }

func info() -> Dictionary:
    if _native != null:
        return _parse_dictionary(_native.info_json())
    if _web != null:
        return _parse_dictionary(_web.info_json())
    return {"error": "gvya_runtime_unavailable"}

func capabilities() -> Dictionary:
    if _native != null:
        return _parse_dictionary(_native.capabilities_json())
    if _web != null:
        return _parse_dictionary(_web.capabilities_json())
    return {"error": "gvya_runtime_unavailable"}

func capability_info(id: String) -> Dictionary:
    if _native != null:
        return _parse_dictionary(_native.capability_info_json(id))
    if _web != null:
        return _parse_dictionary(_web.capability_info_json(id))
    return {"error": "gvya_runtime_unavailable"}

func turn(request: Dictionary) -> Dictionary:
    return _call_json("turn_json", request)

func open_conversation(request: Dictionary) -> Dictionary:
    return _call_json("open_conversation_json", request)

func capability_result(request: Dictionary) -> Dictionary:
    return _call_json("capability_result_json", request)

func asset_by_path(logical_path: String) -> PackedByteArray:
    if _native != null:
        return _native.asset_by_path(logical_path)
    if _web != null:
        return _web_buffer(_web.asset_by_path(logical_path))
    return PackedByteArray()

func asset_by_id(asset_id: String) -> PackedByteArray:
    if _native != null:
        return _native.asset_by_id(asset_id)
    if _web != null:
        return _web_buffer(_web.asset_by_id(asset_id))
    return PackedByteArray()

func asset_info_by_id(asset_id: String) -> Dictionary:
    if _native != null:
        return _parse_dictionary(_native.asset_info_by_id_json(asset_id))
    if _web != null:
        return _parse_dictionary(_web.asset_info_by_id_json(asset_id))
    return {"error": "gvya_runtime_unavailable"}

func last_error() -> Dictionary:
    if _web != null:
        return _parse_dictionary(_web.last_error_json())
    return {}

func close() -> void:
    if _native != null:
        _native.close()
    elif _web != null:
        _web.close()

func _call_json(method: String, request: Dictionary) -> Dictionary:
    var request_json := JSON.stringify(request)
    match method:
        "turn_json":
            if _native != null:
                return _parse_dictionary(_native.turn_json(request_json))
            if _web != null:
                return _parse_dictionary(_web.turn_json(request_json))
        "open_conversation_json":
            if _native != null:
                return _parse_dictionary(_native.open_conversation_json(request_json))
            if _web != null:
                return _parse_dictionary(_web.open_conversation_json(request_json))
        "capability_result_json":
            if _native != null:
                return _parse_dictionary(_native.capability_result_json(request_json))
            if _web != null:
                return _parse_dictionary(_web.capability_result_json(request_json))
    return {"error": "gvya_runtime_unavailable"}

func _web_buffer(value) -> PackedByteArray:
    if value != null and JavaScriptBridge.is_js_buffer(value):
        return JavaScriptBridge.js_buffer_to_packed_byte_array(value)
    return PackedByteArray()

func _parse_dictionary(response_json: String) -> Dictionary:
    var parsed = JSON.parse_string(response_json)
    if typeof(parsed) != TYPE_DICTIONARY:
        return {"error": "gvya_invalid_runtime_response"}
    return parsed
