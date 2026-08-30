//! Semantic kernel behavior tests.

use super::*;
use gvya_model::ReferenceId;

fn test_profiles(profile: SemanticProfile) -> SemanticProfiles {
    BTreeMap::from([
        ("und".to_owned(), profile.clone()),
        ("en".to_owned(), profile.clone()),
        ("en-us".to_owned(), profile.clone()),
        ("fa".to_owned(), profile.clone()),
        ("fa-ir".to_owned(), profile),
    ])
}

fn kernel(patterns: Vec<MeaningPattern>) -> SemanticKernel {
    SemanticKernel::new(
        SemanticCatalog::new(patterns).unwrap(),
        test_profiles(SemanticProfile::empty()),
        SemanticConfig::default(),
    )
    .unwrap()
}

#[test]
fn pareto_evidence_can_promote_a_near_tie_only_when_both_axes_dominate() {
    fn row(id: &str, score: f64, strength: f64, rank: u64) -> ScoredMeaning {
        ScoredMeaning {
            meaning: MeaningId::new(id),
            pattern_index: 0,
            score,
            priority: 0,
            retrieval_rank_milli: rank,
            breakdown: ScoreBreakdown {
                evidence_tier: 4,
                evidence_strength: strength,
                no_evidence: false,
                ..ScoreBreakdown::default()
            },
        }
    }

    let mut rows = vec![
        row("raw", 0.401, 0.256, 137_000),
        row("evidence", 0.393, 0.497, 695_000),
    ];
    SemanticKernel::promote_pareto_evidence(&mut rows);
    assert_eq!(rows[0].meaning.as_str(), "evidence");

    let mut strength_only = vec![
        row("raw", 0.401, 0.256, 400_000),
        row("candidate", 0.393, 0.497, 500_000),
    ];
    SemanticKernel::promote_pareto_evidence(&mut strength_only);
    assert_eq!(strength_only[0].meaning.as_str(), "raw");

    let mut too_far = vec![
        row("raw", 0.401, 0.256, 137_000),
        row("candidate", 0.360, 0.600, 800_000),
    ];
    SemanticKernel::promote_pareto_evidence(&mut too_far);
    assert_eq!(too_far[0].meaning.as_str(), "raw");
}

#[test]
fn unanimous_evidence_can_break_only_a_genuine_close_score_ambiguity() {
    fn row(id: &str, score: f64, strength: f64, rank: u64) -> ScoredMeaning {
        ScoredMeaning {
            meaning: MeaningId::new(id),
            pattern_index: 0,
            score,
            priority: 0,
            retrieval_rank_milli: rank,
            breakdown: ScoreBreakdown {
                evidence_tier: 3,
                evidence_strength: strength,
                no_evidence: false,
                ..ScoreBreakdown::default()
            },
        }
    }

    let best = row("best", 0.547, 0.554, 161_000);
    let second = row("second", 0.525, 0.514, 118_000);
    assert!(SemanticKernel::evidence_consensus_breaks_ambiguity(
        &best, &second
    ));

    let rank_disagrees = row("second", 0.525, 0.514, 170_000);
    assert!(!SemanticKernel::evidence_consensus_breaks_ambiguity(
        &best,
        &rank_disagrees
    ));

    let strength_is_too_close = row("second", 0.525, 0.545, 118_000);
    assert!(!SemanticKernel::evidence_consensus_breaks_ambiguity(
        &best,
        &strength_is_too_close
    ));

    let score_is_too_close = row("second", 0.540, 0.514, 118_000);
    assert!(!SemanticKernel::evidence_consensus_breaks_ambiguity(
        &best,
        &score_is_too_close
    ));
}

#[test]
fn exceptional_retrieval_can_break_saturated_same_tier_ambiguity() {
    fn row(
        id: &str,
        score: f64,
        tier: u8,
        strength: f64,
        rank: u64,
        negative_penalty: f64,
    ) -> ScoredMeaning {
        ScoredMeaning {
            meaning: MeaningId::new(id),
            pattern_index: 0,
            score,
            priority: 0,
            retrieval_rank_milli: rank,
            breakdown: ScoreBreakdown {
                evidence_tier: tier,
                evidence_strength: strength,
                negative_penalty,
                no_evidence: false,
                ..ScoreBreakdown::default()
            },
        }
    }

    let source_layout = vec![
        row("best", 0.8185, 2, 1.0, 882_473, 0.0),
        row("runner", 0.7900, 2, 1.0, 174_046, 0.0),
        row("other", 0.5808, 3, 0.61, 641_042, 0.0),
    ];
    assert!(SemanticKernel::exceptional_retrieval_breaks_ambiguity(
        &source_layout
    ));

    let interaction = vec![
        row("best", 0.6330, 3, 0.6746, 623_450, 0.0),
        row("runner", 0.6293, 3, 0.7006, 123_110, 0.0),
        row("other", 0.5896, 3, 0.6356, 466_967, 0.0),
    ];
    assert!(SemanticKernel::exceptional_retrieval_breaks_ambiguity(
        &interaction
    ));

    let requires_forbidden = vec![
        row("best", 0.6621, 3, 0.7274, 677_906, 0.0),
        row("runner", 0.6425, 3, 0.7173, 176_779, 0.0),
        row("other", 0.5805, 3, 0.6190, 253_532, 0.0),
    ];
    assert!(SemanticKernel::exceptional_retrieval_breaks_ambiguity(
        &requires_forbidden
    ));
}

#[test]
fn exceptional_retrieval_ambiguity_breaker_refuses_weak_close_or_negative_evidence() {
    fn row(
        id: &str,
        score: f64,
        tier: u8,
        strength: f64,
        rank: u64,
        negative_penalty: f64,
    ) -> ScoredMeaning {
        ScoredMeaning {
            meaning: MeaningId::new(id),
            pattern_index: 0,
            score,
            priority: 0,
            retrieval_rank_milli: rank,
            breakdown: ScoreBreakdown {
                evidence_tier: tier,
                evidence_strength: strength,
                negative_penalty,
                no_evidence: false,
                ..ScoreBreakdown::default()
            },
        }
    }

    assert!(!SemanticKernel::exceptional_retrieval_breaks_ambiguity(&[
        row("best", 0.63, 3, 0.70, 590_000, 0.0),
        row("runner", 0.62, 3, 0.68, 100_000, 0.0),
    ]));
    assert!(!SemanticKernel::exceptional_retrieval_breaks_ambiguity(&[
        row("best", 0.63, 3, 0.70, 700_000, 0.0),
        row("runner", 0.62, 3, 0.68, 200_000, 0.0),
        row("other", 0.55, 4, 0.55, 590_000, 0.0),
    ]));
    assert!(!SemanticKernel::exceptional_retrieval_breaks_ambiguity(&[
        row("best", 0.63, 3, 0.70, 700_000, 0.0),
        row("runner", 0.62, 3, 0.68, 250_000, 0.0),
    ]));
    assert!(!SemanticKernel::exceptional_retrieval_breaks_ambiguity(&[
        row("best", 0.63, 3, 0.70, 900_000, 0.35),
        row("runner", 0.62, 3, 0.68, 150_000, 0.0),
    ]));
    assert!(!SemanticKernel::exceptional_retrieval_breaks_ambiguity(&[
        row("best", 0.63, 3, 0.64, 900_000, 0.0),
        row("runner", 0.62, 3, 0.63, 150_000, 0.0),
    ]));
}

