//! Minimal stable C/WASM ABI over the canonical Rust runtime.
//!
//! Safety boundary: raw pointers are copied immediately into owned Rust buffers. No pointer or
//! host callback becomes semantic/conversation/capability authority.

use std::{
    collections::BTreeMap,
    slice,
    sync::{Arc, Mutex, OnceLock},
};

use gvya_model::{AssetId, CapabilityId};
use gvya_runtime::{
    LoadPolicy, ProgramLimits, Runtime, SignatureEnvelope, SignatureVerifier, TrustStatus,
    wire::{
        parse_capability_result_request, parse_open_request, parse_turn_request,
        serialize_asset_info, serialize_capabilities, serialize_capability_info,
        serialize_capability_result_result_with_limits, serialize_runtime_info,
        serialize_turn_result_with_limits,
    },
};
use serde::Deserialize;

mod compiler;

pub const GVYA_ABI_VERSION: u32 = 1;
const OK: i32 = 0;
const INVALID_ARGUMENT: i32 = 1;
const OPEN_FAILED: i32 = 2;
const HANDLE_NOT_FOUND: i32 = 3;
const WIRE_FAILED: i32 = 4;
const INTERNAL_ERROR: i32 = 5;
const NOT_FOUND: i32 = 6;

#[repr(C)]
pub struct GvyaBuffer {
    pub ptr: *mut u8,
    pub len: usize,
    pub capacity: usize,
}
impl Default for GvyaBuffer {
    fn default() -> Self {
        Self {
            ptr: std::ptr::null_mut(),
            len: 0,
            capacity: 0,
        }
    }
}

struct Registry {
    next: u64,
    runtimes: BTreeMap<u64, Arc<Runtime>>,
}
fn registry() -> &'static Mutex<Registry> {
    static VALUE: OnceLock<Mutex<Registry>> = OnceLock::new();
    VALUE.get_or_init(|| {
        Mutex::new(Registry {
            next: 1,
            runtimes: BTreeMap::new(),
        })
    })
}

