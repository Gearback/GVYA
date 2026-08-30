//! Compiler operations exposed through the single GVYA Engine ABI.
//!
//! This module owns source-archive transport only. Source validation, package composition, audit,
//! IR and artifact semantics remain in `gvya-compiler`; memory ownership remains in the parent
//! `gvya-ffi` Engine edge.

use std::{collections::BTreeMap, slice};

use super::{GvyaBuffer, INTERNAL_ERROR, INVALID_ARGUMENT, OK, reset_output, write_buffer};

use gvya_compiler::{
    audit::{AuditLocation, AuditReport, AuditSeverity},
    pipeline::{BuildError, BuildOptions, build_source_project},
    source::{SourceIssue, SourceLimits, SourceTree, resolve_source_project},
};
use serde_json::{Value, json};

const SOURCE_ARCHIVE_MAGIC: &[u8; 8] = b"GVYASRC1";
const SOURCE_ARCHIVE_FAILED: i32 = 20;
const BUILD_FAILED: i32 = 21;
const MAX_PATH_BYTES: usize = 4 * 1024;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn gvya_compiler_validate_source_tree(
    archive_ptr: *const u8,
    archive_len: usize,
    out: *mut GvyaBuffer,
) -> i32 {
    if out.is_null() || (archive_len > 0 && archive_ptr.is_null()) {
        return INVALID_ARGUMENT;
    }
    reset_output(out);
    let limits = SourceLimits::default();
    let archive = if archive_len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(archive_ptr, archive_len) }
    };
    let files = match decode_source_archive(archive, limits) {
        Ok(value) => value,
        Err(message) => {
            return write_error(
                out,
                SOURCE_ARCHIVE_FAILED,
                "source_archive_invalid",
                &message,
                None,
            );
        }
    };
    let tree = match SourceTree::new(files, limits) {
        Ok(value) => value,
        Err(issues) => {
            return write_error(
                out,
                BUILD_FAILED,
                "source_invalid",
                "Source validation failed.",
                Some(source_issues_json(&issues)),
            );
        }
    };
    match resolve_source_project(&tree, limits) {
        Ok(_) => write_buffer(out, Vec::new()),
        Err(issues) => write_error(
            out,
            BUILD_FAILED,
            "source_invalid",
            "Source validation failed.",
            Some(source_issues_json(&issues)),
        ),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn gvya_compiler_build_source_tree(
    archive_ptr: *const u8,
    archive_len: usize,
    out: *mut GvyaBuffer,
) -> i32 {
    if out.is_null() || (archive_len > 0 && archive_ptr.is_null()) {
        return INVALID_ARGUMENT;
    }
    reset_output(out);
    let limits = SourceLimits::default();
    if archive_len
        > limits
            .max_total_bytes
            .saturating_add(limits.max_files.saturating_mul(MAX_PATH_BYTES + 8))
            .saturating_add(12)
    {
        return write_error(
            out,
            SOURCE_ARCHIVE_FAILED,
            "source_archive_too_large",
            "Source archive exceeds the compiler transport limit.",
            None,
        );
    }
    let archive = if archive_len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(archive_ptr, archive_len) }
    };
    let files = match decode_source_archive(archive, limits) {
        Ok(value) => value,
        Err(message) => {
            return write_error(
                out,
                SOURCE_ARCHIVE_FAILED,
                "source_archive_invalid",
                &message,
                None,
            );
        }
    };
    let tree = match SourceTree::new(files, limits) {
        Ok(value) => value,
        Err(issues) => {
            return write_error(
                out,
                BUILD_FAILED,
                "source_invalid",
                "Source validation failed.",
                Some(source_issues_json(&issues)),
            );
        }
    };
    match build_source_project(&tree, BuildOptions::default(), None) {
        Ok(result) => write_buffer(out, result.artifact),
        Err(error) => write_build_error(out, &error),
    }
}