#[test]
fn resolves_exact_meaning_and_binds_number_entity_slot() {
    let mut p = MeaningPattern::new(
        "temperature.set",
        ["set temperature to 22", "temperature 22"],
    );
    p.slots.push(SlotSpec {
        name: "temperature".into(),
        kind: SlotKind::Number,
        required: true,
        elicitation: vec![ElicitationPrompt::new("en", "What temperature?")],
    });
    let analysis = kernel(vec![p]).analyze(&SemanticInput::utterance("set temperature to 22"));
    let SemanticDecision::Resolved { meaning, .. } = analysis.decision else {
        panic!("expected resolved");
    };
    assert_eq!(meaning.id.as_str(), "temperature.set");
    assert_eq!(meaning.slots[0].value, Value::Number(22.0));
}

#[test]
fn close_top_candidates_are_explicitly_ambiguous() {
    let p1 = MeaningPattern::new("light.one", ["turn on light"]);
    let p2 = MeaningPattern::new("light.two", ["turn on light"]);
    let analysis = kernel(vec![p1, p2]).analyze(&SemanticInput::utterance("turn on light"));
    assert!(matches!(
        analysis.decision,
        SemanticDecision::Ambiguous { .. }
    ));
}

fn lexical_rescue_test_kernel(class: MeaningClass) -> SemanticKernel {
    let mut pattern = MeaningPattern::new("candidate", ["candidate sample"]);
    pattern.class = class;
    SemanticKernel::new(
        SemanticCatalog::new(vec![pattern]).unwrap(),
        test_profiles(SemanticProfile::empty()),
        SemanticConfig::default(),
    )
    .unwrap()
}

#[test]
fn lexical_retrieval_dominance_can_rescue_near_floor_candidate() {
    fn row(id: &str, score: f64, strength: f64, rank: u64, retrieval_rescue: f64) -> ScoredMeaning {
        ScoredMeaning {
            meaning: MeaningId::new(id),
            pattern_index: 0,
            score,
            priority: 1,
            retrieval_rank_milli: rank,
            breakdown: ScoreBreakdown {
                evidence_tier: 4,
                evidence_strength: strength,
                retrieval_rescue,
                no_evidence: false,
                ..ScoreBreakdown::default()
            },
        }
    }
    let rows = [
        row("best", 0.38, 0.40, 520_000, 0.52),
        row("runner", 0.37, 0.40, 430_000, 0.0),
    ];
    let kernel = lexical_rescue_test_kernel(MeaningClass::General);
    assert!(kernel.retrieval_dominance_rescue(&rows, 0.45));
}

#[test]
fn exceptional_retrieval_dominance_can_rescue_a_deeper_but_still_bounded_candidate() {
    fn row(score: f64, strength: f64, rank: u64, retrieval_rescue: f64) -> ScoredMeaning {
        ScoredMeaning {
            meaning: MeaningId::new("candidate"),
            pattern_index: 0,
            score,
            priority: 1,
            retrieval_rank_milli: rank,
            breakdown: ScoreBreakdown {
                evidence_tier: 4,
                evidence_strength: strength,
                retrieval_rescue,
                no_evidence: false,
                ..ScoreBreakdown::default()
            },
        }
    }
    let kernel = lexical_rescue_test_kernel(MeaningClass::General);
    let mut rows = vec![
        row(0.313, 0.35, 420_000, 0.36),
        row(0.305, 0.34, 175_000, 0.0),
    ];
    kernel.sort_scores(&mut rows);
    assert_eq!(rows[0].score, 0.313);
    assert!(kernel.retrieval_dominance_rescue(&rows, 0.45));
    assert!(!kernel.retrieval_dominance_rescue(
        &[
            row(0.366, 0.45, 720_000, 0.52),
            row(0.345, 0.41, 720_000, 0.0)
        ],
        0.45
    ));
}

#[test]
fn strong_authored_metadata_can_rescue_low_sample_strength() {
    fn row(score: f64, strength: f64, rank: u64, retrieval_rescue: f64) -> ScoredMeaning {
        ScoredMeaning {
            meaning: MeaningId::new("candidate"),
            pattern_index: 0,
            score,
            priority: 1,
            retrieval_rank_milli: rank,
            breakdown: ScoreBreakdown {
                evidence_tier: 4,
                evidence_strength: strength,
                retrieval_rescue,
                no_evidence: false,
                ..ScoreBreakdown::default()
            },
        }
    }
    let kernel = lexical_rescue_test_kernel(MeaningClass::General);
    assert!(kernel.retrieval_dominance_rescue(
        &[
            row(0.34, 0.24, 600_000, 0.58),
            row(0.33, 0.40, 200_000, 0.0)
        ],
        0.45
    ));
}

#[test]
fn lexical_retrieval_rescue_refuses_close_or_weak_evidence() {
    fn row(score: f64, strength: f64, rank: u64, retrieval_rescue: f64) -> ScoredMeaning {
        ScoredMeaning {
            meaning: MeaningId::new("candidate"),
            pattern_index: 0,
            score,
            priority: 1,
            retrieval_rank_milli: rank,
            breakdown: ScoreBreakdown {
                evidence_tier: 4,
                evidence_strength: strength,
                retrieval_rescue,
                no_evidence: false,
                ..ScoreBreakdown::default()
            },
        }
    }
    let kernel = lexical_rescue_test_kernel(MeaningClass::General);
    assert!(!kernel.retrieval_dominance_rescue(
        &[
            row(0.38, 0.40, 500_000, 0.52),
            row(0.37, 0.40, 470_000, 0.0)
        ],
        0.45
    ));
    assert!(!kernel.retrieval_dominance_rescue(
        &[
            row(0.31, 0.40, 520_000, 0.52),
            row(0.30, 0.40, 430_000, 0.0)
        ],
        0.45
    ));
    assert!(!kernel.retrieval_dominance_rescue(
        &[
            row(0.38, 0.30, 520_000, 0.49),
            row(0.37, 0.30, 430_000, 0.0)
        ],
        0.45
    ));
    assert!(!kernel.retrieval_dominance_rescue(
        &[
            row(0.38, 0.40, 290_000, 0.52),
            row(0.37, 0.40, 100_000, 0.0)
        ],
        0.45
    ));
    let social = lexical_rescue_test_kernel(MeaningClass::Social);
    assert!(!social.retrieval_dominance_rescue(
        &[
            row(0.38, 0.40, 600_000, 0.52),
            row(0.37, 0.40, 100_000, 0.0)
        ],
        0.45
    ));
}

#[test]
fn lexical_retrieval_rescue_requires_explicit_metadata_and_clean_evidence() {
    fn row(
        score: f64,
        strength: f64,
        rank: u64,
        retrieval_rescue: f64,
        negative_penalty: f64,
    ) -> ScoredMeaning {
        ScoredMeaning {
            meaning: MeaningId::new("candidate"),
            pattern_index: 0,
            score,
            priority: 1,
            retrieval_rank_milli: rank,
            breakdown: ScoreBreakdown {
                evidence_tier: 4,
                evidence_strength: strength,
                retrieval_rescue,
                negative_penalty,
                no_evidence: false,
                ..ScoreBreakdown::default()
            },
        }
    }
    let kernel = lexical_rescue_test_kernel(MeaningClass::General);
    let runner = row(0.3229, 0.35, 68_656, 0.0, 0.0);

    assert!(!kernel.retrieval_dominance_rescue(
        &[row(0.3325, 0.3864, 388_509, 0.0, 0.0), runner.clone()],
        0.45
    ));
    assert!(
        !kernel.retrieval_dominance_rescue(&[row(0.38, 0.40, 520_000, 0.52, 0.35), runner], 0.45)
    );
}

