//! Explicit ordered structural-pattern matching.
//!
//! Structural patterns are author rules, not semantic evidence. A structural winner is resolved
//! before sample scoring. The grammar is intentionally small and AIML-inspired:
//! - literal tokens match in order against the whole utterance;
//! - `*` matches one or more tokens;
//! - `^` matches zero or more tokens;
//! - `*{slot}` / `^{slot}` capture the wildcard span into a declared String slot;
//! - `<set:name>` matches an authored matcher-profile set;
//! - `<set:name>{slot}` captures that set's canonical value into a String slot;
//! - `<set:entity.kind>{slot}` binds an authored custom Entity slot of the same kind.

use std::collections::{BTreeMap, BTreeSet};

use gvya_model::{Meaning, MeaningId, SlotValue, Value, ValueProvenance};

use super::{
    MeaningPattern, PartialMeaning, SemanticEntity, SemanticProfile, SemanticProfiles, SlotKind,
    normalization::{language_is_compatible, normalize_language_tag, ordered_tokens},
    profile::profile_for_authored_language,
    resolver::{BindOutcome, bind_meaning_with_slots},
};

pub const SEMANTIC_PATTERNS_PER_MEANING_MAX: usize = 64;
pub const SEMANTIC_STRUCTURAL_RULES_MAX: usize = 8_192;
pub const SEMANTIC_PATTERN_ATOMS_MAX: usize = 128;
pub const SEMANTIC_PATTERN_CAPTURES_MAX: usize = 32;
/// Hard per-analysis work budget for structural matching. Exhaustion fails closed instead of
/// falling through to semantic scoring or accepting a catalog-order partial result.
pub const SEMANTIC_STRUCTURAL_WORK_MAX: usize = 1_000_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalizedStructuralPattern {
    pub language: String,
    pub text: String,
    pub priority: i32,
}

