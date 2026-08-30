//! Lightweight typed entity extraction used to build semantic views and slot candidates.
//!
//! Entity recognition deliberately inspects both raw and normalized text. Structural values such
//! as `17:30`, e-mail addresses and URLs contain punctuation that the lexical normalizer removes;
//! deriving entities only from normalized text would therefore destroy evidence before extraction.

use std::collections::{BTreeMap, BTreeSet};

use gvya_model::Value;

use super::{normalization::ordered_tokens, profile::SemanticProfile};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EntityKind(String);

impl EntityKind {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntityStatus {
    Resolved,
    Suggested,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticEntity {
    pub kind: EntityKind,
    pub value: Value,
    pub source_text: String,
    pub token: String,
    pub status: EntityStatus,
    pub confidence: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EntityExtraction {
    pub entities: Vec<SemanticEntity>,
    pub entity_text: String,
    pub clean_text: String,
}

#[must_use]
pub fn extract_entities(
    raw: &str,
    normalized: &str,
    profile: &SemanticProfile,
) -> EntityExtraction {
    let normalized = if normalized.is_empty() {
        profile.normalize_text(raw)
    } else {
        normalized.to_string()
    };
    let normalized_tokens = ordered_tokens(&normalized);
    let raw_structural = structural_tokens(&profile.rewrite_characters(raw));
    let mut entities = Vec::new();

    extract_numbers(&normalized_tokens, profile, &mut entities);
    extract_dates(&normalized_tokens, profile, &mut entities);
    extract_times(&normalized_tokens, &raw_structural, profile, &mut entities);
    extract_colors(&normalized_tokens, profile, &mut entities);
    extract_quantities(&normalized_tokens, profile, &mut entities);
    extract_email_phone_url(
        &profile.rewrite_characters(raw),
        &raw_structural,
        &mut entities,
    );
    extract_origin(&normalized_tokens, profile, &mut entities);
    extract_custom_entities(&normalized_tokens, profile, &mut entities);
    dedupe(&mut entities);

    let mut by_len = entities.clone();
    by_len.sort_by_key(|entity| std::cmp::Reverse(entity.source_text.chars().count()));

    let mut entity_text = normalized.clone();
    for entity in &by_len {
        let replacement = entity_replacement_token(entity, &entities);
        replace_entity_source(&mut entity_text, &entity.source_text, &replacement);
    }

    // Dates remain useful lexical evidence (`tomorrow`) while highly structural entities are
    // removed from the clean view. This matches the proven behavior floor more closely than
    // deleting dates and preserves natural-language date intent evidence.
    let clean_kinds = ["time", "email", "phone", "url"];
    let mut clean_text = normalized.clone();
    for entity in &by_len {
        if clean_kinds.contains(&entity.kind.as_str()) {
            replace_entity_source(&mut clean_text, &entity.source_text, " ");
        }
    }
    clean_text = collapse_spaces(&clean_text);
    if clean_text.is_empty() {
        clean_text = normalized.clone();
    }

    EntityExtraction {
        entities,
        entity_text: collapse_spaces(&entity_text),
        clean_text,
    }
}

fn extract_custom_entities(
    tokens: &[String],
    profile: &SemanticProfile,
    out: &mut Vec<SemanticEntity>,
) {
    for (kind, values) in &profile.custom_entities {
        for (canonical, aliases) in values {
            for surface in std::iter::once(canonical).chain(aliases.iter()) {
                let normalized = profile.normalize_text(surface);
                let alias_tokens =
                    profile.normalize_colloquial_tokens(&ordered_tokens(&normalized));
                if alias_tokens.is_empty() {
                    continue;
                }
                if let Some(start) = find_exact_sequence(
                    tokens,
                    &alias_tokens.iter().map(String::as_str).collect::<Vec<_>>(),
                ) {
                    let source = tokens[start..start + alias_tokens.len()].join(" ");
                    push(
                        out,
                        kind,
                        Value::String(canonical.clone()),
                        &source,
                        &format!("NLUCUSTOM{}", kind.to_ascii_uppercase()),
                        EntityStatus::Resolved,
                        1.0,
                    );
                }
            }
        }
    }
}

fn entity_replacement_token(entity: &SemanticEntity, all: &[SemanticEntity]) -> String {
    let source_tokens = ordered_tokens(&entity.source_text);
    if source_tokens.len() < 2 {
        return entity.token.clone();
    }
    let mut tokens = vec![entity.token.clone()];
    for nested in all {
        if nested.source_text == entity.source_text || nested.token == entity.token {
            continue;
        }
        let nested_tokens = ordered_tokens(&nested.source_text);
        if nested_tokens.is_empty() || nested_tokens.len() >= source_tokens.len() {
            continue;
        }
        if source_tokens
            .windows(nested_tokens.len())
            .any(|window| window == nested_tokens.as_slice())
            && !tokens.contains(&nested.token)
        {
            tokens.push(nested.token.clone());
        }
    }
    tokens.join(" ")
}

fn push(
    entities: &mut Vec<SemanticEntity>,
    kind: &str,
    value: Value,
    source: &str,
    token: &str,
    status: EntityStatus,
    confidence: f32,
) {
    entities.push(SemanticEntity {
        kind: EntityKind::new(kind),
        value,
        source_text: source.to_string(),
        token: token.to_string(),
        status,
        confidence,
    });
}

fn extract_numbers(tokens: &[String], profile: &SemanticProfile, out: &mut Vec<SemanticEntity>) {
    let words = &profile.number_words;
    for token in tokens {
        if let Ok(value) = token.parse::<f64>() {
            push(
                out,
                "number",
                Value::Number(value),
                token,
                "NLUNUMBER",
                EntityStatus::Resolved,
                1.0,
            );
        } else if let Some(value) = words.get(token.as_str()) {
            push(
                out,
                "number",
                Value::Number(*value),
                token,
                "NLUNUMBER",
                EntityStatus::Resolved,
                0.98,
            );
        }
    }
}

fn extract_dates(tokens: &[String], profile: &SemanticProfile, out: &mut Vec<SemanticEntity>) {
    for token in tokens {
        if is_iso_date(token) {
            push(
                out,
                "date",
                Value::String(token.clone()),
                token,
                "NLUDATE",
                EntityStatus::Resolved,
                1.0,
            );
            continue;
        }
        let relative = profile.relative_dates.get(token);
        if let Some(relative) = relative {
            // Relative dates stay symbolic. Resolving them requires explicit host time later.
            push(
                out,
                "date",
                Value::String(format!("relative:{relative}")),
                token,
                "NLUDATE",
                EntityStatus::Suggested,
                0.98,
            );
        }
    }
}

fn extract_times(
    tokens: &[String],
    raw_tokens: &[String],
    profile: &SemanticProfile,
    out: &mut Vec<SemanticEntity>,
) {
    for token in raw_tokens {
        if let Some(value) = parse_clock_token(token) {
            push(
                out,
                "time",
                Value::String(value),
                token,
                "NLUTIME",
                EntityStatus::Resolved,
                1.0,
            );
        }
    }

    // Natural language clock forms whose punctuation is not structurally significant.
    for (index, token) in tokens.iter().enumerate() {
        let previous = index
            .checked_sub(1)
            .and_then(|i| tokens.get(i))
            .map(String::as_str);
        if previous.is_some_and(|marker| profile.time_markers.contains(marker)) {
            if let Some(hour) = parse_number_token(token, profile)
                .filter(|value| value.fract() == 0.0 && *value >= 0.0 && *value <= 23.0)
            {
                push(
                    out,
                    "time",
                    Value::String(format!("{:02}:00", hour as u8)),
                    token,
                    "NLUTIME",
                    EntityStatus::Suggested,
                    0.92,
                );
            }
        }
    }
}

fn extract_colors(tokens: &[String], profile: &SemanticProfile, out: &mut Vec<SemanticEntity>) {
    for token in tokens {
        if let Some(canonical) = profile.colors.get(token) {
            push(
                out,
                "color",
                Value::String(canonical.clone()),
                token,
                "NLUCOLOR",
                EntityStatus::Resolved,
                1.0,
            );
        }
    }
}

fn extract_quantities(tokens: &[String], profile: &SemanticProfile, out: &mut Vec<SemanticEntity>) {
    let units = &profile.units;

    let mut add_quantity = |number: &str, unit: &str, source: String| {
        let Some(value) = parse_number_token(number, profile) else {
            return;
        };
        let mut object = BTreeMap::new();
        object.insert("value".to_string(), Value::Number(value));
        object.insert("unit".to_string(), Value::String(unit.to_string()));
        push(
            out,
            "quantity",
            Value::Object(object),
            &source,
            "NLUQUANTITY",
            EntityStatus::Resolved,
            1.0,
        );
        push(
            out,
            "unit",
            Value::String(unit.to_string()),
            unit,
            "NLUUNIT",
            EntityStatus::Resolved,
            1.0,
        );
    };

    for window in tokens.windows(2) {
        if let Some(unit) = units.get(&window[1]) {
            add_quantity(&window[0], unit, format!("{} {}", window[0], window[1]));
        }
    }

    // An authored color may bridge a count and an authored quantity unit.
    for window in tokens.windows(3) {
        if profile.colors.contains_key(&window[1]) {
            if let Some(unit) = units.get(&window[2]) {
                add_quantity(
                    &window[0],
                    unit,
                    format!("{} {} {}", window[0], window[1], window[2]),
                );
            }
        }
    }
}

fn extract_email_phone_url(raw: &str, raw_tokens: &[String], out: &mut Vec<SemanticEntity>) {
    for token in raw_tokens {
        if looks_like_url(token) {
            push(
                out,
                "url",
                Value::String(token.clone()),
                token,
                "NLUURL",
                EntityStatus::Resolved,
                1.0,
            );
            continue;
        }
        if looks_like_email(token) {
            push(
                out,
                "email",
                Value::String(token.to_lowercase()),
                token,
                "NLUEMAIL",
                EntityStatus::Resolved,
                1.0,
            );
            continue;
        }
        if looks_like_phone(token) {
            push(
                out,
                "phone",
                Value::String(compact_phone(token)),
                token,
                "NLUPHONE",
                EntityStatus::Suggested,
                0.94,
            );
        }
    }

    // Preserve support for formatted numbers containing spaces by scanning bounded raw chunks.
    let words: Vec<&str> = raw.split_whitespace().collect();
    for width in 2..=4 {
        for window in words.windows(width) {
            let candidate = clean_structural_token(&window.join(" "));
            if looks_like_phone(&candidate) {
                push(
                    out,
                    "phone",
                    Value::String(compact_phone(&candidate)),
                    &candidate,
                    "NLUPHONE",
                    EntityStatus::Suggested,
                    0.88,
                );
            }
        }
    }
}

fn extract_origin(tokens: &[String], profile: &SemanticProfile, out: &mut Vec<SemanticEntity>) {
    for (prefix_text, stops) in &profile.origin_prefixes {
        let prefix_owned = ordered_tokens(prefix_text);
        let prefix: Vec<&str> = prefix_owned.iter().map(String::as_str).collect();
        if let Some(start) = find_exact_sequence(tokens, &prefix) {
            let mut place = Vec::new();
            for token in tokens.iter().skip(start + prefix.len()).take(6) {
                if stops.iter().any(|stop| stop == token) {
                    break;
                }
                place.push(token.clone());
            }
            if !place.is_empty() {
                let value = place.join(" ");
                push(
                    out,
                    "origin",
                    Value::String(value.clone()),
                    &value,
                    "NLUORIGIN",
                    EntityStatus::Suggested,
                    0.78,
                );
                return;
            }
        }
    }
}

/// Canonical value authority for one built-in entity kind.
///
/// This is exactly the value shape the deterministic extractor emits, expressed as a membership
/// test so an untrusted resolver proposal passes the same canonicalization rules as ordinary
/// semantic binding instead of a second, looser type system. `None` means the kind is not a
/// built-in entity, and the caller must fall back to the authored custom entity catalog.
#[must_use]
pub(super) fn builtin_entity_value_is_canonical(
    kind: &str,
    value: &Value,
    profile: &SemanticProfile,
) -> Option<bool> {
    fn text(value: &Value) -> Option<&str> {
        match value {
            Value::String(text) => Some(text.as_str()),
            _ => None,
        }
    }
    Some(match kind {
        "number" => matches!(value, Value::Number(number) if number.is_finite()),
        "date" => text(value).is_some_and(|text| {
            is_iso_date(text)
                || text.strip_prefix("relative:").is_some_and(|relative| {
                    profile.relative_dates.values().any(|row| row == relative)
                })
        }),
        "time" => text(value).is_some_and(|text| parse_clock_token(text).as_deref() == Some(text)),
        "color" => text(value).is_some_and(|text| profile.colors.values().any(|row| row == text)),
        "unit" => text(value).is_some_and(|text| profile.units.values().any(|row| row == text)),
        "quantity" => quantity_value_is_canonical(value, profile),
        "email" => {
            text(value).is_some_and(|text| looks_like_email(text) && text == text.to_lowercase())
        }
        "url" => text(value).is_some_and(looks_like_url),
        "phone" => {
            text(value).is_some_and(|text| looks_like_phone(text) && compact_phone(text) == text)
        }
        "origin" => text(value).is_some_and(|text| !text.is_empty() && text.trim() == text),
        _ => return None,
    })
}

fn quantity_value_is_canonical(value: &Value, profile: &SemanticProfile) -> bool {
    let Value::Object(fields) = value else {
        return false;
    };
    if fields.len() != 2 {
        return false;
    }
    let Some(Value::Number(number)) = fields.get("value") else {
        return false;
    };
    let Some(Value::String(unit)) = fields.get("unit") else {
        return false;
    };
    number.is_finite() && profile.units.values().any(|row| row == unit)
}

fn parse_number_token(token: &str, profile: &SemanticProfile) -> Option<f64> {
    token
        .parse::<f64>()
        .ok()
        .or_else(|| profile.number_words.get(token).copied())
}

fn parse_clock_token(token: &str) -> Option<String> {
    let lower = token.to_lowercase();
    let (clock, suffix) = if let Some(value) = lower.strip_suffix("am") {
        (value, Some("am"))
    } else if let Some(value) = lower.strip_suffix("pm") {
        (value, Some("pm"))
    } else {
        (lower.as_str(), None)
    };
    let (hour, minute) = clock.split_once(':')?;
    let mut hour = hour.parse::<u8>().ok()?;
    let minute = minute.parse::<u8>().ok()?;
    if minute > 59 {
        return None;
    }
    if let Some(suffix) = suffix {
        if hour == 0 || hour > 12 {
            return None;
        }
        if suffix == "pm" && hour != 12 {
            hour += 12;
        }
        if suffix == "am" && hour == 12 {
            hour = 0;
        }
    } else if hour > 23 {
        return None;
    }
    Some(format!("{hour:02}:{minute:02}"))
}

fn looks_like_email(token: &str) -> bool {
    let Some((local, domain)) = token.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && domain.rsplit('.').next().is_some_and(|suffix| {
            suffix.len() >= 2 && suffix.chars().all(|ch| ch.is_ascii_alphabetic())
        })
}

fn looks_like_url(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("www.")
}

fn looks_like_phone(token: &str) -> bool {
    let candidate = token.trim();
    let body = candidate.strip_prefix('+').unwrap_or(candidate);
    let digits = body.chars().filter(char::is_ascii_digit).count();
    digits >= 7
        && digits <= 18
        && body
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, '-' | '(' | ')' | ' ' | '.'))
}

fn compact_phone(token: &str) -> String {
    let plus = token.trim_start().starts_with('+');
    let digits: String = token.chars().filter(char::is_ascii_digit).collect();
    if plus { format!("+{digits}") } else { digits }
}

fn is_iso_date(token: &str) -> bool {
    let parts: Vec<_> = token.split('-').collect();
    if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return false;
    }
    let (Ok(year), Ok(month), Ok(day)) = (
        parts[0].parse::<u16>(),
        parts[1].parse::<u8>(),
        parts[2].parse::<u8>(),
    ) else {
        return false;
    };
    year > 0 && (1..=12).contains(&month) && (1..=days_in_month(year, month)).contains(&day)
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 31,
    }
}