#[test]
fn non_exact_clarification_wrapper_yields_to_supported_general_meaning() {
    let mut clarification = MeaningPattern::new("clarify", ["what do you mean"]);
    clarification.class = MeaningClass::Clarification;
    clarification.priority = 10;
    let task = MeaningPattern::new("install", ["install addon"]);
    let analysis = kernel(vec![clarification, task])
        .analyze(&SemanticInput::utterance("what do you mean install addon"));
    let SemanticDecision::Resolved { meaning, .. } = analysis.decision else {
        panic!("expected supported General Meaning to resolve");
    };
    assert_eq!(meaning.id.as_str(), "install");
    let clarification = analysis
        .scored
        .iter()
        .find(|row| {
            row.breakdown.rejected_reason
                == Some("clarification_wrapper_competed_by_general_meaning")
        })
        .expect("clarification row");
    assert_eq!(
        clarification.breakdown.rejected_reason,
        Some("clarification_wrapper_competed_by_general_meaning")
    );
}

#[test]
fn social_wrapper_remains_answerable_without_a_competing_general_meaning() {
    let mut thanks = MeaningPattern::new("thanks", ["thank you"]);
    thanks.class = MeaningClass::Social;
    let analysis =
        kernel(vec![thanks]).analyze(&SemanticInput::utterance("thank you for your help"));
    let SemanticDecision::Resolved { meaning, .. } = analysis.decision else {
        panic!("expected social phrase to remain answerable");
    };
    assert_eq!(meaning.id.as_str(), "thanks");
}

#[test]
fn exact_social_singleton_remains_answerable_inside_a_long_utterance() {
    let mut hello = MeaningPattern::new("hello", ["hi"]);
    hello.class = MeaningClass::Social;
    // Put enough weak token-sharing distractors ahead of `hello` to overflow the bounded
    // candidate frontier. The exact embedded singleton must remain reachable before scoring;
    // otherwise a larger Bot can disagree with the same Package in isolation.
    let mut patterns = (0..40)
        .map(|index| {
            MeaningPattern::new(
                format!("distractor.{index}"),
                [format!("waiting distractor token {index}")],
            )
        })
        .collect::<Vec<_>>();
    patterns.push(hello);
    let mut profile = SemanticProfile::empty();
    profile.generic_singletons.insert("hi".to_owned());
    profile.social_vocabulary.insert("hi".to_owned());
    profile.reporting_verbs.insert("said".to_owned());
    let kernel = SemanticKernel::new(
        SemanticCatalog::new(patterns).unwrap(),
        test_profiles(profile),
        SemanticConfig {
            candidate_limit: 32,
            ..SemanticConfig::default()
        },
    )
    .unwrap();

    let analysis = kernel.analyze(&SemanticInput::utterance(
        "i am here to say hi to you and i know you were waiting for me",
    ));
    let SemanticDecision::Resolved { meaning, .. } = analysis.decision else {
        panic!("expected the exact embedded social singleton to remain answerable");
    };
    assert_eq!(meaning.id.as_str(), "hello");
    assert_eq!(
        analysis.candidate_pruning_reason,
        "specificity_plus_index_bounded"
    );
    let hello = analysis
        .scored
        .iter()
        .find(|row| row.meaning.as_str() == "hello")
        .expect("hello score");
    assert_eq!(hello.breakdown.match_kind, MatchKind::PhraseSpan);
    assert!(hello.breakdown.match_coverage < 1.0);

    let reported = kernel.analyze(&SemanticInput::utterance("he said hi"));
    assert!(matches!(
        reported.decision,
        SemanticDecision::Unresolved { .. }
    ));
    assert!(reported.scored.iter().any(|row| {
        row.meaning.as_str() == "hello"
            && row.breakdown.rejected_reason == Some("reported_speech_social_suppressed")
    }));
}

#[test]
fn exhaustive_sample_rescue_recovers_strong_sample_hidden_by_candidate_limit() {
    let mut patterns = Vec::new();
    for index in 0..40 {
        let mut distractor = MeaningPattern::new(
            format!("retrieval.distractor.{index:02}"),
            ["alpha beta gamma delta"],
        );
        // The exact positive keeps these rows at the front of retrieval, while the authored
        // negative makes them scorer-ineligible. The target is therefore genuinely hidden by
        // candidate_limit and can only become authoritative through exhaustive sample rescue.
        distractor
            .negative_samples
            .push(LocalizedText::new("und", "alpha beta gamma delta"));
        patterns.push(distractor);
    }
    patterns.push(MeaningPattern::new("sample.target", ["alpha gamma"]));
    let kernel = SemanticKernel::new(
        SemanticCatalog::new(patterns).unwrap(),
        test_profiles(SemanticProfile::empty()),
        SemanticConfig {
            candidate_limit: 32,
            ..SemanticConfig::default()
        },
    )
    .unwrap();

    let analysis = kernel.analyze(&SemanticInput::utterance("alpha beta gamma delta"));
    let SemanticDecision::Resolved { meaning, .. } = &analysis.decision else {
        panic!("expected exhaustive sample rescue to recover the hidden strong sample");
    };
    assert_eq!(meaning.id.as_str(), "sample.target");
    assert_eq!(
        analysis.candidate_pruning_reason,
        "exhaustive_sample_rescue"
    );
    assert_eq!(
        analysis
            .scored
            .iter()
            .filter(|row| row.meaning.as_str().starts_with("retrieval.distractor."))
            .count(),
        32,
        "weak hidden distractors discovered by the scan must not be appended to the frontier"
    );
    let target = analysis
        .scored
        .iter()
        .find(|row| row.meaning.as_str() == "sample.target")
        .expect("rescued target score");
    assert!(SemanticKernel::has_decision_grade_sample_evidence(target));
    let retrieval = analysis
        .trace
        .events
        .iter()
        .find(|event| event.code.as_str() == "semantic.candidates.retrieved")
        .expect("candidate retrieval trace");
    assert_eq!(
        retrieval.details.get("exhaustive_sample_scan"),
        Some(&Value::Bool(true))
    );
    assert_eq!(
        retrieval.details.get("exhaustive_sample_rescue"),
        Some(&Value::Bool(true))
    );
}

#[test]
fn exhaustive_sample_rescue_refuses_catalogs_outside_the_pattern_budget() {
    let patterns = (0..=SEMANTIC_EXHAUSTIVE_RESCUE_PATTERNS_MAX)
        .map(|index| MeaningPattern::new(format!("large.{index:04}"), [format!("sample {index}")]))
        .collect::<Vec<_>>();
    let kernel = SemanticKernel::new(
        SemanticCatalog::new(patterns).unwrap(),
        test_profiles(SemanticProfile::empty()),
        SemanticConfig {
            candidate_limit: 32,
            ..SemanticConfig::default()
        },
    )
    .unwrap();
    let views = build_semantic_views("unrelated query", &SemanticProfile::empty(), None);
    assert!(!kernel.exhaustive_sample_rescue_within_budget(&views, &|_| true));
}

