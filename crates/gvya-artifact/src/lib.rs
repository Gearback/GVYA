//! Canonical GVYA artifact container shared by compiler and runtime.

#![forbid(unsafe_code)]

//! Deterministic `.gvya` container v1.
//!
//! This is deliberately not ZIP. There is one artifact shape, no timestamps, no compression
//! metadata, no ambiguous source/runtime form, and no archive extraction step. Entries are sorted
//! by logical path and each entry is independently SHA-256 authenticated by the container table.

use std::collections::BTreeSet;

use sha2::{Digest, Sha256};

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for value in bytes {
        encoded.push(char::from(DIGITS[usize::from(*value >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(*value & 0x0f)]));
    }
    encoded
}

pub const MAGIC: [u8; 8] = *b"GVYA\r\n\x1a\n";
pub const CONTAINER_VERSION: u16 = 1;
const HEADER_LEN: usize = 24;
const FIXED_ENTRY_LEN: usize = 52;

pub const ARTIFACT_MAX_ENTRIES: usize = 16_384;
pub const ARTIFACT_MAX_PATH_BYTES: usize = 512;
pub const ARTIFACT_MAX_ENTRY_BYTES: usize = 64 * 1024 * 1024;
pub const ARTIFACT_MAX_TOTAL_BYTES: usize = 512 * 1024 * 1024;
pub const ARTIFACT_MAX_MANIFEST_BYTES: usize = 8 * 1024 * 1024;
pub const ARTIFACT_MAX_INTEGRITY_BYTES: usize = 8 * 1024 * 1024;
pub const ARTIFACT_MAX_SIGNATURE_BYTES: usize = 256 * 1024;
pub const ARTIFACT_MAX_DEBUG_MAP_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum EntryKind {
    Manifest = 1,
    Program = 2,
    Asset = 3,
    DebugMap = 4,
    Signature = 5,
    Integrity = 6,
}