fn decode_source_archive(
    bytes: &[u8],
    limits: SourceLimits,
) -> Result<BTreeMap<String, Vec<u8>>, String> {
    if bytes.len() < 12 || &bytes[..8] != SOURCE_ARCHIVE_MAGIC {
        return Err("Source archive magic/version is invalid.".into());
    }
    let mut offset = 8_usize;
    let count = read_u32(bytes, &mut offset)? as usize;
    if count == 0 || count > limits.max_files {
        return Err("Source archive file count is outside the supported range.".into());
    }
    let mut files = BTreeMap::new();
    let mut total = 0_usize;
    for _ in 0..count {
        let path_len = read_u32(bytes, &mut offset)? as usize;
        let file_len = read_u32(bytes, &mut offset)? as usize;
        if path_len == 0 || path_len > MAX_PATH_BYTES {
            return Err("Source archive contains an invalid path length.".into());
        }
        if file_len > limits.max_asset_bytes {
            return Err("Source archive contains an oversized file.".into());
        }
        let path_bytes = take(bytes, &mut offset, path_len)?;
        let path = std::str::from_utf8(path_bytes)
            .map_err(|_| "Source archive path is not UTF-8.")?
            .to_owned();
        let file = take(bytes, &mut offset, file_len)?.to_vec();
        total = total
            .checked_add(file_len)
            .ok_or("Source archive size overflow.")?;
        if total > limits.max_total_bytes {
            return Err("Source archive exceeds total source byte limit.".into());
        }
        if files.insert(path, file).is_some() {
            return Err("Source archive contains a duplicate path.".into());
        }
    }
    if offset != bytes.len() {
        return Err("Source archive contains trailing bytes.".into());
    }
    Ok(files)
}

fn read_u32(bytes: &[u8], offset: &mut usize) -> Result<u32, String> {
    let raw = take(bytes, offset, 4)?;
    Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

fn take<'a>(bytes: &'a [u8], offset: &mut usize, len: usize) -> Result<&'a [u8], String> {
    let end = offset
        .checked_add(len)
        .ok_or("Source archive offset overflow.")?;
    let value = bytes
        .get(*offset..end)
        .ok_or("Source archive is truncated.")?;
    *offset = end;
    Ok(value)
}

fn write_build_error(out: *mut GvyaBuffer, error: &BuildError) -> i32 {
    let (kind, message, details) = match error {
        BuildError::Source(issues) => (
            "source_invalid",
            "Source validation failed.".to_owned(),
            Some(source_issues_json(issues)),
        ),
        BuildError::Audit(report) => (
            "audit_failed",
            "Compiler audit failed.".to_owned(),
            Some(audit_issues_json(report)),
        ),
        BuildError::Composition(issues) => (
            "composition_failed",
            "Package composition failed.".to_owned(),
            Some(json!({"debug": format!("{issues:?}")})),
        ),
        BuildError::Ir(error) => (
            "ir_failed",
            "Canonical IR compilation failed.".to_owned(),
            Some(json!({"debug": format!("{error:?}")})),
        ),
        other => (
            "build_failed",
            "Canonical artifact build failed.".to_owned(),
            Some(json!({"debug": format!("{other:?}")})),
        ),
    };
    write_error(out, BUILD_FAILED, kind, &message, details)
}

fn source_issues_json(issues: &[SourceIssue]) -> Value {
    Value::Array(
        issues
            .iter()
            .map(|issue| json!({"code": issue.code, "path": issue.path, "message": issue.message}))
            .collect(),
    )
}

/// Serializes blocking audit findings in the same shape as source issues so the browser can show
/// the exact rule, location and remediation instead of an opaque failure.
fn audit_issues_json(report: &AuditReport) -> Value {
    Value::Array(
        report
            .issues
            .iter()
            .filter(|issue| issue.severity == AuditSeverity::Error)
            .map(|issue| {
                json!({
                    "code": issue.code.as_str(),
                    "path": audit_location_path(&issue.location),
                    "message": issue.summary,
                    "remediation": issue.remediation,
                })
            })
            .collect(),
    )
}

/// Renders an audit location as one human-readable source path.
///
/// Components are separated by `/` because package ids, contribution kinds and object ids each
/// already contain `.`; joining them with `.` produced ambiguous paths such as
/// `pkg.bot.meaning.meaning.new` where the kind and the object id could not be told apart.
fn audit_location_path(location: &AuditLocation) -> String {
    if let Some(path) = &location.path {
        return path.clone();
    }
    let mut parts: Vec<String> = Vec::new();
    if let Some(package) = &location.package {
        parts.push(package.as_str().to_owned());
    }
    if let Some(kind) = location.kind {
        parts.push(kind.label().to_owned());
    }
    if let Some(item) = &location.item_id {
        parts.push(item.clone());
    }
    if let Some(sub) = &location.sub_id {
        parts.push(sub.clone());
    }
    parts.join("/")
}

fn write_error(
    out: *mut GvyaBuffer,
    code: i32,
    kind: &str,
    message: &str,
    details: Option<Value>,
) -> i32 {
    let payload = json!({
        "format": "gvya.compiler.error",
        "version": 1,
        "kind": kind,
        "message": message,
        "details": details,
    });
    match serde_json::to_vec(&payload) {
        Ok(bytes) => {
            if write_buffer(out, bytes) == OK {
                code
            } else {
                INTERNAL_ERROR
            }
        }
        Err(_) => INTERNAL_ERROR,
    }
}