#[test]
fn exact_social_singleton_yields_to_a_supported_general_intent() {
    let mut hello = MeaningPattern::new("hello", ["hi"]);
    hello.class = MeaningClass::Social;
    let packages = MeaningPattern::new("packages", ["explain packages"]);
    let mut profile = SemanticProfile::empty();
    profile.generic_singletons.insert("hi".to_owned());
    profile.social_vocabulary.insert("hi".to_owned());
    let kernel = SemanticKernel::new(
        SemanticCatalog::new(vec![hello, packages]).unwrap(),
        test_profiles(profile),
        SemanticConfig::default(),
    )
    .unwrap();

    let analysis = kernel.analyze(&SemanticInput::utterance("hi explain packages"));
    let SemanticDecision::Resolved { meaning, .. } = analysis.decision else {
        panic!("expected the supported General Meaning to resolve");
    };
    assert_eq!(meaning.id.as_str(), "packages");
    assert!(analysis.scored.iter().any(|row| {
        row.meaning.as_str() == "hello"
            && row.breakdown.rejected_reason == Some("social_wrapper_competed_by_general_meaning")
    }));
}

#[test]
fn productive_word_variants_resolve_from_one_generic_sample() {
    let pattern = MeaningPattern::new("install.manager", ["manager install"]);
    let analysis =
        kernel(vec![pattern]).analyze(&SemanticInput::utterance("managerial installation"));
    let SemanticDecision::Resolved { meaning, .. } = analysis.decision else {
        panic!("expected generic bounded morphology to resolve");
    };
    assert_eq!(meaning.id.as_str(), "install.manager");
    assert_eq!(
        analysis.candidate_pruning_reason,
        "specificity_plus_index_bounded"
    );
}

#[test]
fn reference_aliases_bind_typed_host_identity_not_label_as_authority() {
    let mut p = MeaningPattern::new("door.inspect", ["what is maintenance door"]);
    let kind = ReferenceKind::new("door");
    p.references.push(ReferenceSpec {
        kind: kind.clone(),
        required: true,
        elicitation: vec![ElicitationPrompt::new("en", "Which door?")],
    });
    let mut input = SemanticInput::utterance("what is maintenance door");
    input.reference_candidates.push(ResolverReferenceCandidate {
        reference: HostReference {
            kind,
            id: ReferenceId::new("door-17"),
        },
        label: Some("Maintenance Door".into()),
        aliases: vec!["maintenance door".into()],
    });
    let analysis = kernel(vec![p]).analyze(&input);
    let SemanticDecision::Resolved { meaning, .. } = analysis.decision else {
        panic!("expected resolved");
    };
    assert_eq!(meaning.references[0].id.as_str(), "door-17");
}

struct Resolver;
impl SemanticResolver for Resolver {
    type Error = ();
    fn propose(&self, request: &ResolverRequest) -> Result<ResolverProposal, Self::Error> {
        Ok(ResolverProposal {
            meaning: request.candidates.first().map(|row| row.meaning.clone()),
            slots: vec![],
            references: vec![],
            confidence: Some(0.95),
            evidence: vec!["structured resolver".into()],
        })
    }
}

/// Capability identity is not part of the semantic resolver contract at all. There is no field to
/// populate, no request field advertising Capabilities, and no branch that could consume one, so
/// Capability selection is structurally impossible rather than merely ignored.
#[test]
fn resolver_contract_cannot_express_capability_selection_at_all() {
    let p = MeaningPattern::new("weather.ask", ["weather today"]);
    let k = kernel(vec![p]);
    let mut input = SemanticInput::utterance("tell weather");
    input
        .resolver_context
        .insert("safe.locale".into(), Value::String("en".into()));
    let analysis = k.analyze(&input);
    let request = k.resolver_request(&input, &analysis);

    // Structural proof: the exposed request carries no Capability surface of any kind.
    let exposed = format!("{request:?}");
    assert!(!exposed.to_lowercase().contains("capability"), "{exposed}");

    let accepted = k.analyze_with_resolver(&input, &Resolver).unwrap();
    assert!(matches!(
        accepted.decision,
        SemanticDecision::Resolved {
            source: ResolutionSource::ResolverProposal,
            ..
        }
    ));
    let review = accepted
        .trace
        .events
        .iter()
        .find(|event| event.code.as_str() == "semantic.resolver.review")
        .unwrap();
    assert_eq!(review.details.get("accepted"), Some(&Value::Bool(true)));
    assert!(!review.details.contains_key("capability_ignored"));

    // A validated resolver Meaning still carries no Capability: binding stays downstream.
    let SemanticDecision::Resolved { meaning, .. } = accepted.decision else {
        panic!("expected resolved");
    };
    assert_eq!(meaning.id.as_str(), "weather.ask");
    assert!(meaning.slots.is_empty());
    assert!(meaning.references.is_empty());
}

#[test]
fn oversized_custom_resolver_proposal_fails_closed_before_semantic_acceptance() {
    let p = MeaningPattern::new("weather.ask", ["weather today"]);
    let k = kernel(vec![p]);
    let input = SemanticInput::utterance("weather maybe");
    let analysis = k.analyze(&input);
    let request = k.resolver_request(&input, &analysis);
    let proposal = ResolverProposal {
        meaning: request.candidates.first().map(|row| row.meaning.clone()),
        slots: vec![],
        references: vec![],
        confidence: Some(0.95),
        evidence: (0..=RESOLVER_PROPOSAL_MAX_EVIDENCE)
            .map(|index| format!("evidence-{index}"))
            .collect(),
    };
    let review = k.review_resolver_proposal(&input, &analysis, &request, proposal);
    assert!(!review.accepted);
    assert_eq!(review.reason_code, "resolver_proposal_limit_exceeded");
}

#[test]
fn resolver_proposal_aggregate_text_budget_fails_closed() {
    let p = MeaningPattern::new("weather.ask", ["weather today"]);
    let k = kernel(vec![p]);
    let input = SemanticInput::utterance("weather maybe");
    let analysis = k.analyze(&input);
    let request = k.resolver_request(&input, &analysis);
    let proposal = ResolverProposal {
        meaning: request.candidates.first().map(|row| row.meaning.clone()),
        slots: vec![],
        references: vec![],
        confidence: Some(0.95),
        evidence: (0..64).map(|_| "x".repeat(4097)).collect(),
    };
    let review = k.review_resolver_proposal(&input, &analysis, &request, proposal);
    assert!(!review.accepted);
    assert_eq!(review.reason_code, "resolver_proposal_limit_exceeded");
}

#[test]
fn resolver_missing_confidence_fails_closed() {
    let p = MeaningPattern::new("weather.ask", ["weather today"]);
    let k = kernel(vec![p]);
    let input = SemanticInput::utterance("weather maybe");
    let analysis = k.analyze(&input);
    let request = k.resolver_request(&input, &analysis);
    let proposal = ResolverProposal {
        meaning: request.candidates.first().map(|row| row.meaning.clone()),
        slots: vec![],
        references: vec![],
        confidence: None,
        evidence: vec![],
    };
    let review = k.review_resolver_proposal(&input, &analysis, &request, proposal);
    assert!(!review.accepted);
    assert_eq!(review.reason_code, "resolver_missing_confidence");
}

#[test]
fn resolver_non_finite_and_out_of_range_confidence_fails_closed() {
    let p = MeaningPattern::new("weather.ask", ["weather today"]);
    let k = kernel(vec![p]);
    let input = SemanticInput::utterance("weather maybe");
    let analysis = k.analyze(&input);
    let request = k.resolver_request(&input, &analysis);
    let meaning = request.candidates.first().map(|row| row.meaning.clone());
    for confidence in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        let proposal = ResolverProposal {
            meaning: meaning.clone(),
            slots: vec![],
            references: vec![],
            confidence: Some(confidence),
            evidence: vec![],
        };
        let review = k.review_resolver_proposal(&input, &analysis, &request, proposal);
        assert!(!review.accepted);
        assert_eq!(review.reason_code, "resolver_invalid_confidence");
    }
}