fn is_leap_year(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn dedupe(entities: &mut Vec<SemanticEntity>) {
    let mut seen = BTreeSet::new();
    entities.retain(|entity| {
        // Equivalent values discovered through raw/normalized or overlapping structural scans are
        // one semantic candidate. Keeping duplicate detector paths would create false ambiguity.
        let value = format!("{:?}", entity.value);
        seen.insert(format!("{}|{value}", entity.kind.as_str()))
    });
}

fn structural_tokens(raw: &str) -> Vec<String> {
    raw.split_whitespace()
        .map(clean_structural_token)
        .filter(|token| !token.is_empty())
        .collect()
}

fn clean_structural_token(token: &str) -> String {
    token
        .trim()
        .trim_matches(|ch: char| {
            matches!(
                ch,
                ',' | ';'
                    | '!'
                    | '?'
                    | '؟'
                    | '،'
                    | '؛'
                    | '"'
                    | '\''
                    | '`'
                    | '['
                    | ']'
                    | '{'
                    | '}'
                    | '('
                    | ')'
                    | '.'
            )
        })
        .to_string()
}

fn replace_entity_source(text: &mut String, source: &str, replacement: &str) {
    if source.is_empty() {
        return;
    }
    let normalized_source = source.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized_source.is_empty() {
        return;
    }
    *text = replace_token_phrase(text, &normalized_source, replacement);
}

fn replace_token_phrase(text: &str, source: &str, replacement: &str) -> String {
    let padded = format!(" {text} ");
    let needle = format!(" {source} ");
    collapse_spaces(&padded.replace(&needle, &format!(" {replacement} ")))
}

fn find_exact_sequence(tokens: &[String], prefix: &[&str]) -> Option<usize> {
    if prefix.is_empty() || prefix.len() > tokens.len() {
        return None;
    }
    (0..=tokens.len() - prefix.len()).find(|start| {
        prefix
            .iter()
            .enumerate()
            .all(|(offset, expected)| tokens[start + offset] == *expected)
    })
}

fn collapse_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> SemanticProfile {
        let mut profile = SemanticProfile::empty();
        profile.number_words.insert("two".into(), 2.0);
        profile
            .relative_dates
            .insert("tomorrow".into(), "tomorrow".into());
        profile.colors.insert("red".into(), "red".into());
        profile.units.insert("boxes".into(), "box".into());
        profile.time_markers.insert("at".into());
        profile
    }

    #[test]
    fn structural_and_authored_entities_are_separate() {
        let profile = profile();
        let raw = "bring 2 red boxes tomorrow at 5 test@example.com https://example.com at 17:30";
        let norm = profile.normalize_text(raw);
        let result = extract_entities(raw, &norm, &profile);
        let kinds: Vec<_> = result.entities.iter().map(|e| e.kind.as_str()).collect();
        for expected in [
            "number", "quantity", "color", "date", "time", "email", "url",
        ] {
            assert!(kinds.contains(&expected), "missing {expected}");
        }
    }

    #[test]
    fn language_vocabulary_is_not_built_in() {
        let neutral = SemanticProfile::empty();
        let result = extract_entities("tomorrow red boxes", "tomorrow red boxes", &neutral);
        assert!(
            !result
                .entities
                .iter()
                .any(|row| matches!(row.kind.as_str(), "date" | "color" | "unit" | "quantity"))
        );
    }

    #[test]
    fn rejects_invalid_calendar_dates() {
        let profile = profile();
        let result = extract_entities("2025-02-31", "2025-02-31", &profile);
        assert!(
            !result
                .entities
                .iter()
                .any(|entity| entity.kind.as_str() == "date")
        );
    }
}