impl LocalizedStructuralPattern {
    #[must_use]
    pub fn new(language: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            language: language.into(),
            text: text.into(),
            priority: 0,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StructuralPatternError {
    EmptyPattern,
    TooManyAtoms,
    TooManyCaptures,
    InvalidCaptureName(String),
    DuplicateCapture(String),
    InvalidSetName(String),
    CaptureSlotMissing(String),
    CaptureSlotMustBeString(String),
    CaptureSlotSetKindMismatch(String),
    LiteralNormalizesEmpty(String),
    NoAnchor,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Atom {
    Literal(String),
    Wildcard {
        min: usize,
        capture: Option<String>,
    },
    Set {
        name: String,
        capture: Option<String>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParsedPattern {
    atoms: Vec<Atom>,
    literal_atoms: usize,
    set_atoms: usize,
    wildcard_atoms: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StructuralMatchSummary {
    pub meaning: MeaningId,
    pub pattern: String,
    pub language: String,
    pub priority: i32,
    pub captures: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum StructuralDecision {
    NoMatch,
    BudgetExceeded,
    Resolved {
        meaning: Meaning,
        summary: StructuralMatchSummary,
    },
    Partial {
        partial: PartialMeaning,
        summary: StructuralMatchSummary,
    },
    Ambiguous {
        candidates: Vec<MeaningId>,
        reason_code: String,
        summary: Option<StructuralMatchSummary>,
    },
    Invalid {
        reason_code: String,
        summary: StructuralMatchSummary,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StructuralMatcherBuildError {
    TooManyRules,
    MissingLanguageProfile(String),
    InvalidPattern {
        meaning: String,
        pattern: String,
        reason: StructuralPatternError,
    },
    UnknownSet {
        meaning: String,
        pattern: String,
        set: String,
    },
    InvalidSetName {
        set: String,
    },
    InvalidSetAlias {
        set: String,
        alias: String,
    },
    InvalidSetCanonical {
        set: String,
        alias: String,
    },
    ConflictingSetAlias {
        set: String,
        alias: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct StructuralMatcher {
    rules: Vec<CompiledRule>,
    sets_by_language: BTreeMap<String, BTreeMap<String, Vec<CompiledSetAlias>>>,
}

#[derive(Clone, Debug, PartialEq)]
struct CompiledRule {
    pattern_index: usize,
    rule_index: usize,
    meaning_id: MeaningId,
    meaning_priority: i32,
    rule: LocalizedStructuralPattern,
    parsed: ParsedPattern,
}

#[derive(Clone, Debug, PartialEq)]
struct RuleMatch {
    pattern_index: usize,
    rule_index: usize,
    meaning_id: MeaningId,
    meaning_priority: i32,
    rule_priority: i32,
    language: String,
    pattern_text: String,
    literal_atoms: usize,
    set_atoms: usize,
    wildcard_atoms: usize,
    set_tokens: usize,
    wildcard_tokens: usize,
    captures: BTreeMap<String, String>,
    captures_ambiguous: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct PathResult {
    captures: BTreeMap<String, String>,
    set_tokens: usize,
    wildcard_tokens: usize,
    captures_ambiguous: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CompiledSetAlias {
    tokens: Vec<String>,
    canonical: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SetCandidate {
    consumed: usize,
    canonical: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct InputToken {
    surface: String,
    canonical: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MatchBudget {
    remaining: usize,
}

impl MatchBudget {
    fn new(limit: usize) -> Self {
        Self { remaining: limit }
    }

    fn spend(&mut self) -> Result<(), ()> {
        if self.remaining == 0 {
            Err(())
        } else {
            self.remaining -= 1;
            Ok(())
        }
    }
}

pub(super) fn validate_pattern(
    pattern: &MeaningPattern,
    rule: &LocalizedStructuralPattern,
) -> Result<(), StructuralPatternError> {
    let parsed = parse_pattern(&rule.text)?;
    let slots: BTreeMap<_, _> = pattern
        .slots
        .iter()
        .map(|slot| (slot.name.as_str(), &slot.kind))
        .collect();
    for atom in &parsed.atoms {
        let Some(capture) = atom_capture(atom) else {
            continue;
        };
        let Some(kind) = slots.get(capture.as_str()) else {
            return Err(StructuralPatternError::CaptureSlotMissing(capture));
        };
        match atom {
            Atom::Wildcard { .. } if !matches!(kind, SlotKind::String) => {
                return Err(StructuralPatternError::CaptureSlotMustBeString(capture));
            }
            Atom::Set { .. } if !matches!(kind, SlotKind::String | SlotKind::Entity(_)) => {
                return Err(StructuralPatternError::CaptureSlotSetKindMismatch(capture));
            }
            Atom::Literal(_) | Atom::Wildcard { .. } | Atom::Set { .. } => {}
        }
    }
    Ok(())
}

pub fn structural_pattern_set_names(text: &str) -> Result<Vec<String>, StructuralPatternError> {
    let parsed = parse_pattern(text)?;
    let mut names = BTreeSet::new();
    for atom in &parsed.atoms {
        if let Atom::Set { name, .. } = atom {
            names.insert(name.clone());
        }
    }
    Ok(names.into_iter().collect())
}

/// Validates the complete structural matcher contract for a composed catalog/profile pair.
/// This is compiler-facing validation; runtimes build the same matcher and therefore enforce
/// the identical contract again at load time.
pub fn validate_structural_matcher(
    catalog: &[MeaningPattern],
    profiles: &SemanticProfiles,
) -> Result<(), StructuralMatcherBuildError> {
    StructuralMatcher::build(catalog, profiles).map(|_| ())
}

impl StructuralMatcher {
    pub(super) fn build(
        catalog: &[MeaningPattern],
        profiles: &SemanticProfiles,
    ) -> Result<Self, StructuralMatcherBuildError> {
        let rule_count = catalog
            .iter()
            .map(|meaning| meaning.patterns.len())
            .sum::<usize>();
        if rule_count > SEMANTIC_STRUCTURAL_RULES_MAX {
            return Err(StructuralMatcherBuildError::TooManyRules);
        }
        let mut rules = Vec::with_capacity(rule_count);
        let mut used_sets: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (pattern_index, meaning) in catalog.iter().enumerate() {
            for (rule_index, rule) in meaning.patterns.iter().enumerate() {
                let language = normalize_language_tag(&rule.language);
                let profile =
                    profile_for_authored_language(profiles, &rule.language).ok_or_else(|| {
                        StructuralMatcherBuildError::MissingLanguageProfile(language.clone())
                    })?;
                let parsed = parse_pattern(&rule.text)
                    .and_then(|parsed| canonicalize_pattern_literals(parsed, profile))
                    .and_then(|parsed| {
                        validate_pattern_capture_sets(meaning, &parsed, profile)?;
                        Ok(parsed)
                    })
                    .map_err(|reason| StructuralMatcherBuildError::InvalidPattern {
                        meaning: meaning.id.as_str().to_string(),
                        pattern: rule.text.clone(),
                        reason,
                    })?;
                for atom in &parsed.atoms {
                    if let Atom::Set { name, .. } = atom {
                        if !profile.pattern_sets.contains_key(name)
                            && custom_entity_kind(name)
                                .is_none_or(|kind| !profile.custom_entities.contains_key(kind))
                        {
                            return Err(StructuralMatcherBuildError::UnknownSet {
                                meaning: meaning.id.as_str().to_string(),
                                pattern: rule.text.clone(),
                                set: name.clone(),
                            });
                        }
                        used_sets
                            .entry(language.clone())
                            .or_default()
                            .insert(name.clone());
                    }
                }
                rules.push(CompiledRule {
                    pattern_index,
                    rule_index,
                    meaning_id: meaning.id.clone(),
                    meaning_priority: meaning.priority,
                    rule: rule.clone(),
                    parsed,
                });
            }
        }
        let mut sets_by_language = BTreeMap::new();
        for (language, names) in used_sets {
            let profile = profiles.get(&language).ok_or_else(|| {
                StructuralMatcherBuildError::MissingLanguageProfile(language.clone())
            })?;
            let mut sets = BTreeMap::new();
            for name in names {
                if validate_identifier(&name).is_err() {
                    return Err(StructuralMatcherBuildError::InvalidSetName { set: name });
                }
                let compiled = compile_pattern_set(profile, &name)?;
                sets.insert(name, compiled);
            }
            sets_by_language.insert(language, sets);
        }
        Ok(Self {
            rules,
            sets_by_language,
        })
    }

    pub(super) fn resolve(
        &self,
        catalog: &[MeaningPattern],
        profile: &SemanticProfile,
        normalized_input: &str,
        entities: &[SemanticEntity],
        references: &[crate::ResolverReferenceCandidate],
        requested_language: Option<&str>,
        language_fallbacks: &[String],
        permitted: impl Fn(&MeaningPattern) -> bool,
    ) -> StructuralDecision {
        let input_tokens = structural_input_tokens(profile, normalized_input);
        let mut matches = Vec::new();
        let mut budget = MatchBudget::new(SEMANTIC_STRUCTURAL_WORK_MAX);

        for compiled in &self.rules {
            if budget.spend().is_err() {
                return StructuralDecision::BudgetExceeded;
            }
            let Some(meaning) = catalog.get(compiled.pattern_index) else {
                continue;
            };
            if !permitted(meaning)
                || !language_is_compatible(
                    requested_language,
                    language_fallbacks,
                    &compiled.rule.language,
                )
            {
                continue;
            }
            let language = normalize_language_tag(&compiled.rule.language);
            let empty_sets = BTreeMap::new();
            let sets = self.sets_by_language.get(&language).unwrap_or(&empty_sets);
            let path = match match_parsed(&compiled.parsed, &input_tokens, sets, &mut budget) {
                Ok(Some(path)) => path,
                Ok(None) => continue,
                Err(()) => return StructuralDecision::BudgetExceeded,
            };
            matches.push(RuleMatch {
                pattern_index: compiled.pattern_index,
                rule_index: compiled.rule_index,
                meaning_id: compiled.meaning_id.clone(),
                meaning_priority: compiled.meaning_priority,
                rule_priority: compiled.rule.priority,
                language: compiled.rule.language.clone(),
                pattern_text: compiled.rule.text.clone(),
                literal_atoms: compiled.parsed.literal_atoms,
                set_atoms: compiled.parsed.set_atoms,
                wildcard_atoms: compiled.parsed.wildcard_atoms,
                set_tokens: path.set_tokens,
                wildcard_tokens: path.wildcard_tokens,
                captures: path.captures,
                captures_ambiguous: path.captures_ambiguous,
            });
        }

        let Some(best) = matches
            .iter()
            .max_by(|left, right| compare_rule_match(left, right))
        else {
            return StructuralDecision::NoMatch;
        };
        let best_key = specificity_key(best);
        let mut top: Vec<_> = matches
            .iter()
            .filter(|candidate| specificity_key(candidate) == best_key)
            .collect();
        top.sort_by(|left, right| {
            left.meaning_id
                .cmp(&right.meaning_id)
                .then_with(|| left.rule_index.cmp(&right.rule_index))
        });

        let top_meanings: BTreeSet<_> = top.iter().map(|row| row.meaning_id.clone()).collect();
        if top_meanings.len() > 1 {
            return StructuralDecision::Ambiguous {
                candidates: top_meanings.into_iter().collect(),
                reason_code: "structural_patterns_tied".to_string(),
                summary: Some(summary(best)),
            };
        }

        if top.iter().any(|candidate| candidate.captures_ambiguous)
            || top
                .iter()
                .skip(1)
                .any(|candidate| candidate.captures != top[0].captures)
        {
            return StructuralDecision::Ambiguous {
                candidates: vec![best.meaning_id.clone()],
                reason_code: "structural_captures_tied".to_string(),
                summary: Some(summary(best)),
            };
        }

        let Some(meaning_pattern) = catalog.get(best.pattern_index) else {
            return StructuralDecision::NoMatch;
        };
        let captured_slots: Vec<_> = best
            .captures
            .iter()
            .map(|(name, value)| SlotValue {
                name: name.clone(),
                value: Value::String(value.clone()),
                provenance: ValueProvenance::Utterance,
            })
            .collect();
        let summary = summary(best);
        match bind_meaning_with_slots(
            meaning_pattern,
            entities,
            normalized_input,
            references,
            profile,
            captured_slots,
        ) {
            BindOutcome::Resolved(meaning) => StructuralDecision::Resolved { meaning, summary },
            BindOutcome::Partial(partial) => StructuralDecision::Partial { partial, summary },
            BindOutcome::Ambiguous(reason_code) => StructuralDecision::Ambiguous {
                candidates: vec![meaning_pattern.id.clone()],
                reason_code,
                summary: Some(summary),
            },
            BindOutcome::Invalid(reason_code) => StructuralDecision::Invalid {
                reason_code,
                summary,
            },
        }
    }
}

fn summary(value: &RuleMatch) -> StructuralMatchSummary {
    StructuralMatchSummary {
        meaning: value.meaning_id.clone(),
        pattern: value.pattern_text.clone(),
        language: value.language.clone(),
        priority: value.rule_priority,
        captures: value.captures.clone(),
    }
}

fn specificity_key(
    value: &RuleMatch,
) -> (
    usize,
    usize,
    usize,
    std::cmp::Reverse<usize>,
    std::cmp::Reverse<usize>,
    i32,
    i32,
) {
    (
        value.literal_atoms,
        value.set_atoms,
        value.set_tokens,
        std::cmp::Reverse(value.wildcard_atoms),
        std::cmp::Reverse(value.wildcard_tokens),
        value.rule_priority,
        value.meaning_priority,
    )
}

fn compare_rule_match(left: &RuleMatch, right: &RuleMatch) -> std::cmp::Ordering {
    specificity_key(left)
        .cmp(&specificity_key(right))
        .then_with(|| right.meaning_id.cmp(&left.meaning_id))
        .then_with(|| right.rule_index.cmp(&left.rule_index))
}

fn match_parsed(
    pattern: &ParsedPattern,
    input_tokens: &[InputToken],
    sets: &BTreeMap<String, Vec<CompiledSetAlias>>,
    budget: &mut MatchBudget,
) -> Result<Option<PathResult>, ()> {
    let mut memo = BTreeMap::new();
    match_from(pattern, 0, input_tokens, 0, sets, budget, &mut memo)
}

fn match_from(
    pattern: &ParsedPattern,
    atom_index: usize,
    input: &[InputToken],
    input_index: usize,
    sets: &BTreeMap<String, Vec<CompiledSetAlias>>,
    budget: &mut MatchBudget,
    memo: &mut BTreeMap<(usize, usize), Option<PathResult>>,
) -> Result<Option<PathResult>, ()> {
    budget.spend()?;
    if let Some(cached) = memo.get(&(atom_index, input_index)) {
        return Ok(cached.clone());
    }
    let result = if atom_index == pattern.atoms.len() {
        (input_index == input.len()).then(PathResult::default)
    } else {
        match &pattern.atoms[atom_index] {
            Atom::Literal(expected) => {
                if input
                    .get(input_index)
                    .is_some_and(|actual| actual.canonical == *expected)
                {
                    match_from(
                        pattern,
                        atom_index + 1,
                        input,
                        input_index + 1,
                        sets,
                        budget,
                        memo,
                    )?
                } else {
                    None
                }
            }
            Atom::Set { name, capture } => {
                let mut best: Option<PathResult> = None;
                for candidate in set_candidates(sets, name, input, input_index, budget)? {
                    let Some(mut suffix) = match_from(
                        pattern,
                        atom_index + 1,
                        input,
                        input_index + candidate.consumed,
                        sets,
                        budget,
                        memo,
                    )?
                    else {
                        continue;
                    };
                    suffix.set_tokens = suffix.set_tokens.saturating_add(candidate.consumed);
                    if let Some(capture) = capture {
                        suffix.captures.insert(capture.clone(), candidate.canonical);
                    }
                    consider_path(&mut best, suffix);
                }
                best
            }
            Atom::Wildcard { min, capture } => {
                let remaining = input.len().saturating_sub(input_index);
                let mut best: Option<PathResult> = None;
                for consumed in *min..=remaining {
                    budget.spend()?;
                    let Some(mut suffix) = match_from(
                        pattern,
                        atom_index + 1,
                        input,
                        input_index + consumed,
                        sets,
                        budget,
                        memo,
                    )?
                    else {
                        continue;
                    };
                    suffix.wildcard_tokens = suffix.wildcard_tokens.saturating_add(consumed);
                    if let Some(capture) = capture {
                        suffix.captures.insert(
                            capture.clone(),
                            input[input_index..input_index + consumed]
                                .iter()
                                .map(|token| token.surface.as_str())
                                .collect::<Vec<_>>()
                                .join(" "),
                        );
                    }
                    consider_path(&mut best, suffix);
                }
                best
            }
        }
    };
    memo.insert((atom_index, input_index), result.clone());
    Ok(result)
}

fn path_quality(value: &PathResult) -> (usize, std::cmp::Reverse<usize>) {
    (value.set_tokens, std::cmp::Reverse(value.wildcard_tokens))
}

fn consider_path(best: &mut Option<PathResult>, candidate: PathResult) {
    let Some(current) = best.as_mut() else {
        *best = Some(candidate);
        return;
    };
    match path_quality(&candidate).cmp(&path_quality(current)) {
        std::cmp::Ordering::Greater => *current = candidate,
        std::cmp::Ordering::Equal => {
            current.captures_ambiguous = current.captures_ambiguous
                || candidate.captures_ambiguous
                || current.captures != candidate.captures;
        }
        std::cmp::Ordering::Less => {}
    }
}

fn set_candidates(
    sets: &BTreeMap<String, Vec<CompiledSetAlias>>,
    name: &str,
    input: &[InputToken],
    input_index: usize,
    budget: &mut MatchBudget,
) -> Result<Vec<SetCandidate>, ()> {
    let Some(set) = sets.get(name) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for alias in set {
        budget.spend()?;
        if input_index + alias.tokens.len() > input.len() {
            continue;
        }
        if alias
            .tokens
            .iter()
            .enumerate()
            .all(|(offset, expected)| input[input_index + offset].canonical == *expected)
        {
            out.push(SetCandidate {
                consumed: alias.tokens.len(),
                canonical: alias.canonical.clone(),
            });
        }
    }
    Ok(out)
}

fn canonical_token_from_normalized(profile: &SemanticProfile, token: &str) -> String {
    profile
        .canonical_tokens
        .get(token)
        .cloned()
        .unwrap_or_else(|| token.to_string())
}

fn canonical_ordered_tokens(profile: &SemanticProfile, text: &str) -> Vec<String> {
    ordered_tokens(&profile.normalize_text(text))
        .into_iter()
        .map(|token| canonical_token_from_normalized(profile, &token))
        .collect()
}

fn structural_input_tokens(profile: &SemanticProfile, normalized_input: &str) -> Vec<InputToken> {
    ordered_tokens(normalized_input)
        .into_iter()
        .map(|surface| InputToken {
            canonical: canonical_token_from_normalized(profile, &surface),
            surface,
        })
        .collect()
}

fn canonicalize_pattern_literals(
    parsed: ParsedPattern,
    profile: &SemanticProfile,
) -> Result<ParsedPattern, StructuralPatternError> {
    let mut atoms = Vec::new();
    let mut literal_atoms = 0usize;
    let mut set_atoms = 0usize;
    let mut wildcard_atoms = 0usize;
    for atom in parsed.atoms {
        match atom {
            Atom::Literal(raw) => {
                let tokens = canonical_ordered_tokens(profile, &raw);
                if tokens.is_empty() {
                    return Err(StructuralPatternError::LiteralNormalizesEmpty(raw));
                }
                for token in tokens {
                    atoms.push(Atom::Literal(token));
                    literal_atoms += 1;
                }
            }
            Atom::Set { name, capture } => {
                atoms.push(Atom::Set { name, capture });
                set_atoms += 1;
            }
            Atom::Wildcard { min, capture } => {
                atoms.push(Atom::Wildcard { min, capture });
                wildcard_atoms += 1;
            }
        }
        if atoms.len() > SEMANTIC_PATTERN_ATOMS_MAX {
            return Err(StructuralPatternError::TooManyAtoms);
        }
    }
    if literal_atoms == 0 && set_atoms == 0 {
        return Err(StructuralPatternError::NoAnchor);
    }
    Ok(ParsedPattern {
        atoms,
        literal_atoms,
        set_atoms,
        wildcard_atoms,
    })
}

fn compile_pattern_set(
    profile: &SemanticProfile,
    name: &str,
) -> Result<Vec<CompiledSetAlias>, StructuralMatcherBuildError> {
    let mut by_tokens: BTreeMap<Vec<String>, (String, String)> = BTreeMap::new();
    if let Some(source) = profile.pattern_sets.get(name) {
        for (alias, canonical) in source {
            insert_compiled_set_alias(&mut by_tokens, profile, name, alias, canonical, false)?;
        }
    } else if let Some(kind) = custom_entity_kind(name) {
        if let Some(values) = profile.custom_entities.get(kind) {
            for (canonical, aliases) in values {
                insert_compiled_set_alias(
                    &mut by_tokens,
                    profile,
                    name,
                    canonical,
                    canonical,
                    true,
                )?;
                for alias in aliases {
                    insert_compiled_set_alias(
                        &mut by_tokens,
                        profile,
                        name,
                        alias,
                        canonical,
                        true,
                    )?;
                }
            }
        }
    }
    let mut out: Vec<_> = by_tokens
        .into_iter()
        .map(|(tokens, (canonical, _))| CompiledSetAlias { tokens, canonical })
        .collect();
    out.sort_by(|left, right| {
        right
            .tokens
            .len()
            .cmp(&left.tokens.len())
            .then_with(|| left.tokens.cmp(&right.tokens))
            .then_with(|| left.canonical.cmp(&right.canonical))
    });
    Ok(out)
}

fn insert_compiled_set_alias(
    by_tokens: &mut BTreeMap<Vec<String>, (String, String)>,
    profile: &SemanticProfile,
    set_name: &str,
    alias: &str,
    canonical: &str,
    custom_entity: bool,
) -> Result<(), StructuralMatcherBuildError> {
    if canonical.trim().is_empty() {
        return Err(StructuralMatcherBuildError::InvalidSetCanonical {
            set: set_name.to_string(),
            alias: alias.to_string(),
        });
    }
    let tokens = if custom_entity {
        custom_entity_ordered_tokens(profile, alias)
    } else {
        canonical_ordered_tokens(profile, alias)
    };
    if tokens.is_empty() {
        return Err(StructuralMatcherBuildError::InvalidSetAlias {
            set: set_name.to_string(),
            alias: alias.to_string(),
        });
    }
    match by_tokens.get(&tokens) {
        Some((existing, _)) if existing != canonical => {
            return Err(StructuralMatcherBuildError::ConflictingSetAlias {
                set: set_name.to_string(),
                alias: alias.to_string(),
            });
        }
        Some(_) => {}
        None => {
            by_tokens.insert(tokens, (canonical.to_string(), alias.to_string()));
        }
    }
    Ok(())
}

fn custom_entity_kind(set_name: &str) -> Option<&str> {
    set_name
        .strip_prefix("entity.")
        .filter(|kind| !kind.is_empty())
}

fn custom_entity_ordered_tokens(profile: &SemanticProfile, text: &str) -> Vec<String> {
    profile
        .normalize_colloquial_tokens(&ordered_tokens(&profile.normalize_text(text)))
        .into_iter()
        .map(|token| canonical_token_from_normalized(profile, &token))
        .collect()
}

fn validate_pattern_capture_sets(
    meaning: &MeaningPattern,
    parsed: &ParsedPattern,
    profile: &SemanticProfile,
) -> Result<(), StructuralPatternError> {
    let slots: BTreeMap<_, _> = meaning
        .slots
        .iter()
        .map(|slot| (slot.name.as_str(), &slot.kind))
        .collect();
    for atom in &parsed.atoms {
        let Atom::Set {
            name,
            capture: Some(capture),
        } = atom
        else {
            continue;
        };
        let Some(kind) = slots.get(capture.as_str()) else {
            return Err(StructuralPatternError::CaptureSlotMissing(capture.clone()));
        };
        if let SlotKind::Entity(entity_kind) = kind {
            if custom_entity_kind(name) != Some(entity_kind.as_str())
                || !profile.custom_entities.contains_key(entity_kind.as_str())
            {
                return Err(StructuralPatternError::CaptureSlotSetKindMismatch(
                    capture.clone(),
                ));
            }
        }
    }
    Ok(())
}

fn parse_pattern(text: &str) -> Result<ParsedPattern, StructuralPatternError> {
    let raw = text.trim();
    if raw.is_empty() {
        return Err(StructuralPatternError::EmptyPattern);
    }
    let mut atoms = Vec::new();
    let mut captures = BTreeSet::new();
    let mut literal_atoms = 0usize;
    let mut set_atoms = 0usize;
    let mut wildcard_atoms = 0usize;

    for token in raw.split_whitespace() {
        let atom = if token == "*" {
            wildcard_atoms += 1;
            Atom::Wildcard {
                min: 1,
                capture: None,
            }
        } else if token == "^" {
            wildcard_atoms += 1;
            Atom::Wildcard {
                min: 0,
                capture: None,
            }
        } else if let Some(capture) = parse_wrapped(token, "*{", "}") {
            validate_capture_name(capture)?;
            register_capture(capture, &mut captures)?;
            wildcard_atoms += 1;
            Atom::Wildcard {
                min: 1,
                capture: Some(capture.to_string()),
            }
        } else if let Some(capture) = parse_wrapped(token, "^{", "}") {
            validate_capture_name(capture)?;
            register_capture(capture, &mut captures)?;
            wildcard_atoms += 1;
            Atom::Wildcard {
                min: 0,
                capture: Some(capture.to_string()),
            }
        } else if token.starts_with("<set:") {
            let (name, capture) = parse_set_atom(token)?;
            if let Some(capture) = &capture {
                register_capture(capture, &mut captures)?;
            }
            set_atoms += 1;
            Atom::Set { name, capture }
        } else {
            literal_atoms += 1;
            Atom::Literal(token.to_string())
        };
        atoms.push(atom);
        if atoms.len() > SEMANTIC_PATTERN_ATOMS_MAX {
            return Err(StructuralPatternError::TooManyAtoms);
        }
        if captures.len() > SEMANTIC_PATTERN_CAPTURES_MAX {
            return Err(StructuralPatternError::TooManyCaptures);
        }
    }

    if literal_atoms == 0 && set_atoms == 0 {
        return Err(StructuralPatternError::NoAnchor);
    }
    Ok(ParsedPattern {
        atoms,
        literal_atoms,
        set_atoms,
        wildcard_atoms,
    })
}

fn parse_set_atom(token: &str) -> Result<(String, Option<String>), StructuralPatternError> {
    let Some(close) = token.find('>') else {
        return Err(StructuralPatternError::InvalidSetName(token.to_string()));
    };
    if !token.starts_with("<set:") || close <= 5 {
        return Err(StructuralPatternError::InvalidSetName(token.to_string()));
    }
    let name = &token[5..close];
    validate_identifier(name).map_err(|_| StructuralPatternError::InvalidSetName(name.into()))?;
    let suffix = &token[close + 1..];
    let capture = if suffix.is_empty() {
        None
    } else if let Some(value) = parse_wrapped(suffix, "{", "}") {
        validate_capture_name(value)?;
        Some(value.to_string())
    } else {
        return Err(StructuralPatternError::InvalidSetName(token.to_string()));
    };
    Ok((name.to_string(), capture))
}

fn atom_capture(atom: &Atom) -> Option<String> {
    match atom {
        Atom::Wildcard { capture, .. } | Atom::Set { capture, .. } => capture.clone(),
        Atom::Literal(_) => None,
    }
}

fn parse_wrapped<'a>(value: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    value
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_suffix(suffix))
        .filter(|inner| !inner.is_empty())
}

fn register_capture(
    capture: &str,
    captures: &mut BTreeSet<String>,
) -> Result<(), StructuralPatternError> {
    if captures.insert(capture.to_string()) {
        Ok(())
    } else {
        Err(StructuralPatternError::DuplicateCapture(
            capture.to_string(),
        ))
    }
}

fn validate_capture_name(value: &str) -> Result<(), StructuralPatternError> {
    validate_identifier(value)
        .map_err(|_| StructuralPatternError::InvalidCaptureName(value.to_string()))
}

fn validate_identifier(value: &str) -> Result<(), ()> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(());
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(());
    }
    if chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.')) {
        Ok(())
    } else {
        Err(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::{ElicitationPrompt, LocalizedSample, ReferenceSpec, SlotSpec};

    fn test_profiles(profile: SemanticProfile) -> SemanticProfiles {
        BTreeMap::from([("en".to_owned(), profile)])
    }

    fn meaning(id: &str, rule: &str) -> MeaningPattern {
        MeaningPattern {
            id: MeaningId::new(id),
            class: super::super::MeaningClass::General,
            patterns: vec![LocalizedStructuralPattern::new("en", rule)],
            samples: vec![LocalizedSample::new("en", "semantic sample")],
            negative_samples: vec![],
            retrieval_terms: vec![],
            priority: 1,
            positive_assumption: false,
            slots: vec![],
            references: Vec::<ReferenceSpec>::new(),
        }
    }

    fn match_rule(
        rule: &str,
        input: &str,
        profile: &SemanticProfile,
        sets: &BTreeMap<String, Vec<CompiledSetAlias>>,
    ) -> Option<PathResult> {
        let pattern = canonicalize_pattern_literals(parse_pattern(rule).unwrap(), profile).unwrap();
        let normalized = profile.normalize_text(input);
        let tokens = structural_input_tokens(profile, &normalized);
        let mut budget = MatchBudget::new(10_000);
        match_parsed(&pattern, &tokens, sets, &mut budget).unwrap()
    }

    #[test]
    fn aiml_style_wildcards_are_anchored_and_ordered() {
        let profile = SemanticProfile::empty();
        let sets = BTreeMap::new();
        assert!(
            match_rule(
                "^ capability * runtime",
                "how does capability reach runtime",
                &profile,
                &sets,
            )
            .is_some()
        );
        assert!(
            match_rule(
                "^ capability * runtime",
                "runtime capability",
                &profile,
                &sets
            )
            .is_none()
        );
        assert!(
            match_rule(
                "capability * runtime",
                "capability runtime",
                &profile,
                &sets
            )
            .is_none()
        );
        assert!(
            match_rule(
                "capability ^ runtime",
                "capability runtime",
                &profile,
                &sets
            )
            .is_some()
        );
    }

    #[test]
    fn captures_and_profile_sets_are_deterministic() {
        let mut profile = SemanticProfile::empty();
        profile.pattern_sets.insert(
            "devices".into(),
            BTreeMap::from([("bedroom light".into(), "light.bedroom".into())]),
        );
        let sets = BTreeMap::from([(
            "devices".to_string(),
            compile_pattern_set(&profile, "devices").unwrap(),
        )]);
        let result = match_rule(
            "turn on <set:devices>{device}",
            "turn on bedroom light",
            &profile,
            &sets,
        )
        .unwrap();
        assert_eq!(
            result.captures.get("device"),
            Some(&"light.bedroom".to_string())
        );
    }

    #[test]
    fn wildcard_capture_preserves_normalized_surface_instead_of_canonical_alias() {
        let mut profile = SemanticProfile::empty();
        profile
            .canonical_tokens
            .insert("puppy".into(), "dog".into());
        let result = match_rule(
            "search for *{query}",
            "SEARCH for Puppy",
            &profile,
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(result.captures.get("query"), Some(&"puppy".to_string()));
    }

    #[test]
    fn structural_specificity_beats_broader_rule() {
        let profile = SemanticProfile::empty();
        let broad = meaning("broad", "^ capability ^");
        let specific = meaning("specific", "^ capability * runtime");
        let catalog = vec![broad, specific];
        let matcher = StructuralMatcher::build(&catalog, &test_profiles(profile.clone())).unwrap();
        let decision = matcher.resolve(
            &catalog,
            &profile,
            "how does capability reach runtime",
            &[],
            &[],
            Some("en-US"),
            &[],
            |_| true,
        );
        match decision {
            StructuralDecision::Resolved { meaning, .. } => {
                assert_eq!(meaning.id.as_str(), "specific")
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }

    #[test]
    fn capture_must_target_declared_string_slot() {
        let mut pattern = meaning("search", "search for *{query}");
        assert!(matches!(
            validate_pattern(&pattern, &pattern.patterns[0]),
            Err(StructuralPatternError::CaptureSlotMissing(_))
        ));
        pattern.slots.push(SlotSpec {
            name: "query".into(),
            kind: SlotKind::String,
            required: true,
            elicitation: vec![ElicitationPrompt::new("en", "What query?")],
        });
        assert!(validate_pattern(&pattern, &pattern.patterns[0]).is_ok());
    }

    #[test]
    fn literals_follow_profile_normalization_before_matching() {
        let profile = SemanticProfile::empty();
        let catalog = vec![meaning("voice", "VOICE-CONTROLLED DEVICE")];
        let matcher = StructuralMatcher::build(&catalog, &test_profiles(profile.clone())).unwrap();
        let decision = matcher.resolve(
            &catalog,
            &profile,
            "voice controlled device",
            &[],
            &[],
            Some("en-US"),
            &[],
            |_| true,
        );
        assert!(matches!(decision, StructuralDecision::Resolved { .. }));
    }

    #[test]
    fn equal_quality_capture_partitions_fail_as_ambiguous() {
        let profile = SemanticProfile::empty();
        let mut pattern = meaning("ambiguous", "*{left} x *{right}");
        pattern.slots = vec![
            SlotSpec {
                name: "left".into(),
                kind: SlotKind::String,
                required: true,
                elicitation: vec![ElicitationPrompt::new("en", "What left value?")],
            },
            SlotSpec {
                name: "right".into(),
                kind: SlotKind::String,
                required: true,
                elicitation: vec![ElicitationPrompt::new("en", "What right value?")],
            },
        ];
        let catalog = vec![pattern];
        let matcher = StructuralMatcher::build(&catalog, &test_profiles(profile.clone())).unwrap();
        let decision = matcher.resolve(
            &catalog,
            &profile,
            "a x b x c",
            &[],
            &[],
            Some("en-US"),
            &[],
            |_| true,
        );
        assert!(matches!(
            decision,
            StructuralDecision::Ambiguous { ref reason_code, .. }
                if reason_code == "structural_captures_tied"
        ));
    }

    #[test]
    fn normalized_set_alias_collision_with_different_values_is_rejected() {
        let mut profile = SemanticProfile::empty();
        profile.pattern_sets.insert(
            "devices".into(),
            BTreeMap::from([
                ("bedroom-light".into(), "device.one".into()),
                ("bedroom light".into(), "device.two".into()),
            ]),
        );
        let catalog = vec![meaning("device", "<set:devices>")];
        assert!(matches!(
            StructuralMatcher::build(&catalog, &test_profiles(profile.clone())),
            Err(StructuralMatcherBuildError::ConflictingSetAlias { .. })
        ));
    }

    #[test]
    fn structural_work_budget_is_hard_and_fail_closed_at_match_boundary() {
        let profile = SemanticProfile::empty();
        let pattern =
            canonicalize_pattern_literals(parse_pattern("^ x ^").unwrap(), &profile).unwrap();
        let normalized = profile.normalize_text("a b c x d e f");
        let input = structural_input_tokens(&profile, &normalized);
        let mut budget = MatchBudget::new(1);
        assert!(match_parsed(&pattern, &input, &BTreeMap::new(), &mut budget).is_err());
    }
}