fn entity_profile() -> SemanticProfile {
    let mut profile = SemanticProfile::empty();
    profile
        .relative_dates
        .insert("tomorrow".into(), "tomorrow".into());
    profile.colors.insert("crimson".into(), "red".into());
    profile.units.insert("kg".into(), "kilogram".into());
    profile
}

fn entity_slot_kernel(slot: &str, kind: SlotKind, profile: SemanticProfile) -> SemanticKernel {
    let mut pattern = MeaningPattern::new("trip.plan", ["plan trip"]);
    pattern.slots.push(SlotSpec {
        name: slot.into(),
        kind,
        required: true,
        elicitation: vec![ElicitationPrompt::new("en", "When?")],
    });
    SemanticKernel::new(
        SemanticCatalog::new(vec![pattern]).unwrap(),
        test_profiles(profile),
        SemanticConfig::default(),
    )
    .unwrap()
}

fn slot_proposal(meaning: &MeaningId, name: &str, value: Value) -> ResolverProposal {
    ResolverProposal {
        meaning: Some(meaning.clone()),
        slots: vec![SlotValue {
            name: name.into(),
            value,
            provenance: ValueProvenance::NeuralProposal,
        }],
        references: vec![],
        confidence: Some(0.99),
        evidence: vec![],
    }
}

/// A resolver-proposed built-in entity value passes exactly the canonicalization the deterministic
/// extractor performs. A well-typed string is not by itself a valid date, colour or unit.
#[test]
fn resolver_built_in_entity_values_must_be_canonical_not_merely_well_typed() {
    let cases: Vec<(&str, SlotKind, Vec<(Value, bool)>)> = vec![
        (
            "when",
            SlotKind::Entity(EntityKind::new("date")),
            vec![
                (Value::String("relative:tomorrow".into()), true),
                (Value::String("2026-03-01".into()), true),
                (Value::String("2026-02-30".into()), false),
                (Value::String("relative:someday".into()), false),
                (Value::String("tomorrow".into()), false),
                (Value::Number(1.0), false),
            ],
        ),
        (
            "at",
            SlotKind::Entity(EntityKind::new("time")),
            vec![
                (Value::String("17:30".into()), true),
                (Value::String("5pm".into()), false),
                (Value::String("29:00".into()), false),
            ],
        ),
        (
            "shade",
            SlotKind::Entity(EntityKind::new("color")),
            vec![
                (Value::String("red".into()), true),
                (Value::String("crimson".into()), false),
                (Value::String("chartreuse".into()), false),
            ],
        ),
        (
            "measure",
            SlotKind::Entity(EntityKind::new("unit")),
            vec![
                (Value::String("kilogram".into()), true),
                (Value::String("parsec".into()), false),
            ],
        ),
        (
            "mail",
            SlotKind::Entity(EntityKind::new("email")),
            vec![
                (Value::String("ali@example.com".into()), true),
                (Value::String("Ali@Example.com".into()), false),
                (Value::String("not-an-email".into()), false),
            ],
        ),
    ];
    for (slot, kind, values) in cases {
        let k = entity_slot_kernel(slot, kind.clone(), entity_profile());
        let input = SemanticInput::utterance("plan trip");
        let analysis = k.analyze(&input);
        let request = k.resolver_request(&input, &analysis);
        let meaning = MeaningId::new("trip.plan");
        assert!(request.permits_meaning(&meaning));
        for (value, canonical) in values {
            let review = k.review_resolver_proposal(
                &input,
                &analysis,
                &request,
                slot_proposal(&meaning, slot, value.clone()),
            );
            assert_eq!(
                review.accepted, canonical,
                "{kind:?} value {value:?} acceptance"
            );
            if !canonical {
                assert_eq!(review.reason_code, "resolver_slot_type_mismatch");
            }
        }
    }
}

/// A quantity value must be the extractor's own object shape, not an arbitrary object.
#[test]
fn resolver_quantity_value_must_match_the_canonical_extractor_object() {
    let k = entity_slot_kernel(
        "amount",
        SlotKind::Entity(EntityKind::new("quantity")),
        entity_profile(),
    );
    let input = SemanticInput::utterance("plan trip");
    let analysis = k.analyze(&input);
    let request = k.resolver_request(&input, &analysis);
    let meaning = MeaningId::new("trip.plan");
    let canonical = Value::Object(BTreeMap::from([
        ("value".into(), Value::Number(2.0)),
        ("unit".into(), Value::String("kilogram".into())),
    ]));
    let unknown_unit = Value::Object(BTreeMap::from([
        ("value".into(), Value::Number(2.0)),
        ("unit".into(), Value::String("parsec".into())),
    ]));
    let extra_field = Value::Object(BTreeMap::from([
        ("value".into(), Value::Number(2.0)),
        ("unit".into(), Value::String("kilogram".into())),
        ("note".into(), Value::String("smuggled".into())),
    ]));
    for (value, accepted) in [
        (canonical, true),
        (unknown_unit, false),
        (extra_field, false),
        (Value::Object(BTreeMap::new()), false),
        (Value::String("2 kg".into()), false),
    ] {
        let review = k.review_resolver_proposal(
            &input,
            &analysis,
            &request,
            slot_proposal(&meaning, "amount", value.clone()),
        );
        assert_eq!(review.accepted, accepted, "quantity {value:?}");
    }
}

/// An entity kind with no authority in the active language profile has no canonical value set,
/// so the resolver cannot fill it at all. Deterministic extraction cannot produce one either.
#[test]
fn resolver_cannot_fill_an_entity_kind_that_has_no_authority_in_the_active_profile() {
    let k = entity_slot_kernel(
        "item",
        SlotKind::Entity(EntityKind::new("game.item")),
        SemanticProfile::empty(),
    );
    let input = SemanticInput::utterance("plan trip");
    let analysis = k.analyze(&input);
    let request = k.resolver_request(&input, &analysis);
    let meaning = MeaningId::new("trip.plan");
    for value in [
        Value::String("health_potion".into()),
        Value::String("laser_cannon".into()),
        Value::Object(BTreeMap::new()),
    ] {
        let review = k.review_resolver_proposal(
            &input,
            &analysis,
            &request,
            slot_proposal(&meaning, "item", value.clone()),
        );
        assert!(!review.accepted, "{value:?} must fail closed");
        assert_eq!(review.reason_code, "resolver_slot_type_mismatch");
    }
}

/// Reference authority is what the ResolverRequest exposed, not everything the host happened to
/// attach to the semantic input.
#[test]
fn resolver_reference_authority_is_the_exposed_request_projection_not_the_raw_input() {
    let kind = ReferenceKind::new("person");
    let mut pattern = MeaningPattern::new("message.send", ["send message"]);
    pattern.references.push(ReferenceSpec {
        kind: kind.clone(),
        required: true,
        elicitation: vec![ElicitationPrompt::new("en", "Who?")],
    });
    let k = kernel(vec![pattern]);
    let reference = HostReference {
        kind,
        id: ReferenceId::new("person-1"),
    };
    let mut input = SemanticInput::utterance("send message");
    input.reference_candidates.push(ResolverReferenceCandidate {
        reference: reference.clone(),
        label: Some("Ali".into()),
        aliases: vec!["ali".into()],
    });
    let analysis = k.analyze(&input);
    let exposed = k.resolver_request(&input, &analysis);
    let proposal = || ResolverProposal {
        meaning: Some(MeaningId::new("message.send")),
        slots: vec![],
        references: vec![reference.clone()],
        confidence: Some(0.99),
        evidence: vec![],
    };
    assert!(
        k.review_resolver_proposal(&input, &analysis, &exposed, proposal())
            .accepted
    );

    let mut withheld = exposed.clone();
    withheld.reference_candidates.clear();
    let review = k.review_resolver_proposal(&input, &analysis, &withheld, proposal());
    assert!(!review.accepted);
    assert_eq!(review.reason_code, "resolver_unknown_reference");
}