impl EntryKind {
    fn from_byte(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Manifest),
            2 => Some(Self::Program),
            3 => Some(Self::Asset),
            4 => Some(Self::DebugMap),
            5 => Some(Self::Signature),
            6 => Some(Self::Integrity),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactEntry {
    pub kind: EntryKind,
    pub path: String,
    pub essential: bool,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArtifactLimits {
    pub max_entries: usize,
    pub max_path_bytes: usize,
    pub max_entry_bytes: usize,
    pub max_total_bytes: usize,
    pub max_manifest_bytes: usize,
    pub max_integrity_bytes: usize,
    pub max_signature_bytes: usize,
    pub max_debug_map_bytes: usize,
}

impl Default for ArtifactLimits {
    fn default() -> Self {
        Self {
            max_entries: ARTIFACT_MAX_ENTRIES,
            max_path_bytes: ARTIFACT_MAX_PATH_BYTES,
            max_entry_bytes: ARTIFACT_MAX_ENTRY_BYTES,
            max_total_bytes: ARTIFACT_MAX_TOTAL_BYTES,
            max_manifest_bytes: ARTIFACT_MAX_MANIFEST_BYTES,
            max_integrity_bytes: ARTIFACT_MAX_INTEGRITY_BYTES,
            max_signature_bytes: ARTIFACT_MAX_SIGNATURE_BYTES,
            max_debug_map_bytes: ARTIFACT_MAX_DEBUG_MAP_BYTES,
        }
    }
}

impl ArtifactLimits {
    /// Artifact limits are caller-tightenable only. No compiler/runtime/FFI caller may raise the
    /// canonical container or metadata ceilings.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError::InvalidLimits`] when any limit is zero or exceeds its canonical
    /// ceiling.
    pub fn validate(self) -> Result<(), ArtifactError> {
        let canonical = Self::default();
        let positive = self.max_entries > 0
            && self.max_path_bytes > 0
            && self.max_entry_bytes > 0
            && self.max_total_bytes > 0
            && self.max_manifest_bytes > 0
            && self.max_integrity_bytes > 0
            && self.max_signature_bytes > 0
            && self.max_debug_map_bytes > 0;
        if !positive {
            return Err(ArtifactError::InvalidLimits(
                "artifact limits must be positive",
            ));
        }
        let bounded = self.max_entries <= canonical.max_entries
            && self.max_path_bytes <= canonical.max_path_bytes
            && self.max_entry_bytes <= canonical.max_entry_bytes
            && self.max_total_bytes <= canonical.max_total_bytes
            && self.max_manifest_bytes <= canonical.max_manifest_bytes
            && self.max_integrity_bytes <= canonical.max_integrity_bytes
            && self.max_signature_bytes <= canonical.max_signature_bytes
            && self.max_debug_map_bytes <= canonical.max_debug_map_bytes;
        if !bounded {
            return Err(ArtifactError::InvalidLimits(
                "artifact limits may tighten but not relax canonical ceilings",
            ));
        }
        Ok(())
    }

    fn max_bytes_for_kind(self, kind: EntryKind) -> usize {
        let specialized = match kind {
            EntryKind::Manifest => self.max_manifest_bytes,
            EntryKind::Integrity => self.max_integrity_bytes,
            EntryKind::Signature => self.max_signature_bytes,
            EntryKind::DebugMap => self.max_debug_map_bytes,
            EntryKind::Program | EntryKind::Asset => self.max_entry_bytes,
        };
        specialized.min(self.max_entry_bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArtifactError {
    InvalidLimits(&'static str),
    InvalidMagic,
    UnsupportedVersion(u16),
    ReservedHeaderBits,
    TooManyEntries,
    InvalidPath(String),
    DuplicatePath(String),
    EntryTooLarge(String),
    ArtifactTooLarge,
    OffsetOverflow,
    Truncated,
    InvalidEntryKind(u8),
    InvalidUtf8Path,
    TableNotCanonical,
    EntryOutOfBounds(String),
    DigestMismatch(String),
    MissingRequiredEntry(&'static str),
    InvalidRequiredEntryKind(String),
    InvalidEntryConvention(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactEntryInfo {
    pub kind: EntryKind,
    pub path: String,
    pub essential: bool,
    pub offset: u64,
    pub length: u64,
    pub digest: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct ParsedArtifact<'a> {
    bytes: &'a [u8],
    entries: Vec<ArtifactEntryInfo>,
}

impl<'a> ParsedArtifact<'a> {
    #[must_use]
    pub fn entries(&self) -> &[ArtifactEntryInfo] {
        &self.entries
    }

    #[must_use]
    pub fn entry(&self, path: &str) -> Option<&'a [u8]> {
        let entry = self.entries.iter().find(|entry| entry.path == path)?;
        let start = usize::try_from(entry.offset).ok()?;
        let length = usize::try_from(entry.length).ok()?;
        self.bytes.get(start..start.checked_add(length)?)
    }

    #[must_use]
    pub fn content_root(&self) -> [u8; 32] {
        content_root(&self.entries)
    }
}

/// Builds a canonical artifact from the supplied entries.
///
/// # Errors
///
/// Returns an [`ArtifactError`] when limits are invalid, required entries or conventions are
/// missing, a path or digest-table size is invalid, or the resulting artifact exceeds a bound.
pub fn build_artifact(
    mut entries: Vec<ArtifactEntry>,
    limits: ArtifactLimits,
) -> Result<Vec<u8>, ArtifactError> {
    limits.validate()?;
    if entries.len() > limits.max_entries {
        return Err(ArtifactError::TooManyEntries);
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    let mut seen = BTreeSet::new();
    let mut table_len = 0_usize;
    let mut total_payload = 0_usize;
    for entry in &entries {
        validate_path(&entry.path, limits)?;
        validate_entry_convention(entry.kind, &entry.path, entry.essential)?;
        if !seen.insert(entry.path.clone()) {
            return Err(ArtifactError::DuplicatePath(entry.path.clone()));
        }
        if entry.bytes.len() > limits.max_bytes_for_kind(entry.kind) {
            return Err(ArtifactError::EntryTooLarge(entry.path.clone()));
        }
        table_len = table_len
            .checked_add(FIXED_ENTRY_LEN + entry.path.len())
            .ok_or(ArtifactError::OffsetOverflow)?;
        total_payload = total_payload
            .checked_add(entry.bytes.len())
            .ok_or(ArtifactError::OffsetOverflow)?;
    }
    require_core_entries(&entries)?;
    let total_len = HEADER_LEN
        .checked_add(table_len)
        .and_then(|value| value.checked_add(total_payload))
        .ok_or(ArtifactError::OffsetOverflow)?;
    if total_len > limits.max_total_bytes {
        return Err(ArtifactError::ArtifactTooLarge);
    }

    let payload_start = HEADER_LEN
        .checked_add(table_len)
        .ok_or(ArtifactError::OffsetOverflow)?;
    let mut infos = Vec::with_capacity(entries.len());
    let mut offset = payload_start;
    for entry in &entries {
        infos.push(ArtifactEntryInfo {
            kind: entry.kind,
            path: entry.path.clone(),
            essential: entry.essential,
            offset: u64::try_from(offset).map_err(|_| ArtifactError::OffsetOverflow)?,
            length: u64::try_from(entry.bytes.len()).map_err(|_| ArtifactError::OffsetOverflow)?,
            digest: sha256(&entry.bytes),
        });
        offset = offset
            .checked_add(entry.bytes.len())
            .ok_or(ArtifactError::OffsetOverflow)?;
    }

    let mut out = Vec::with_capacity(total_len);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&CONTAINER_VERSION.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes()); // container flags reserved
    out.extend_from_slice(
        &(u32::try_from(entries.len()).map_err(|_| ArtifactError::TooManyEntries)?).to_le_bytes(),
    );
    out.extend_from_slice(
        &(u64::try_from(table_len).map_err(|_| ArtifactError::OffsetOverflow)?).to_le_bytes(),
    );
    for info in &infos {
        out.push(info.kind as u8);
        out.push(u8::from(info.essential));
        out.extend_from_slice(
            &(u16::try_from(info.path.len())
                .map_err(|_| ArtifactError::InvalidPath(info.path.clone()))?)
            .to_le_bytes(),
        );
        out.extend_from_slice(&info.offset.to_le_bytes());
        out.extend_from_slice(&info.length.to_le_bytes());
        out.extend_from_slice(&info.digest);
        out.extend_from_slice(info.path.as_bytes());
    }
    for entry in entries {
        out.extend_from_slice(&entry.bytes);
    }
    debug_assert_eq!(out.len(), total_len);
    Ok(out)
}

fn parse_header(bytes: &[u8], limits: ArtifactLimits) -> Result<(usize, usize), ArtifactError> {
    limits.validate()?;
    if bytes.len() > limits.max_total_bytes {
        return Err(ArtifactError::ArtifactTooLarge);
    }
    if bytes.len() < HEADER_LEN {
        return Err(ArtifactError::Truncated);
    }
    if bytes[..8] != MAGIC {
        return Err(ArtifactError::InvalidMagic);
    }
    let version = u16_at(bytes, 8)?;
    if version != CONTAINER_VERSION {
        return Err(ArtifactError::UnsupportedVersion(version));
    }
    if u16_at(bytes, 10)? != 0 {
        return Err(ArtifactError::ReservedHeaderBits);
    }
    let count = usize::try_from(u32_at(bytes, 12)?).map_err(|_| ArtifactError::TooManyEntries)?;
    if count > limits.max_entries {
        return Err(ArtifactError::TooManyEntries);
    }
    let table_len =
        usize::try_from(u64_at(bytes, 16)?).map_err(|_| ArtifactError::OffsetOverflow)?;
    let table_end = HEADER_LEN
        .checked_add(table_len)
        .ok_or(ArtifactError::OffsetOverflow)?;
    if table_end > bytes.len() {
        return Err(ArtifactError::Truncated);
    }
    Ok((count, table_end))
}

/// Parses and authenticates a canonical artifact without copying its payloads.
///
/// # Errors
///
/// Returns an [`ArtifactError`] when limits are invalid or the input is truncated, oversized,
/// non-canonical, structurally invalid, or fails entry digest authentication.
pub fn parse_artifact(
    bytes: &[u8],
    limits: ArtifactLimits,
) -> Result<ParsedArtifact<'_>, ArtifactError> {
    let (count, table_end) = parse_header(bytes, limits)?;

    let mut cursor = HEADER_LEN;
    let mut entries = Vec::with_capacity(count);
    let mut seen = BTreeSet::new();
    let mut previous_path: Option<String> = None;
    let mut expected_payload_offset = table_end;
    for _ in 0..count {
        let fixed_end = cursor
            .checked_add(FIXED_ENTRY_LEN)
            .ok_or(ArtifactError::OffsetOverflow)?;
        if fixed_end > table_end {
            return Err(ArtifactError::Truncated);
        }
        let kind_byte = bytes[cursor];
        let kind =
            EntryKind::from_byte(kind_byte).ok_or(ArtifactError::InvalidEntryKind(kind_byte))?;
        let flag = bytes[cursor + 1];
        if flag > 1 {
            return Err(ArtifactError::ReservedHeaderBits);
        }
        let path_len = usize::from(u16_at(bytes, cursor + 2)?);
        let offset = u64_at(bytes, cursor + 4)?;
        let length = u64_at(bytes, cursor + 12)?;
        let mut digest = [0_u8; 32];
        digest.copy_from_slice(&bytes[cursor + 20..cursor + 52]);
        let path_start = fixed_end;
        let path_end = path_start
            .checked_add(path_len)
            .ok_or(ArtifactError::OffsetOverflow)?;
        if path_end > table_end {
            return Err(ArtifactError::Truncated);
        }
        let path = std::str::from_utf8(&bytes[path_start..path_end])
            .map_err(|_| ArtifactError::InvalidUtf8Path)?
            .to_owned();
        validate_path(&path, limits)?;
        validate_entry_convention(kind, &path, flag == 1)?;
        if !seen.insert(path.clone()) {
            return Err(ArtifactError::DuplicatePath(path));
        }
        if previous_path
            .as_ref()
            .is_some_and(|previous| previous >= &path)
        {
            return Err(ArtifactError::TableNotCanonical);
        }
        previous_path = Some(path.clone());
        let start = usize::try_from(offset).map_err(|_| ArtifactError::OffsetOverflow)?;
        let len = usize::try_from(length).map_err(|_| ArtifactError::OffsetOverflow)?;
        if len > limits.max_bytes_for_kind(kind) {
            return Err(ArtifactError::EntryTooLarge(path));
        }
        let end = start
            .checked_add(len)
            .ok_or(ArtifactError::OffsetOverflow)?;
        if start < table_end || end > bytes.len() {
            return Err(ArtifactError::EntryOutOfBounds(path));
        }
        if start != expected_payload_offset {
            return Err(ArtifactError::TableNotCanonical);
        }
        expected_payload_offset = end;
        if sha256(&bytes[start..end]) != digest {
            return Err(ArtifactError::DigestMismatch(path));
        }
        entries.push(ArtifactEntryInfo {
            kind,
            path,
            essential: flag == 1,
            offset,
            length,
            digest,
        });
        cursor = path_end;
    }
    if cursor != table_end || expected_payload_offset != bytes.len() {
        return Err(ArtifactError::TableNotCanonical);
    }
    require_parsed_core_entries(&entries)?;
    Ok(ParsedArtifact { bytes, entries })
}

fn validate_entry_convention(
    kind: EntryKind,
    path: &str,
    essential: bool,
) -> Result<(), ArtifactError> {
    let valid = match kind {
        EntryKind::Manifest => path == "manifest.json" && essential,
        EntryKind::Program => path == "program.json" && essential,
        EntryKind::Integrity => path == "integrity.json" && essential,
        EntryKind::Asset => path.starts_with("assets/") && essential,
        EntryKind::DebugMap => path == "debug/source-map.json" && !essential,
        EntryKind::Signature => path == "signature.json" && !essential,
    };
    if valid {
        Ok(())
    } else {
        Err(ArtifactError::InvalidEntryConvention(path.to_owned()))
    }
}

fn validate_path(path: &str, limits: ArtifactLimits) -> Result<(), ArtifactError> {
    let invalid = path.is_empty()
        || path.len() > limits.max_path_bytes
        || path.starts_with('/')
        || path.ends_with('/')
        || path.contains('\\')
        || path.contains('\0')
        || path
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        || path.bytes().any(|byte| byte < 0x20 || byte == 0x7f);
    if invalid {
        Err(ArtifactError::InvalidPath(path.to_owned()))
    } else {
        Ok(())
    }
}

fn require_core_entries(entries: &[ArtifactEntry]) -> Result<(), ArtifactError> {
    for (path, kind) in [
        ("manifest.json", EntryKind::Manifest),
        ("program.json", EntryKind::Program),
        ("integrity.json", EntryKind::Integrity),
    ] {
        let Some(entry) = entries.iter().find(|entry| entry.path == path) else {
            return Err(ArtifactError::MissingRequiredEntry(path));
        };
        if entry.kind != kind || !entry.essential {
            return Err(ArtifactError::InvalidRequiredEntryKind(path.to_owned()));
        }
    }
    Ok(())
}

fn require_parsed_core_entries(entries: &[ArtifactEntryInfo]) -> Result<(), ArtifactError> {
    for (path, kind) in [
        ("manifest.json", EntryKind::Manifest),
        ("program.json", EntryKind::Program),
        ("integrity.json", EntryKind::Integrity),
    ] {
        let Some(entry) = entries.iter().find(|entry| entry.path == path) else {
            return Err(ArtifactError::MissingRequiredEntry(path));
        };
        if entry.kind != kind || !entry.essential {
            return Err(ArtifactError::InvalidRequiredEntryKind(path.to_owned()));
        }
    }
    Ok(())
}

#[must_use]
pub fn content_root(entries: &[ArtifactEntryInfo]) -> [u8; 32] {
    // Stable Merkle-like root over essential entry metadata, excluding signature envelopes.
    let mut rows = Vec::new();
    let mut essential: Vec<&ArtifactEntryInfo> = entries
        .iter()
        .filter(|entry| entry.essential && entry.kind != EntryKind::Signature)
        .collect();
    essential.sort_by(|left, right| left.path.cmp(&right.path));
    for entry in essential {
        rows.extend_from_slice(&(entry.kind as u8).to_le_bytes());
        rows.extend_from_slice(
            &(u32::try_from(entry.path.len()).unwrap_or(u32::MAX)).to_le_bytes(),
        );
        rows.extend_from_slice(entry.path.as_bytes());
        rows.extend_from_slice(&entry.digest);
    }
    sha256(&rows)
}

#[must_use]
pub fn describe_digest(digest: &[u8; 32]) -> String {
    hex(digest)
}

fn u16_at(bytes: &[u8], offset: usize) -> Result<u16, ArtifactError> {
    let slice = bytes
        .get(offset..offset + 2)
        .ok_or(ArtifactError::Truncated)?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}
fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, ArtifactError> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or(ArtifactError::Truncated)?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}
fn u64_at(bytes: &[u8], offset: usize) -> Result<u64, ArtifactError> {
    let slice = bytes
        .get(offset..offset + 8)
        .ok_or(ArtifactError::Truncated)?;
    Ok(u64::from_le_bytes(
        slice.try_into().map_err(|_| ArtifactError::Truncated)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core(extra: Vec<ArtifactEntry>) -> Vec<ArtifactEntry> {
        let mut entries = vec![
            ArtifactEntry {
                kind: EntryKind::Manifest,
                path: "manifest.json".into(),
                essential: true,
                bytes: b"{}".to_vec(),
            },
            ArtifactEntry {
                kind: EntryKind::Program,
                path: "program.json".into(),
                essential: true,
                bytes: b"{}".to_vec(),
            },
            ArtifactEntry {
                kind: EntryKind::Integrity,
                path: "integrity.json".into(),
                essential: true,
                bytes: b"{}".to_vec(),
            },
        ];
        entries.extend(extra);
        entries
    }

    #[test]
    fn identical_entries_produce_identical_bytes() {
        let a = build_artifact(core(vec![]), ArtifactLimits::default()).unwrap();
        let b = build_artifact(core(vec![]), ArtifactLimits::default()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn input_order_does_not_change_artifact() {
        let extra = ArtifactEntry {
            kind: EntryKind::Asset,
            path: "assets/a.bin".into(),
            essential: true,
            bytes: vec![1, 2, 3],
        };
        let left = core(vec![extra.clone()]);
        let mut right = left.clone();
        right.reverse();
        assert_eq!(
            build_artifact(left, ArtifactLimits::default()).unwrap(),
            build_artifact(right, ArtifactLimits::default()).unwrap()
        );
    }

    #[test]
    fn tampered_payload_is_rejected() {
        let mut bytes = build_artifact(core(vec![]), ArtifactLimits::default()).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        assert!(matches!(
            parse_artifact(&bytes, ArtifactLimits::default()),
            Err(ArtifactError::DigestMismatch(_))
        ));
    }

    #[test]
    fn noncanonical_payload_offset_is_rejected() {
        let mut bytes = build_artifact(core(vec![]), ArtifactLimits::default()).unwrap();
        // First table row starts at byte 24; its payload offset is at row+4. Move it by one byte
        // without changing any payload. A permissive parser could otherwise accept layout gaps.
        let offset = u64_at(&bytes, HEADER_LEN + 4).unwrap();
        bytes[HEADER_LEN + 4..HEADER_LEN + 12].copy_from_slice(&(offset + 1).to_le_bytes());
        assert!(matches!(
            parse_artifact(&bytes, ArtifactLimits::default()),
            Err(ArtifactError::TableNotCanonical)
        ));
    }

    #[test]
    fn entry_kind_and_essential_flag_are_canonical() {
        let result = build_artifact(
            core(vec![ArtifactEntry {
                kind: EntryKind::DebugMap,
                path: "debug/source-map.json".into(),
                essential: true,
                bytes: vec![],
            }]),
            ArtifactLimits::default(),
        );
        assert!(matches!(
            result,
            Err(ArtifactError::InvalidEntryConvention(_))
        ));
        let result = build_artifact(
            core(vec![ArtifactEntry {
                kind: EntryKind::Asset,
                path: "other.bin".into(),
                essential: true,
                bytes: vec![],
            }]),
            ArtifactLimits::default(),
        );
        assert!(matches!(
            result,
            Err(ArtifactError::InvalidEntryConvention(_))
        ));
    }

    #[test]
    fn caller_cannot_relax_canonical_artifact_ceiling() {
        let mut limits = ArtifactLimits::default();
        limits.max_total_bytes += 1;
        assert!(matches!(
            build_artifact(core(vec![]), limits),
            Err(ArtifactError::InvalidLimits(_))
        ));
    }

    #[test]
    fn metadata_entries_have_stricter_canonical_byte_ceilings() {
        let exact_limits = ArtifactLimits {
            max_manifest_bytes: 2,
            ..ArtifactLimits::default()
        };
        assert!(build_artifact(core(vec![]), exact_limits).is_ok());

        let too_small_limits = ArtifactLimits {
            max_manifest_bytes: 1,
            ..ArtifactLimits::default()
        };
        let result = build_artifact(core(vec![]), too_small_limits);
        assert!(
            matches!(result, Err(ArtifactError::EntryTooLarge(path)) if path == "manifest.json")
        );
    }

    #[test]
    fn traversal_path_is_rejected() {
        let result = build_artifact(
            core(vec![ArtifactEntry {
                kind: EntryKind::Asset,
                path: "assets/../secret".into(),
                essential: true,
                bytes: vec![],
            }]),
            ArtifactLimits::default(),
        );
        assert!(matches!(result, Err(ArtifactError::InvalidPath(_))));
    }
}