fn allocate_handle(registry: &mut Registry) -> Option<u64> {
    if registry.runtimes.len() == usize::MAX {
        return None;
    }
    let start = registry.next.max(1);
    let mut candidate = start;
    loop {
        if !registry.runtimes.contains_key(&candidate) {
            registry.next = if candidate == u64::MAX {
                1
            } else {
                candidate + 1
            };
            return Some(candidate);
        }
        candidate = if candidate == u64::MAX {
            1
        } else {
            candidate + 1
        };
        if candidate == start {
            return None;
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn gvya_abi_version() -> u32 {
    GVYA_ABI_VERSION
}

#[unsafe(no_mangle)]
pub extern "C" fn gvya_pointer_width() -> u32 {
    usize::BITS
}

#[unsafe(no_mangle)]
pub extern "C" fn gvya_buffer_struct_size() -> usize {
    std::mem::size_of::<GvyaBuffer>()
}

/// Adapter scratch allocation. Ownership stays tracked by GVYA; callers release the returned
/// pointer with `gvya_dealloc`. The legacy `len` parameter is advisory only and is never trusted
/// to reconstruct an allocation.
#[unsafe(no_mangle)]
pub extern "C" fn gvya_alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::null_mut();
    }
    let mut value = vec![0_u8; len].into_boxed_slice();
    let ptr = value.as_mut_ptr();
    let Ok(mut registry) = scratch_allocations().lock() else {
        return std::ptr::null_mut();
    };
    if registry.insert(ptr as usize, value).is_some() {
        return std::ptr::null_mut();
    }
    ptr
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn gvya_dealloc(ptr: *mut u8, _len: usize) {
    if ptr.is_null() {
        return;
    }
    if let Ok(mut registry) = scratch_allocations().lock() {
        // Dropping the tracked Box owns the deallocation. Fabricated/double-free pointers are
        // ignored instead of becoming inputs to Box::from_raw.
        registry.remove(&(ptr as usize));
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenOptionsDoc {
    format: String,
    version: u32,
    #[serde(default)]
    artifact_limits: ArtifactLimitsDoc,
    #[serde(default)]
    program_limits: ProgramLimitsDoc,
    #[serde(default)]
    signature: SignaturePolicyDoc,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ArtifactLimitsDoc {
    max_entries: Option<usize>,
    max_path_bytes: Option<usize>,
    max_entry_bytes: Option<usize>,
    max_total_bytes: Option<usize>,
    max_manifest_bytes: Option<usize>,
    max_integrity_bytes: Option<usize>,
    max_signature_bytes: Option<usize>,
    max_debug_map_bytes: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ProgramLimitsDoc {
    max_program_bytes: Option<usize>,
    max_depth: Option<usize>,
    max_nodes: Option<usize>,
    max_collection_entries: Option<usize>,
    max_string_bytes: Option<usize>,
    max_packages: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SignaturePolicyDoc {
    #[serde(default = "allow_unsigned_mode")]
    mode: String,
    preverified: Option<PreverifiedSignatureDoc>,
}
impl Default for SignaturePolicyDoc {
    fn default() -> Self {
        Self {
            mode: allow_unsigned_mode(),
            preverified: None,
        }
    }
}
fn allow_unsigned_mode() -> String {
    "allow_unsigned".into()
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreverifiedSignatureDoc {
    content_root: String,
    algorithm: String,
    key_id: String,
    signature: String,
}

struct PreverifiedVerifier {
    expected: PreverifiedSignatureDoc,
}
impl SignatureVerifier for PreverifiedVerifier {
    fn verify(&self, content_root: [u8; 32], envelope: &SignatureEnvelope) -> Result<(), String> {
        let actual_root = content_root
            .iter()
            .map(|value| format!("{value:02x}"))
            .collect::<String>();
        if actual_root != self.expected.content_root
            || envelope.algorithm != self.expected.algorithm
            || envelope.key_id != self.expected.key_id
            || envelope.signature != self.expected.signature
        {
            return Err(
                "preverified signature attestation does not match the validated artifact envelope"
                    .into(),
            );
        }
        Ok(())
    }
}

const OPEN_OPTIONS_MAX_BYTES: usize = 64 * 1024;
const FFI_TEXT_INPUT_MAX_BYTES: usize = 64 * 1024;
/// Process-wide safety ceiling. Hosts needing a larger fleet should shard across explicit processes
/// rather than growing an implicit global ownership root without bound.
const FFI_MAX_OPEN_RUNTIMES: usize = 256;

fn owned_buffers() -> &'static Mutex<BTreeMap<usize, Vec<u8>>> {
    static VALUE: OnceLock<Mutex<BTreeMap<usize, Vec<u8>>>> = OnceLock::new();
    VALUE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn scratch_allocations() -> &'static Mutex<BTreeMap<usize, Box<[u8]>>> {
    static VALUE: OnceLock<Mutex<BTreeMap<usize, Box<[u8]>>>> = OnceLock::new();
    VALUE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// ABI v1 open boundary. Signature cryptography remains host-owned. `preverified` is an explicit
/// host attestation for the exact content root + signature envelope that the host has already
/// verified; GVYA binds that attestation to the parsed artifact before reporting `verified` trust.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gvya_runtime_open_with_options_json(
    artifact_ptr: *const u8,
    artifact_len: usize,
    options_ptr: *const u8,
    options_len: usize,
    out_handle: *mut u64,
    out_message: *mut GvyaBuffer,
) -> i32 {
    if artifact_ptr.is_null() || options_ptr.is_null() || out_handle.is_null() {
        return INVALID_ARGUMENT;
    }
    unsafe {
        std::ptr::write_unaligned(out_handle, 0);
    }
    reset_output(out_message);
    if options_len > OPEN_OPTIONS_MAX_BYTES {
        return write_error(
            out_message,
            INVALID_ARGUMENT,
            "runtime open options exceed byte limit",
        );
    }
    let options_raw = unsafe { slice::from_raw_parts(options_ptr, options_len) };
    let options: OpenOptionsDoc = match serde_json::from_slice(options_raw) {
        Ok(value) => value,
        Err(error) => {
            return write_error(
                out_message,
                INVALID_ARGUMENT,
                &format!("invalid runtime open options: {error}"),
            );
        }
    };
    if options.format != "gvya.runtime.open-options" || options.version != 1 {
        return write_error(
            out_message,
            INVALID_ARGUMENT,
            "unsupported runtime open options format",
        );
    }
    let mut policy = LoadPolicy::default();
    if let Some(value) = options.artifact_limits.max_entries {
        policy.artifact_limits.max_entries = value;
    }
    if let Some(value) = options.artifact_limits.max_path_bytes {
        policy.artifact_limits.max_path_bytes = value;
    }
    if let Some(value) = options.artifact_limits.max_entry_bytes {
        policy.artifact_limits.max_entry_bytes = value;
    }
    if let Some(value) = options.artifact_limits.max_total_bytes {
        policy.artifact_limits.max_total_bytes = value;
    }
    if let Some(value) = options.artifact_limits.max_manifest_bytes {
        policy.artifact_limits.max_manifest_bytes = value;
    }
    if let Some(value) = options.artifact_limits.max_integrity_bytes {
        policy.artifact_limits.max_integrity_bytes = value;
    }
    if let Some(value) = options.artifact_limits.max_signature_bytes {
        policy.artifact_limits.max_signature_bytes = value;
    }
    if let Some(value) = options.artifact_limits.max_debug_map_bytes {
        policy.artifact_limits.max_debug_map_bytes = value;
    }
    if let Some(value) = options.program_limits.max_program_bytes {
        policy.program_limits.max_program_bytes = value;
    }
    if let Some(value) = options.program_limits.max_depth {
        policy.program_limits.max_depth = value;
    }
    if let Some(value) = options.program_limits.max_nodes {
        policy.program_limits.max_nodes = value;
    }
    if let Some(value) = options.program_limits.max_collection_entries {
        policy.program_limits.max_collection_entries = value;
    }
    if let Some(value) = options.program_limits.max_string_bytes {
        policy.program_limits.max_string_bytes = value;
    }
    if let Some(value) = options.program_limits.max_packages {
        policy.program_limits.max_packages = value;
    }
    if policy.program_limits.max_program_bytes == 0
        || policy.program_limits.max_depth == 0
        || policy.program_limits.max_nodes == 0
        || policy.program_limits.max_collection_entries == 0
        || policy.program_limits.max_string_bytes == 0
        || policy.program_limits.max_packages == 0
    {
        return write_error(
            out_message,
            INVALID_ARGUMENT,
            "program limits must be non-zero",
        );
    }
    let canonical_program_limits = ProgramLimits::default();
    if policy.program_limits.max_program_bytes > canonical_program_limits.max_program_bytes
        || policy.program_limits.max_depth > canonical_program_limits.max_depth
        || policy.program_limits.max_nodes > canonical_program_limits.max_nodes
        || policy.program_limits.max_collection_entries
            > canonical_program_limits.max_collection_entries
        || policy.program_limits.max_string_bytes > canonical_program_limits.max_string_bytes
        || policy.program_limits.max_packages > canonical_program_limits.max_packages
    {
        return write_error(
            out_message,
            INVALID_ARGUMENT,
            "program limits may tighten but not relax canonical executable ceilings",
        );
    }
    if let Err(error) = policy.artifact_limits.validate() {
        return write_error(
            out_message,
            INVALID_ARGUMENT,
            &format!("invalid artifact limits: {error:?}"),
        );
    }
    if artifact_len > policy.artifact_limits.max_total_bytes {
        return write_error(
            out_message,
            OPEN_FAILED,
            "artifact exceeds configured total byte limit",
        );
    }
    let require_present = match options.signature.mode.as_str() {
        "allow_unsigned" => false,
        "require_present" => true,
        "require_verified" => {
            policy.require_signature = true;
            true
        }
        _ => {
            return write_error(
                out_message,
                INVALID_ARGUMENT,
                "unknown signature policy mode",
            );
        }
    };
    if options.signature.mode == "require_verified" && options.signature.preverified.is_none() {
        return write_error(
            out_message,
            INVALID_ARGUMENT,
            "require_verified needs an explicit preverified signature attestation",
        );
    }
    if let Some(attestation) = &options.signature.preverified {
        if attestation.content_root.len() != 64
            || !attestation
                .content_root
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || attestation.algorithm.is_empty()
            || attestation.key_id.is_empty()
            || attestation.signature.is_empty()
            || attestation.algorithm.len() > 1024
            || attestation.key_id.len() > 4096
            || attestation.signature.len() > 16 * 1024
        {
            return write_error(
                out_message,
                INVALID_ARGUMENT,
                "invalid preverified signature attestation",
            );
        }
    }
    let verifier = options
        .signature
        .preverified
        .clone()
        .map(|expected| PreverifiedVerifier { expected });
    {
        let Ok(guard) = registry().lock() else {
            return write_error(out_message, INTERNAL_ERROR, "runtime registry poisoned");
        };
        if guard.runtimes.len() >= FFI_MAX_OPEN_RUNTIMES {
            return write_error(
                out_message,
                OPEN_FAILED,
                "runtime registry reached its canonical open-runtime limit",
            );
        }
    }
    let artifact = unsafe { slice::from_raw_parts(artifact_ptr, artifact_len) }.to_vec();
    match Runtime::load(
        artifact,
        policy,
        verifier
            .as_ref()
            .map(|value| value as &dyn SignatureVerifier),
    ) {
        Ok(runtime) => {
            if require_present && matches!(runtime.trust(), TrustStatus::Unsigned) {
                return write_error(
                    out_message,
                    OPEN_FAILED,
                    "signature policy requires a signature",
                );
            }
            if options.signature.preverified.is_some()
                && !matches!(runtime.trust(), TrustStatus::Verified { .. })
            {
                return write_error(
                    out_message,
                    OPEN_FAILED,
                    "preverified signature attestation was not consumed by a matching artifact signature",
                );
            }
            let Ok(mut guard) = registry().lock() else {
                return write_error(out_message, INTERNAL_ERROR, "runtime registry poisoned");
            };
            if guard.runtimes.len() >= FFI_MAX_OPEN_RUNTIMES {
                return write_error(
                    out_message,
                    OPEN_FAILED,
                    "runtime registry reached its canonical open-runtime limit",
                );
            }
            let Some(handle) = allocate_handle(&mut guard) else {
                return write_error(
                    out_message,
                    INTERNAL_ERROR,
                    "runtime handle space exhausted",
                );
            };
            guard.runtimes.insert(handle, Arc::new(runtime));
            unsafe {
                std::ptr::write_unaligned(out_handle, handle);
            }
            OK
        }
        Err(error) => write_error(out_message, OPEN_FAILED, &format!("{error:?}")),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn gvya_runtime_close(handle: u64) -> i32 {
    let Ok(mut guard) = registry().lock() else {
        return INTERNAL_ERROR;
    };
    if guard.runtimes.remove(&handle).is_some() {
        OK
    } else {
        HANDLE_NOT_FOUND
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn gvya_runtime_info_json(handle: u64, out: *mut GvyaBuffer) -> i32 {
    if out.is_null() {
        return INVALID_ARGUMENT;
    }
    reset_output(out);
    with_runtime(handle, out, |runtime| serialize_runtime_info(runtime))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn gvya_runtime_capabilities_json(handle: u64, out: *mut GvyaBuffer) -> i32 {
    if out.is_null() {
        return INVALID_ARGUMENT;
    }
    reset_output(out);
    with_runtime(handle, out, |runtime| serialize_capabilities(runtime))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn gvya_runtime_capability_info_json(
    handle: u64,
    id_ptr: *const u8,
    id_len: usize,
    out: *mut GvyaBuffer,
) -> i32 {
    if id_ptr.is_null() || out.is_null() {
        return INVALID_ARGUMENT;
    }
    reset_output(out);
    if id_len > FFI_TEXT_INPUT_MAX_BYTES {
        return write_error(out, INVALID_ARGUMENT, "capability id exceeds byte limit");
    }
    let raw = unsafe { slice::from_raw_parts(id_ptr, id_len) };
    let Ok(id) = std::str::from_utf8(raw) else {
        return write_error(out, INVALID_ARGUMENT, "capability id is not UTF-8");
    };
    let runtime = {
        let Ok(guard) = registry().lock() else {
            return write_error(out, INTERNAL_ERROR, "runtime registry poisoned");
        };
        let Some(runtime) = guard.runtimes.get(&handle) else {
            return write_error(out, HANDLE_NOT_FOUND, "runtime handle not found");
        };
        Arc::clone(runtime)
    };
    match serialize_capability_info(runtime.as_ref(), &CapabilityId::new(id)) {
        Ok(Some(bytes)) => write_buffer(out, bytes),
        Ok(None) => write_error(out, NOT_FOUND, "capability not found"),
        Err(error) => write_error(out, WIRE_FAILED, &format!("{error:?}")),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn gvya_runtime_turn_json(
    handle: u64,
    request_ptr: *const u8,
    request_len: usize,
    out: *mut GvyaBuffer,
) -> i32 {
    if request_ptr.is_null() || out.is_null() {
        return INVALID_ARGUMENT;
    }
    reset_output(out);
    let request = unsafe { slice::from_raw_parts(request_ptr, request_len) };
    let request = match parse_turn_request(request) {
        Ok(value) => value,
        Err(error) => return write_error(out, WIRE_FAILED, &format!("{error:?}")),
    };
    with_runtime(handle, out, |runtime| match runtime.turn(request) {
        Ok(output) => serialize_turn_result_with_limits(&output, runtime.limits()),
        Err(error) => Err(gvya_runtime::wire::WireError::Invalid(format!(
            "runtime request rejected: {error:?}"
        ))),
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn gvya_runtime_open_conversation_json(
    handle: u64,
    request_ptr: *const u8,
    request_len: usize,
    out: *mut GvyaBuffer,
) -> i32 {
    if request_ptr.is_null() || out.is_null() {
        return INVALID_ARGUMENT;
    }
    reset_output(out);
    let request = unsafe { slice::from_raw_parts(request_ptr, request_len) };
    let request = match parse_open_request(request) {
        Ok(value) => value,
        Err(error) => return write_error(out, WIRE_FAILED, &format!("{error:?}")),
    };
    with_runtime(handle, out, |runtime| match runtime.open(request) {
        Ok(output) => serialize_turn_result_with_limits(&output, runtime.limits()),
        Err(error) => Err(gvya_runtime::wire::WireError::Invalid(format!(
            "runtime request rejected: {error:?}"
        ))),
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn gvya_runtime_capability_result_json(
    handle: u64,
    request_ptr: *const u8,
    request_len: usize,
    out: *mut GvyaBuffer,
) -> i32 {
    if request_ptr.is_null() || out.is_null() {
        return INVALID_ARGUMENT;
    }
    reset_output(out);
    let request = unsafe { slice::from_raw_parts(request_ptr, request_len) };
    let request = match parse_capability_result_request(request) {
        Ok(value) => value,
        Err(error) => return write_error(out, WIRE_FAILED, &format!("{error:?}")),
    };
    with_runtime(handle, out, |runtime| {
        match runtime.capability_result(request) {
            Ok(result) => serialize_capability_result_result_with_limits(&result, runtime.limits()),
            Err(error) => Err(gvya_runtime::wire::WireError::Invalid(format!(
                "runtime request rejected: {error:?}"
            ))),
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn gvya_runtime_asset_by_path(
    handle: u64,
    path_ptr: *const u8,
    path_len: usize,
    out: *mut GvyaBuffer,
) -> i32 {
    if path_ptr.is_null() || out.is_null() {
        return INVALID_ARGUMENT;
    }
    reset_output(out);
    if path_len > FFI_TEXT_INPUT_MAX_BYTES {
        return write_error(out, INVALID_ARGUMENT, "asset path exceeds byte limit");
    }
    let raw = unsafe { slice::from_raw_parts(path_ptr, path_len) };
    let Ok(path) = std::str::from_utf8(raw) else {
        return write_error(out, INVALID_ARGUMENT, "asset path is not UTF-8");
    };
    let runtime = {
        let Ok(guard) = registry().lock() else {
            return write_error(out, INTERNAL_ERROR, "runtime registry poisoned");
        };
        let Some(runtime) = guard.runtimes.get(&handle) else {
            return write_error(out, HANDLE_NOT_FOUND, "runtime handle not found");
        };
        Arc::clone(runtime)
    };
    let Some(asset) = runtime.asset_by_logical_path(path) else {
        return write_error(out, NOT_FOUND, "asset not found");
    };
    write_buffer(out, asset.bytes.to_vec())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn gvya_runtime_asset_by_id(
    handle: u64,
    id_ptr: *const u8,
    id_len: usize,
    out: *mut GvyaBuffer,
) -> i32 {
    if id_ptr.is_null() || out.is_null() {
        return INVALID_ARGUMENT;
    }
    reset_output(out);
    if id_len > FFI_TEXT_INPUT_MAX_BYTES {
        return write_error(out, INVALID_ARGUMENT, "asset id exceeds byte limit");
    }
    let raw = unsafe { slice::from_raw_parts(id_ptr, id_len) };
    let Ok(id) = std::str::from_utf8(raw) else {
        return write_error(out, INVALID_ARGUMENT, "asset id is not UTF-8");
    };
    let runtime = {
        let Ok(guard) = registry().lock() else {
            return write_error(out, INTERNAL_ERROR, "runtime registry poisoned");
        };
        let Some(runtime) = guard.runtimes.get(&handle) else {
            return write_error(out, HANDLE_NOT_FOUND, "runtime handle not found");
        };
        Arc::clone(runtime)
    };
    let Some(asset) = runtime.asset(&AssetId::new(id)) else {
        return write_error(out, NOT_FOUND, "asset not found");
    };
    write_buffer(out, asset.bytes.to_vec())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn gvya_runtime_asset_info_by_id_json(
    handle: u64,
    id_ptr: *const u8,
    id_len: usize,
    out: *mut GvyaBuffer,
) -> i32 {
    if id_ptr.is_null() || out.is_null() {
        return INVALID_ARGUMENT;
    }
    reset_output(out);
    if id_len > FFI_TEXT_INPUT_MAX_BYTES {
        return write_error(out, INVALID_ARGUMENT, "asset id exceeds byte limit");
    }
    let raw = unsafe { slice::from_raw_parts(id_ptr, id_len) };
    let Ok(id) = std::str::from_utf8(raw) else {
        return write_error(out, INVALID_ARGUMENT, "asset id is not UTF-8");
    };
    let runtime = {
        let Ok(guard) = registry().lock() else {
            return write_error(out, INTERNAL_ERROR, "runtime registry poisoned");
        };
        let Some(runtime) = guard.runtimes.get(&handle) else {
            return write_error(out, HANDLE_NOT_FOUND, "runtime handle not found");
        };
        Arc::clone(runtime)
    };
    let Some(asset) = runtime.asset(&AssetId::new(id)) else {
        return write_error(out, NOT_FOUND, "asset not found");
    };
    match serialize_asset_info(&asset) {
        Ok(bytes) => write_buffer(out, bytes),
        Err(error) => write_error(out, WIRE_FAILED, &format!("{error:?}")),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn gvya_buffer_free(buffer: *mut GvyaBuffer) {
    if buffer.is_null() {
        return;
    }
    let value = unsafe { std::ptr::read_unaligned(buffer) };
    if !value.ptr.is_null() {
        if let Ok(mut registry) = owned_buffers().lock() {
            // The registry owns the Vec. Pointer/len/capacity supplied by the caller are never
            // used to reconstruct allocation metadata, so fabricated triples and double frees
            // cannot reach Vec::from_raw_parts.
            registry.remove(&(value.ptr as usize));
        }
    }
    unsafe {
        std::ptr::write_unaligned(buffer, GvyaBuffer::default());
    }
}

fn with_runtime(
    handle: u64,
    out: *mut GvyaBuffer,
    call: impl FnOnce(&Runtime) -> Result<Vec<u8>, gvya_runtime::wire::WireError>,
) -> i32 {
    let runtime = {
        let Ok(guard) = registry().lock() else {
            return write_error(out, INTERNAL_ERROR, "runtime registry poisoned");
        };
        let Some(runtime) = guard.runtimes.get(&handle) else {
            return write_error(out, HANDLE_NOT_FOUND, "runtime handle not found");
        };
        Arc::clone(runtime)
    };
    match call(runtime.as_ref()) {
        Ok(bytes) => write_buffer(out, bytes),
        Err(error) => write_error(out, WIRE_FAILED, &format!("{error:?}")),
    }
}

fn reset_output(out: *mut GvyaBuffer) {
    if !out.is_null() {
        unsafe {
            std::ptr::write_unaligned(out, GvyaBuffer::default());
        }
    }
}

fn write_buffer(out: *mut GvyaBuffer, mut bytes: Vec<u8>) -> i32 {
    if out.is_null() {
        return INVALID_ARGUMENT;
    }
    let value = GvyaBuffer {
        ptr: bytes.as_mut_ptr(),
        len: bytes.len(),
        capacity: bytes.capacity(),
    };
    if !value.ptr.is_null() && value.capacity > 0 {
        let Ok(mut registry) = owned_buffers().lock() else {
            return INTERNAL_ERROR;
        };
        if registry.insert(value.ptr as usize, bytes).is_some() {
            return INTERNAL_ERROR;
        }
    }
    unsafe {
        std::ptr::write_unaligned(out, value);
    }
    OK
}

fn write_error(out: *mut GvyaBuffer, code: i32, message: &str) -> i32 {
    if !out.is_null() {
        let bytes = serde_json::to_vec(&serde_json::json!({ "error": message, "code": code }))
            .unwrap_or_else(|_| {
                b"{\"error\":\"internal error serialization failed\",\"code\":5}".to_vec()
            });
        let _ = write_buffer(out, bytes);
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_abi_opens_canonical_runtime_and_exposes_info_and_asset() {
        let artifact = include_bytes!("../../../validation/fixtures/runtime-action.gvya");
        let mut handle = 0_u64;
        let mut message = GvyaBuffer::default();
        let status = unsafe {
            let options = br#"{"format":"gvya.runtime.open-options","version":1}"#;
            gvya_runtime_open_with_options_json(
                artifact.as_ptr(),
                artifact.len(),
                options.as_ptr(),
                options.len(),
                &raw mut handle,
                &raw mut message,
            )
        };
        assert_eq!(status, OK);
        assert_ne!(handle, 0);
        assert!(message.ptr.is_null());

        let mut info = GvyaBuffer::default();
        assert_eq!(unsafe { gvya_runtime_info_json(handle, &raw mut info) }, OK);
        let info_bytes = unsafe { slice::from_raw_parts(info.ptr, info.len) };
        let info_text = std::str::from_utf8(info_bytes).unwrap();
        assert!(info_text.contains("\"format\":\"gvya.runtime.info\""));
        assert!(info_text.contains("\"project_id\":\"runtime-action\""));
        unsafe {
            gvya_buffer_free(&raw mut info);
        }

        let asset_id = b"tone";
        let mut asset = GvyaBuffer::default();
        assert_eq!(
            unsafe {
                gvya_runtime_asset_by_id(handle, asset_id.as_ptr(), asset_id.len(), &raw mut asset)
            },
            OK
        );
        let asset_bytes = unsafe { slice::from_raw_parts(asset.ptr, asset.len) };
        assert_eq!(asset_bytes, b"GVYA runtime fixture asset\n");
        unsafe {
            gvya_buffer_free(&raw mut asset);
        }

        assert_eq!(gvya_runtime_close(handle), OK);
    }

    #[test]
    fn open_policy_binds_preverified_attestation_to_the_exact_signed_artifact() {
        let artifact = include_bytes!("../../../validation/fixtures/runtime-signed.gvya");
        let fixture = Runtime::load(artifact.to_vec(), LoadPolicy::default(), None).unwrap();
        let content_root = fixture
            .content_root()
            .iter()
            .map(|value| format!("{value:02x}"))
            .collect::<String>();
        let options = format!(
            r#"{{"format":"gvya.runtime.open-options","version":1,"signature":{{"mode":"require_verified","preverified":{{"content_root":"{content_root}","algorithm":"fixture-v1","key_id":"fixture-key","signature":"fixture-signature"}}}}}}"#
        );
        let options = options.as_bytes();
        let mut handle = 0_u64;
        let mut message = GvyaBuffer::default();
        let code = unsafe {
            gvya_runtime_open_with_options_json(
                artifact.as_ptr(),
                artifact.len(),
                options.as_ptr(),
                options.len(),
                &raw mut handle,
                &raw mut message,
            )
        };
        assert_eq!(code, OK);
        assert_ne!(handle, 0);
        assert!(message.ptr.is_null());

        let mut info = GvyaBuffer::default();
        assert_eq!(unsafe { gvya_runtime_info_json(handle, &raw mut info) }, OK);
        let info_bytes = unsafe { slice::from_raw_parts(info.ptr, info.len) };
        let info_text = std::str::from_utf8(info_bytes).unwrap();
        assert!(info_text.contains("\"status\":\"verified\""));
        unsafe {
            gvya_buffer_free(&raw mut info);
        }
        assert_eq!(gvya_runtime_close(handle), OK);

        let wrong = br#"{"format":"gvya.runtime.open-options","version":1,"signature":{"mode":"require_verified","preverified":{"content_root":"0000000000000000000000000000000000000000000000000000000000000000","algorithm":"fixture-v1","key_id":"fixture-key","signature":"fixture-signature"}}}"#;
        let code = unsafe {
            gvya_runtime_open_with_options_json(
                artifact.as_ptr(),
                artifact.len(),
                wrong.as_ptr(),
                wrong.len(),
                &raw mut handle,
                &raw mut message,
            )
        };
        assert_eq!(code, OPEN_FAILED);
        unsafe {
            gvya_buffer_free(&raw mut message);
        }
    }

    #[test]
    fn open_policy_rejects_unsigned_and_preflights_artifact_length() {
        let artifact = include_bytes!("../../../validation/fixtures/runtime-action.gvya");
        let options = br#"{"format":"gvya.runtime.open-options","version":1,"signature":{"mode":"require_present"}}"#;
        let mut handle = 0_u64;
        let mut message = GvyaBuffer::default();
        let code = unsafe {
            gvya_runtime_open_with_options_json(
                artifact.as_ptr(),
                artifact.len(),
                options.as_ptr(),
                options.len(),
                &raw mut handle,
                &raw mut message,
            )
        };
        assert_eq!(code, OPEN_FAILED);
        unsafe {
            gvya_buffer_free(&raw mut message);
        }

        let tiny = br#"{"format":"gvya.runtime.open-options","version":1,"artifact_limits":{"max_total_bytes":1}}"#;
        let code = unsafe {
            gvya_runtime_open_with_options_json(
                artifact.as_ptr(),
                artifact.len(),
                tiny.as_ptr(),
                tiny.len(),
                &raw mut handle,
                &raw mut message,
            )
        };
        assert_eq!(code, OPEN_FAILED);
        unsafe {
            gvya_buffer_free(&raw mut message);
        }

        let relaxed = br#"{"format":"gvya.runtime.open-options","version":1,"artifact_limits":{"max_total_bytes":536870913}}"#;
        let code = unsafe {
            gvya_runtime_open_with_options_json(
                artifact.as_ptr(),
                artifact.len(),
                relaxed.as_ptr(),
                relaxed.len(),
                &raw mut handle,
                &raw mut message,
            )
        };
        assert_eq!(code, INVALID_ARGUMENT);
        unsafe {
            gvya_buffer_free(&raw mut message);
        }
    }
}