/// An accepted proposal produces the same canonical semantic state regardless of the order the
/// untrusted source listed its values in.
#[test]
fn accepted_resolver_meaning_uses_canonical_value_ordering() {
    let mut pattern = MeaningPattern::new("order.create", ["create order"]);
    for name in ["note", "count"] {
        pattern.slots.push(SlotSpec {
            name: name.into(),
            kind: if name == "count" {
                SlotKind::Number
            } else {
                SlotKind::String
            },
            required: true,
            elicitation: vec![ElicitationPrompt::new("en", "Value?")],
        });
    }
    let k = kernel(vec![pattern]);
    let input = SemanticInput::utterance("create order");
    let analysis = k.analyze(&input);
    let request = k.resolver_request(&input, &analysis);
    let note = SlotValue {
        name: "note".into(),
        value: Value::String("rush".into()),
        provenance: ValueProvenance::NeuralProposal,
    };
    let count = SlotValue {
        name: "count".into(),
        value: Value::Number(3.0),
        provenance: ValueProvenance::NeuralProposal,
    };
    let mut accepted = Vec::new();
    for slots in [vec![note.clone(), count.clone()], vec![count, note]] {
        let review = k.review_resolver_proposal(
            &input,
            &analysis,
            &request,
            ResolverProposal {
                meaning: Some(MeaningId::new("order.create")),
                slots,
                references: vec![],
                confidence: Some(0.99),
                evidence: vec![],
            },
        );
        assert!(review.accepted);
        accepted.push(review.meaning.expect("complete meaning"));
    }
    assert_eq!(accepted[0], accepted[1]);
    assert_eq!(accepted[0].slots[0].name, "count");
    assert_eq!(accepted[0].slots[1].name, "note");
}

#[test]
fn direct_semantic_kernel_rejects_invalid_config_instead_of_silently_running() {
    let catalog = SemanticCatalog::new(vec![MeaningPattern::new("hello", ["hello"])]).unwrap();
    for config in [
        SemanticConfig {
            candidate_limit: 1,
            ..SemanticConfig::default()
        },
        SemanticConfig {
            resolution_threshold: -0.1,
            ..SemanticConfig::default()
        },
        SemanticConfig {
            resolution_threshold: f64::NAN,
            ..SemanticConfig::default()
        },
        SemanticConfig {
            ambiguity_margin: 1.1,
            ..SemanticConfig::default()
        },
        SemanticConfig {
            resolver_min_confidence: f32::NAN,
            ..SemanticConfig::default()
        },
        SemanticConfig {
            resolver_candidate_limit: 0,
            ..SemanticConfig::default()
        },
    ] {
        assert!(matches!(
            SemanticKernel::new(
                catalog.clone(),
                test_profiles(SemanticProfile::empty()),
                config
            ),
            Err(SemanticKernelBuildError::Config(_))
        ));
    }
}

#[test]
fn resolver_confidence_threshold_is_closed_and_inclusive() {
    let p = MeaningPattern::new("weather.ask", ["weather today"]);
    let mut config = SemanticConfig::default();
    config.resolver_min_confidence = 0.75;
    let k = SemanticKernel::new(
        SemanticCatalog::new(vec![p]).unwrap(),
        test_profiles(SemanticProfile::empty()),
        config,
    )
    .unwrap();
    let input = SemanticInput::utterance("weather maybe");
    let analysis = k.analyze(&input);
    let request = k.resolver_request(&input, &analysis);
    let meaning = request.candidates.first().map(|row| row.meaning.clone());
    for (confidence, accepted) in [(0.74, false), (0.75, true), (0.99, true)] {
        let proposal = ResolverProposal {
            meaning: meaning.clone(),
            slots: vec![],
            references: vec![],
            confidence: Some(confidence),
            evidence: vec![],
        };
        let review = k.review_resolver_proposal(&input, &analysis, &request, proposal);
        assert_eq!(review.accepted, accepted, "confidence={confidence}");
    }
}

#[test]
fn built_in_entity_slot_types_are_not_flattened_to_strings() {
    let quantity_kind = SlotKind::Entity(EntityKind::new("quantity"));
    let mut quantity = BTreeMap::new();
    quantity.insert("value".into(), Value::Number(2.0));
    quantity.insert("unit".into(), Value::String("kg".into()));
    assert!(slot_value_matches_kind(
        &Value::Object(quantity),
        &quantity_kind
    ));
    assert!(!slot_value_matches_kind(
        &Value::String("2 kg".into()),
        &quantity_kind
    ));

    let date_kind = SlotKind::Entity(EntityKind::new("date"));
    assert!(slot_value_matches_kind(
        &Value::String("relative:tomorrow".into()),
        &date_kind
    ));
    assert!(!slot_value_matches_kind(&Value::Number(1.0), &date_kind));
}

#[test]
fn custom_entity_slot_is_non_null_but_remains_compiler_typed_later() {
    let kind = SlotKind::Entity(EntityKind::new("game.item"));
    assert!(slot_value_matches_kind(
        &Value::String("sword-17".into()),
        &kind
    ));
    assert!(slot_value_matches_kind(
        &Value::Object(BTreeMap::new()),
        &kind
    ));
    assert!(!slot_value_matches_kind(&Value::Null, &kind));
}

#[test]
fn trace_identity_is_deterministic_for_same_semantic_input() {
    let k = kernel(vec![MeaningPattern::new("door.open", ["open door"])]);
    let first = k.analyze(&SemanticInput::utterance("open door"));
    let second = k.analyze(&SemanticInput::utterance("open door"));
    assert_eq!(first.trace.id, second.trace.id);
    assert_eq!(first.decision, second.decision);
}

#[test]
fn resolver_reference_slot_must_name_an_exposed_reference_of_the_declared_kind() {
    let kind = ReferenceKind::new("door");
    let mut p = MeaningPattern::new("door.open", ["open maintenance door"]);
    p.slots.push(SlotSpec {
        name: "door".into(),
        kind: SlotKind::Reference(kind.clone()),
        required: true,
        elicitation: vec![ElicitationPrompt::new("en", "Which door?")],
    });
    let k = kernel(vec![p]);
    let mut input = SemanticInput::utterance("open maintenance door");
    input.reference_candidates.push(ResolverReferenceCandidate {
        reference: HostReference {
            kind,
            id: ReferenceId::new("door-17"),
        },
        label: Some("Maintenance Door".into()),
        aliases: vec!["maintenance door".into()],
    });
    let analysis = k.analyze(&input);
    let request = k.resolver_request(&input, &analysis);
    let proposal = ResolverProposal {
        meaning: Some(MeaningId::new("door.open")),
        slots: vec![SlotValue {
            name: "door".into(),
            value: Value::String("door-invented".into()),
            provenance: ValueProvenance::NeuralProposal,
        }],
        references: vec![],
        confidence: Some(0.99),
        evidence: vec![],
    };
    let review = k.review_resolver_proposal(&input, &analysis, &request, proposal);
    assert!(!review.accepted);
    assert_eq!(review.reason_code, "resolver_unknown_reference_slot");
}

