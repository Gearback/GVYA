//! Deterministic Unicode normalization and language-tag helpers.

use super::unicode_nfc::nfc;

/// Canonical language-neutral lexical normalization.
///
/// GVYA first NFC-normalizes Unicode, applies Unicode lowercase, folds only structural sentence
/// punctuation to spaces, and collapses whitespace. Language-specific character rewrites,
/// diacritic stripping and morphology belong to authored `SemanticProfile` data.
#[must_use]
pub fn normalize_text(input: &str) -> String {
    let canonical = nfc(input);
    let mut out = String::with_capacity(canonical.len());
    let mut last_was_space = true;
    for raw in canonical.trim().chars().flat_map(char::to_lowercase) {
        let ch = match raw {
            '!' | '?' | '.' | ',' | ';' | ':' | '-' | '‐' | '‑' | '‒' | '–' | '—' | '―' | '،'
            | '؛' | '؟' => ' ',
            other => other,
        };
        if ch.is_whitespace() {
            if !last_was_space {
                out.push(' ');
                last_was_space = true;
            }
        } else {
            out.push(ch);
            last_was_space = false;
        }
    }
    out.trim().to_string()
}

#[must_use]
pub fn ordered_tokens(normalized: &str) -> Vec<String> {
    normalized.split_whitespace().map(str::to_string).collect()
}

#[must_use]
pub fn normalize_meta_text(input: &str) -> Vec<String> {
    let replaced: String = input
        .chars()
        .map(|ch| {
            if matches!(ch, '_' | '/' | '.' | '-') {
                ' '
            } else {
                ch
            }
        })
        .collect();
    ordered_tokens(&normalize_text(&replaced))
}

/// Canonical BCP47 comparison form used by semantic authority. This is deliberately ASCII-only;
/// BCP47 syntax is ASCII and must never depend on process/browser locale.
#[must_use]
pub fn normalize_language_tag(raw: &str) -> String {
    raw.trim().replace('_', "-").to_ascii_lowercase()
}

/// Requested semantic language fallback order: exact/base followed by explicit fallback tags.
/// The kernel never injects a natural language or `und` by ambient policy.
#[must_use]
pub fn language_fallbacks(requested: Option<&str>, explicit_fallbacks: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(requested) = requested {
        push_language_candidates(&mut out, requested);
    }
    for fallback in explicit_fallbacks {
        push_language_candidates(&mut out, fallback);
    }
    out
}

#[must_use]
pub fn language_is_compatible(
    requested: Option<&str>,
    explicit_fallbacks: &[String],
    authored: &str,
) -> bool {
    let authored = normalize_language_tag(authored);
    language_fallbacks(requested, explicit_fallbacks)
        .iter()
        .any(|row| row == &authored)
}

fn push_language_candidates(out: &mut Vec<String>, raw: &str) {
    let normalized = normalize_language_tag(raw);
    if normalized.is_empty() {
        return;
    }
    if !out.contains(&normalized) {
        out.push(normalized.clone());
    }
    if let Some((base, _)) = normalized.split_once('-') {
        let base = base.to_string();
        if !out.contains(&base) {
            out.push(base);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_is_unicode_canonical_but_language_neutral() {
        assert_eq!(normalize_text("  CAFÉ?!  "), "café");
        assert_eq!(normalize_text("e\u{301}"), normalize_text("é"));
        assert_eq!(
            normalize_text("voice-controlled—device"),
            "voice controlled device"
        );
        // Arabic/Persian variants are intentionally not silently rewritten by the kernel.
        assert_ne!(normalize_text("ي"), normalize_text("ی"));
    }

    #[test]
    fn language_fallbacks_are_exact_base_then_explicit_policy() {
        assert_eq!(
            language_fallbacks(Some("EN_us"), &["und".to_string()]),
            vec!["en-us", "en", "und"]
        );
        assert!(language_fallbacks(None, &[]).is_empty());
        assert_eq!(language_fallbacks(None, &["und".to_string()]), vec!["und"]);
        assert!(language_is_compatible(Some("en-US"), &[], "en"));
        assert!(!language_is_compatible(Some("en-US"), &[], "de"));
    }
}