#[test]
fn resolver_cannot_attach_an_exposed_but_undeclared_meaning_reference() {
    let kind = ReferenceKind::new("door");
    let p = MeaningPattern::new("weather.ask", ["weather today"]);
    let k = kernel(vec![p]);
    let mut input = SemanticInput::utterance("weather today");
    let reference = HostReference {
        kind,
        id: ReferenceId::new("door-17"),
    };
    input.reference_candidates.push(ResolverReferenceCandidate {
        reference: reference.clone(),
        label: Some("Maintenance Door".into()),
        aliases: vec!["maintenance door".into()],
    });
    let analysis = k.analyze(&input);
    let request = k.resolver_request(&input, &analysis);
    let proposal = ResolverProposal {
        meaning: Some(MeaningId::new("weather.ask")),
        slots: vec![],
        references: vec![reference],
        confidence: Some(0.99),
        evidence: vec![],
    };
    let review = k.review_resolver_proposal(&input, &analysis, &request, proposal);
    assert!(!review.accepted);
    assert_eq!(review.reason_code, "resolver_undeclared_reference");
}

#[test]
fn persian_colloquial_profile_resolves_plain_what_is_question() {
    let mut pattern = MeaningPattern::new("gvya.about", ["gvya چیست"]);
    pattern.samples = vec![LocalizedSample::new("fa", "gvya چیست")];
    let catalog = SemanticCatalog::new(vec![pattern]).unwrap();
    let mut profile = SemanticProfile::empty();
    profile.colloquial.insert("چیه".into(), vec!["چیست".into()]);
    let kernel =
        SemanticKernel::new(catalog, test_profiles(profile), SemanticConfig::default()).unwrap();
    let mut input = SemanticInput::utterance("gvya چیه");
    input.utterance.language = Some("fa".into());
    let analysis = kernel.analyze(&input);
    match analysis.decision {
        SemanticDecision::Resolved { meaning, .. } => assert_eq!(meaning.id.as_str(), "gvya.about"),
        other => panic!("expected resolved, got {other:?}"),
    }
}

#[test]
fn english_language_profile_resolves_polite_inflected_wording() {
    let mut pattern = MeaningPattern::new("package.create", ["please create package"]);
    pattern.samples = vec![LocalizedSample::new("en-US", "please create package")];
    let catalog = SemanticCatalog::new(vec![pattern]).unwrap();
    let mut profile = SemanticProfile::empty();
    profile
        .canonical_tokens
        .insert("created".into(), "create".into());
    profile.canonical_suffixes.insert("s".into(), String::new());
    profile.colloquial.insert(
        "could you please".into(),
        vec!["please".into(), "can".into(), "you".into()],
    );
    profile
        .pure_glue
        .extend(["please".into(), "can".into(), "you".into()]);
    let kernel =
        SemanticKernel::new(catalog, test_profiles(profile), SemanticConfig::default()).unwrap();
    let mut input = SemanticInput::utterance("could you please created packages");
    input.utterance.language = Some("en-US".into());
    let analysis = kernel.analyze(&input);
    match analysis.decision {
        SemanticDecision::Resolved { meaning, .. } => {
            assert_eq!(meaning.id.as_str(), "package.create");
        }
        other => panic!("expected resolved, got {other:?}"),
    }
}

#[test]
fn persian_language_profile_resolves_plural_and_inflection_without_stealing_confounder() {
    let mut pattern = MeaningPattern::new("package.create", ["پکیج بساز"]);
    pattern.samples = vec![LocalizedSample::new("fa-IR", "پکیج بساز")];
    let catalog = SemanticCatalog::new(vec![pattern]).unwrap();
    let mut profile = SemanticProfile::empty();
    profile.normalization_rewrites.insert("‌".into(), " ".into());
    profile.detached_suffixes.insert("ها".into());
    profile.canonical_tokens.extend([
        ("بساز".into(), "ساختن".into()),
        ("بسازید".into(), "ساختن".into()),
    ]);
    profile.pure_glue.extend(["لطفا".into(), "را".into()]);
    let kernel =
        SemanticKernel::new(catalog, test_profiles(profile), SemanticConfig::default()).unwrap();

    let mut positive = SemanticInput::utterance("لطفا پکیج‌ها را بسازید");
    positive.utterance.language = Some("fa-IR".into());
    match kernel.analyze(&positive).decision {
        SemanticDecision::Resolved { meaning, .. } => {
            assert_eq!(meaning.id.as_str(), "package.create");
        }
        other => panic!("expected resolved, got {other:?}"),
    }

    let mut confounder = SemanticInput::utterance("این پکیج ها نیست");
    confounder.utterance.language = Some("fa-IR".into());
    assert!(!matches!(
        kernel.analyze(&confounder).decision,
        SemanticDecision::Resolved { .. }
    ));
}

fn english_input(text: &str) -> SemanticInput {
    let mut input = SemanticInput::utterance(text);
    input.utterance.language = Some("en-US".into());
    input
}

#[test]
fn structural_pattern_is_authoritative_before_semantic_scoring() {
    let mut explicit = MeaningPattern::new("capability.runtime", ["unrelated semantic example"]);
    explicit.patterns.push(LocalizedStructuralPattern::new(
        "en",
        "^ capability * runtime",
    ));
    let semantic_competitor =
        MeaningPattern::new("semantic.competitor", ["how does capability reach runtime"]);
    let analysis = kernel(vec![explicit, semantic_competitor])
        .analyze(&english_input("how does capability reach runtime"));
    let SemanticDecision::Resolved { meaning, source } = analysis.decision else {
        panic!("expected structural resolution");
    };
    assert_eq!(meaning.id.as_str(), "capability.runtime");
    assert_eq!(source, ResolutionSource::StructuralPattern);
    assert!(analysis.scored.is_empty());
    assert_eq!(
        analysis.candidate_pruning_reason,
        "structural_pattern_authority"
    );
}

#[test]
fn semantic_matcher_remains_fallback_when_no_structural_rule_matches() {
    let mut pattern = MeaningPattern::new("capability.help", ["how does capability work"]);
    pattern.samples = vec![LocalizedSample::new("en", "how does capability work")];
    pattern
        .patterns
        .push(LocalizedStructuralPattern::new("en", "runtime * internals"));
    let analysis = kernel(vec![pattern]).analyze(&english_input("how does capability work"));
    let SemanticDecision::Resolved { meaning, source } = analysis.decision else {
        panic!("expected semantic fallback resolution");
    };
    assert_eq!(meaning.id.as_str(), "capability.help");
    assert_eq!(source, ResolutionSource::Deterministic);
    assert!(analysis.structural_match.is_none());
    assert!(!analysis.scored.is_empty());
}

#[test]
fn structural_capture_binds_declared_string_slot() {
    let mut pattern = MeaningPattern::new("search", ["search example"]);
    pattern.slots.push(SlotSpec {
        name: "query".into(),
        kind: SlotKind::String,
        required: true,
        elicitation: vec![ElicitationPrompt::new("en", "What should I search for?")],
    });
    pattern
        .patterns
        .push(LocalizedStructuralPattern::new("en", "search for *{query}"));
    let analysis = kernel(vec![pattern]).analyze(&english_input("search for red running shoes"));
    let SemanticDecision::Resolved { meaning, source } = analysis.decision else {
        panic!("expected structural capture resolution");
    };
    assert_eq!(source, ResolutionSource::StructuralPattern);
    assert_eq!(meaning.slots.len(), 1);
    assert_eq!(meaning.slots[0].name, "query");
    assert_eq!(
        meaning.slots[0].value,
        Value::String("red running shoes".into())
    );
}

#[test]
fn structural_set_capture_uses_profile_canonical_value() {
    let mut pattern = MeaningPattern::new("device.on", ["device example"]);
    pattern.slots.push(SlotSpec {
        name: "device".into(),
        kind: SlotKind::String,
        required: true,
        elicitation: vec![ElicitationPrompt::new("en", "Which device?")],
    });
    pattern.patterns.push(LocalizedStructuralPattern::new(
        "en",
        "turn on <set:devices>{device}",
    ));
    let mut profile = SemanticProfile::empty();
    profile.pattern_sets.insert(
        "devices".into(),
        BTreeMap::from([("bedroom light".into(), "light.bedroom".into())]),
    );
    let k = SemanticKernel::new(
        SemanticCatalog::new(vec![pattern]).unwrap(),
        test_profiles(profile),
        SemanticConfig::default(),
    )
    .unwrap();
    let analysis = k.analyze(&english_input("turn on bedroom light"));
    let SemanticDecision::Resolved { meaning, source } = analysis.decision else {
        panic!("expected structural set resolution");
    };
    assert_eq!(source, ResolutionSource::StructuralPattern);
    assert_eq!(
        meaning.slots[0].value,
        Value::String("light.bedroom".into())
    );
}

#[test]
fn structural_pattern_unknown_set_fails_kernel_construction() {
    let mut pattern = MeaningPattern::new("device.on", ["device example"]);
    pattern.patterns.push(LocalizedStructuralPattern::new(
        "en",
        "turn on <set:devices>",
    ));
    let error = SemanticKernel::new(
        SemanticCatalog::new(vec![pattern]).unwrap(),
        test_profiles(SemanticProfile::empty()),
        SemanticConfig::default(),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        SemanticKernelBuildError::Structural(StructuralMatcherBuildError::UnknownSet { .. })
    ));
}

#[test]
fn bounded_retrieval_authority_breaks_same_tier_near_ties_only_when_guarded() {
    fn row(id: &str, score: f64, rank: u64, rescue: f64) -> ScoredMeaning {
        ScoredMeaning {
            meaning: MeaningId::new(id),
            pattern_index: 0,
            score,
            priority: 1,
            retrieval_rank_milli: rank,
            breakdown: ScoreBreakdown {
                evidence_tier: 3,
                evidence_strength: 0.62,
                retrieval_rescue: rescue,
                no_evidence: false,
                ..ScoreBreakdown::default()
            },
        }
    }
    let breaks = |rows: &[ScoredMeaning]| SemanticKernel::retrieval_authority_breaks_near_tie(rows);

    // Several unrelated sibling pairs, each measured from the real corpus, must resolve.
    for (leader_rank, runner_rank, leader_score, runner_score, rescue) in [
        (362_355u64, 249_864u64, 0.5106, 0.5106, 0.60),
        (333_031, 254_923, 0.5877, 0.6082, 0.74),
        (518_305, 365_548, 0.6318, 0.6077, 0.82),
        (547_004, 164_165, 0.6318, 0.6061, 0.82),
        (288_143, 183_218, 0.5988, 0.5988, 0.76),
        (914_723, 133_913, 0.6510, 0.6696, 0.60),
    ] {
        let rows = vec![
            row("leader", leader_score, leader_rank, rescue),
            row("runner", runner_score, runner_rank, 0.36),
        ];
        assert!(
            breaks(&rows),
            "material authored retrieval must break the near tie at rank {leader_rank}"
        );
    }

    let strong = || {
        vec![
            row("leader", 0.6318, 547_004, 0.82),
            row("runner", 0.6061, 164_165, 0.36),
        ]
    };

    // Guard: weak absolute retrieval.
    let mut weak = strong();
    weak[0].retrieval_rank_milli = 240_000;
    weak[1].retrieval_rank_milli = 100_000;
    assert!(!breaks(&weak), "weak retrieval must stay ambiguous");

    // Guard: a close retrieval competitor.
    let mut close = strong();
    close[1].retrieval_rank_milli = 500_000;
    assert!(
        !breaks(&close),
        "a close retrieval competitor must stay ambiguous"
    );

    // Guard: authored negative evidence on the leader is never overridden by retrieval.
    let mut penalised = strong();
    penalised[0].breakdown.negative_penalty = 0.35;
    assert!(
        !breaks(&penalised),
        "retrieval must not override a soft negative"
    );
    let mut blocked = strong();
    blocked[0].breakdown.negative_hard_block = true;
    assert!(
        !breaks(&blocked),
        "retrieval must not override a hard block"
    );
    let mut rejected = strong();
    rejected[0].breakdown.rejected_reason = Some("negative_hard_block");
    assert!(
        !breaks(&rejected),
        "a rejected leader can never be confirmed"
    );

    // Guard: index authority without authored retrieval metadata.
    let mut no_metadata = strong();
    no_metadata[0].breakdown.retrieval_rescue = 0.36;
    assert!(
        !breaks(&no_metadata),
        "bounded index retrieval alone is not discrimination"
    );

    // Guard: cross-tier and weak-evidence rows stay out of this milestone entirely.
    let mut cross_tier = strong();
    cross_tier[1].breakdown.evidence_tier = 4;
    assert!(!breaks(&cross_tier), "cross-tier pairs are out of scope");
    let mut weak_tier = strong();
    weak_tier[0].breakdown.evidence_tier = 4;
    weak_tier[1].breakdown.evidence_tier = 4;
    assert!(
        !breaks(&weak_tier),
        "tier-4 evidence is never confirmed by retrieval"
    );

    // Guard: the leader may not trail the runner-up outside the near-tie band.
    let mut trailing = strong();
    trailing[0].score = 0.5700;
    trailing[1].score = 0.6300;
    assert!(
        !breaks(&trailing),
        "a materially lower score must stay ambiguous"
    );
}

#[test]
fn language_profiles_are_isolated_during_indexing_and_turn_analysis() {
    let mut en_pattern = MeaningPattern::new("package.en", ["package"]);
    en_pattern.samples = vec![LocalizedSample::new("en-US", "package")];
    let mut fa_pattern = MeaningPattern::new("package.fa", ["packages"]);
    fa_pattern.samples = vec![LocalizedSample::new("fa-IR", "packages")];
    let catalog = SemanticCatalog::new(vec![en_pattern, fa_pattern]).unwrap();

    let mut en = SemanticProfile::empty();
    en.canonical_suffixes.insert("s".into(), String::new());
    let fa = SemanticProfile::empty();
    let profiles = BTreeMap::from([("en-us".to_owned(), en), ("fa-ir".to_owned(), fa)]);
    let kernel = SemanticKernel::new(catalog, profiles, SemanticConfig::default()).unwrap();

    let mut english = SemanticInput::utterance("packages");
    english.utterance.language = Some("en-US".into());
    match kernel.analyze(&english).decision {
        SemanticDecision::Resolved { meaning, .. } => assert_eq!(meaning.id.as_str(), "package.en"),
        other => panic!("expected English profile route, got {other:?}"),
    }

    let mut persian_scope = SemanticInput::utterance("packages");
    persian_scope.utterance.language = Some("fa-IR".into());
    match kernel.analyze(&persian_scope).decision {
        SemanticDecision::Resolved { meaning, .. } => assert_eq!(meaning.id.as_str(), "package.fa"),
        other => panic!("expected isolated fa-IR profile route, got {other:?}"),
    }
}
