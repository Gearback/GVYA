//! Conversation engine tests.
use super::ConversationTurnRequest as KernelTurnRequest;
use super::*;
use crate::conversation::catalog::{
    ConversationEffect, ExtraMessage, FallbackBehavior, FallbackTrigger, FollowupDirective,
    LocalizedTexts, PredicateOp, RepeatStage, ResponseDefinition, ResponseLink, StateTarget,
    StyleLexicon, ValueCondition, ValuePath, ValueRequirement,
};
use crate::semantic::{
    ElicitationPrompt, LocalizedSample, LocalizedStructuralPattern, MeaningClass, MeaningPattern,
    SemanticCatalog, SlotKind, SlotSpec,
};
use gvya_model::{FollowupId, ResponseItem, TopicId};

struct ConversationTurnRequest;

impl ConversationTurnRequest {
    fn utterance(text: impl Into<String>, state: GvyaState) -> KernelTurnRequest {
        let mut request = KernelTurnRequest::utterance(text, state);
        request.utterance.language = Some("en".to_owned());
        request.semantic_language_fallbacks = vec!["und".to_owned()];
        request
    }
}

fn meaning_pattern<I, S>(id: impl Into<String>, samples: I) -> MeaningPattern
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut pattern = MeaningPattern::new(id, samples);
    for sample in &mut pattern.samples {
        sample.language = "en".to_owned();
    }
    pattern
}

fn behavior(id: &str, meaning: &str, response: ResponseDefinition) -> ConversationBehavior {
    ConversationBehavior {
        id: BehaviorId::new(id),
        meaning: MeaningId::new(meaning),
        topic: None,
        topic_scoped: false,
        activates_topic: false,
        topic_ttl: None,
        followup_scope: None,
        repair_continuation_candidate: false,
        repeat_same_input_after: None,
        repeat_same_meaning_after: None,
        requires_values: Vec::new(),
        forbidden_values: Vec::new(),
        responses: vec![response],
    }
}

fn conversation_test_profile() -> SemanticProfiles {
    let mut profile = SemanticProfile::empty();
    profile.pure_glue.extend(
        ["a", "the", "is", "this", "that", "it", "please"]
            .into_iter()
            .map(str::to_owned),
    );
    profile
        .generic_singletons
        .extend(["what", "why", "how"].into_iter().map(str::to_owned));
    profile
        .social_vocabulary
        .extend(["hello", "hi", "friend"].into_iter().map(str::to_owned));
    profile.task_cues.extend(
        ["open", "send", "inspect", "settings"]
            .into_iter()
            .map(str::to_owned),
    );
    profile
        .continuation_exact_phrases
        .extend(["the other one", "got it"].into_iter().map(str::to_owned));
    profile
        .generic_followup_phrases
        .insert("tell me more".to_owned());
    profile.pronouns.insert("it".to_owned());
    profile.continuation_references.insert("it".to_owned());
    profile
        .continuation_question_starters
        .extend(["who", "what"].into_iter().map(str::to_owned));
    BTreeMap::from([
        ("und".to_owned(), profile.clone()),
        ("en".to_owned(), profile.clone()),
        ("fa".to_owned(), profile),
    ])
}

fn empty_test_profiles() -> SemanticProfiles {
    let profile = SemanticProfile::empty();
    BTreeMap::from([
        ("und".to_owned(), profile.clone()),
        ("en".to_owned(), profile.clone()),
        ("fa".to_owned(), profile),
    ])
}

fn build_kernel(
    patterns: Vec<MeaningPattern>,
    behaviors: Vec<ConversationBehavior>,
    openings: Vec<OpeningDefinition>,
    fallback: Vec<ResponseDefinition>,
    repeat_fallback: Vec<ResponseDefinition>,
    config: ConversationConfig,
) -> ConversationKernel {
    let semantic = SemanticCatalog::new(patterns).expect("semantic catalog");
    let mut fallback_behaviors = Vec::new();
    if !fallback.is_empty() {
        fallback_behaviors.push(FallbackBehavior {
            id: BehaviorId::new("test.fallback.unresolved"),
            trigger: FallbackTrigger::Unresolved,
            priority: 0,
            conditions: Vec::new(),
            responses: fallback,
        });
    }
    if !repeat_fallback.is_empty() {
        fallback_behaviors.push(FallbackBehavior {
            id: BehaviorId::new("test.fallback.repeat"),
            trigger: FallbackTrigger::Repeat,
            priority: 0,
            conditions: Vec::new(),
            responses: repeat_fallback,
        });
    }
    let catalog = ConversationCatalog::new(behaviors, Vec::new(), openings, fallback_behaviors)
        .expect("conversation catalog");
    ConversationKernel::new(
        semantic,
        conversation_test_profile(),
        SemanticConfig::default(),
        catalog,
        config,
    )
    .expect("valid conversation config")
}

fn kernel() -> ConversationKernel {
    let mut hello = meaning_pattern("hello", ["hello", "hi"]);
    hello.class = MeaningClass::Social;
    build_kernel(
        vec![
            hello,
            meaning_pattern("why_alarm", ["why"]),
            meaning_pattern("settings.open", ["open settings"]),
        ],
        vec![
            behavior(
                "hello.behavior",
                "hello",
                ResponseDefinition::text("hello.response", "en", "Hello"),
            ),
            behavior(
                "settings.behavior",
                "settings.open",
                ResponseDefinition::text("settings.response", "en", "Settings"),
            ),
        ],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        ConversationConfig::default(),
    )
}

fn first_text(outcome: &ConversationOutcome) -> Option<&str> {
    outcome
        .response
        .messages
        .first()?
        .items
        .iter()
        .find_map(|item| {
            if let ResponseItem::Text { text, .. } = item {
                Some(text.as_str())
            } else {
                None
            }
        })
}

#[test]
fn greeting_is_legitimate_conversation_not_a_tool_call() {
    let out = kernel().respond(ConversationTurnRequest::utterance(
        "hi",
        GvyaState::default(),
    ));
    assert_eq!(out.mode, ConversationMode::Answer);
    assert_eq!(first_text(&out), Some("Hello"));
}

#[test]
fn ordinary_deictic_question_can_be_authored_as_conversation() {
    let k = build_kernel(
        vec![meaning_pattern("describe_current", ["what is this"])],
        vec![behavior(
            "describe.behavior",
            "describe_current",
            ResponseDefinition::text("describe.response", "en", "It is the current object."),
        )],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        ConversationConfig::default(),
    );
    let out = k.respond(ConversationTurnRequest::utterance(
        "what is this",
        GvyaState::default(),
    ));
    assert_eq!(out.mode, ConversationMode::Answer);
    assert_eq!(first_text(&out), Some("It is the current object."));
}

#[test]
fn short_referential_phrase_uses_explicit_conversation_continuation() {
    let k = build_kernel(
        vec![meaning_pattern("inspect", ["inspect object"])],
        vec![behavior(
            "inspect.behavior",
            "inspect",
            ResponseDefinition::text("inspect.response", "en", "Still on that subject."),
        )],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        ConversationConfig::default(),
    );
    let first = k.respond(ConversationTurnRequest::utterance(
        "inspect object",
        GvyaState::default(),
    ));
    let second = k.respond(ConversationTurnRequest::utterance(
        "the other one",
        first.state,
    ));
    assert_eq!(second.mode, ConversationMode::Continuation);
    assert_eq!(
        second.behavior.as_ref().map(BehaviorId::as_str),
        Some("inspect.behavior")
    );
    assert_eq!(first_text(&second), Some("Still on that subject."));
}

#[test]
fn strong_standalone_meaning_is_not_stolen_by_contextual_continuation() {
    let k = build_kernel(
        vec![
            meaning_pattern("inspect", ["inspect object"]),
            meaning_pattern("acknowledge", ["got it"]),
        ],
        vec![
            behavior(
                "inspect.behavior",
                "inspect",
                ResponseDefinition::text("inspect.response", "en", "Inspect"),
            ),
            behavior(
                "acknowledge.behavior",
                "acknowledge",
                ResponseDefinition::text("acknowledge.response", "en", "Acknowledged"),
            ),
        ],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        ConversationConfig::default(),
    );
    let first = k.respond(ConversationTurnRequest::utterance(
        "inspect object",
        GvyaState::default(),
    ));
    let second = k.respond(ConversationTurnRequest::utterance("got it", first.state));
    assert_eq!(second.mode, ConversationMode::Answer);
    assert_eq!(
        second.meaning.as_ref().map(|meaning| meaning.id.as_str()),
        Some("acknowledge")
    );
    assert_eq!(first_text(&second), Some("Acknowledged"));
}

#[test]
fn structural_standalone_meaning_is_not_stolen_by_contextual_continuation() {
    let mut structural = meaning_pattern("describe_current", ["unrelated semantic sample"]);
    structural
        .patterns
        .push(LocalizedStructuralPattern::new("en", "what is it"));
    let k = build_kernel(
        vec![meaning_pattern("inspect", ["inspect object"]), structural],
        vec![
            behavior(
                "inspect.behavior",
                "inspect",
                ResponseDefinition::text("inspect.response", "en", "Still inspecting."),
            ),
            behavior(
                "describe.behavior",
                "describe_current",
                ResponseDefinition::text("describe.response", "en", "Structural answer."),
            ),
        ],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        ConversationConfig::default(),
    );
    let first = k.respond(ConversationTurnRequest::utterance(
        "inspect object",
        GvyaState::default(),
    ));
    let second = k.respond(ConversationTurnRequest::utterance(
        "what is it",
        first.state,
    ));
    assert_eq!(second.mode, ConversationMode::Answer);
    assert_eq!(
        second.meaning.as_ref().map(|meaning| meaning.id.as_str()),
        Some("describe_current")
    );
    assert_eq!(first_text(&second), Some("Structural answer."));
}

#[test]
fn weak_referential_question_continues_the_previous_meaning() {
    let k = build_kernel(
        vec![meaning_pattern("capability", ["what is a capability"])],
        vec![behavior(
            "capability.behavior",
            "capability",
            ResponseDefinition::text("capability.response", "en", "The host executes it."),
        )],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        ConversationConfig::default(),
    );
    let first = k.respond(ConversationTurnRequest::utterance(
        "what is a capability",
        GvyaState::default(),
    ));
    let second = k.respond(ConversationTurnRequest::utterance(
        "who actually executes it",
        first.state,
    ));
    assert_eq!(second.mode, ConversationMode::Continuation);
    assert_eq!(
        second.meaning.as_ref().map(|meaning| meaning.id.as_str()),
        Some("capability")
    );
    assert_eq!(first_text(&second), Some("The host executes it."));
}

#[test]
fn behavior_requires_values_gate_semantic_candidate_before_response_selection() {
    let mut exact = behavior(
        "vault.exact.behavior",
        "vault.exact",
        ResponseDefinition::text("vault.exact.response", "en", "exact"),
    );
    exact.requires_values.push(ValueRequirement {
        path: ValuePath::context("vaultAllowed"),
        value: Value::Bool(true),
    });
    let k = build_kernel(
        vec![
            meaning_pattern("vault.exact", ["open the vault door"]),
            meaning_pattern("vault.general", ["open vault"]),
        ],
        vec![
            exact,
            behavior(
                "vault.general.behavior",
                "vault.general",
                ResponseDefinition::text("vault.general.response", "en", "general"),
            ),
        ],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        ConversationConfig::default(),
    );

    let blocked = k.respond(ConversationTurnRequest::utterance(
        "open the vault door",
        GvyaState::default(),
    ));
    assert_eq!(
        blocked.meaning.as_ref().map(|meaning| meaning.id.as_str()),
        Some("vault.general")
    );

    let mut request =
        ConversationTurnRequest::utterance("open the vault door", GvyaState::default());
    request
        .context
        .values
        .insert("vaultAllowed".into(), Value::Bool(true));
    let allowed = k.respond(request);
    assert_eq!(
        allowed.meaning.as_ref().map(|meaning| meaning.id.as_str()),
        Some("vault.exact")
    );
    assert_eq!(first_text(&allowed), Some("exact"));
}

#[test]
fn behavior_forbidden_values_gate_semantic_candidate_when_blocked_value_matches() {
    let mut exact = behavior(
        "door.exact.behavior",
        "door.exact",
        ResponseDefinition::text("door.exact.response", "en", "exact"),
    );
    exact.forbidden_values.push(ValueRequirement {
        path: ValuePath::context("maintenance"),
        value: Value::Bool(true),
    });
    let k = build_kernel(
        vec![
            meaning_pattern("door.exact", ["open the service door"]),
            meaning_pattern("door.general", ["open door"]),
        ],
        vec![
            exact,
            behavior(
                "door.general.behavior",
                "door.general",
                ResponseDefinition::text("door.general.response", "en", "general"),
            ),
        ],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        ConversationConfig::default(),
    );

    let allowed = k.respond(ConversationTurnRequest::utterance(
        "open the service door",
        GvyaState::default(),
    ));
    assert_eq!(
        allowed.meaning.as_ref().map(|meaning| meaning.id.as_str()),
        Some("door.exact")
    );

    let mut request =
        ConversationTurnRequest::utterance("open the service door", GvyaState::default());
    request
        .context
        .values
        .insert("maintenance".into(), Value::Bool(true));
    let blocked = k.respond(request);
    assert_eq!(
        blocked.meaning.as_ref().map(|meaning| meaning.id.as_str()),
        Some("door.general")
    );
}

#[test]
fn response_ineligible_top_candidate_reranks_to_answerable_semantic_runner_up() {
    let mut blocked = ResponseDefinition::text("vault.blocked", "en", "top candidate");
    blocked.conditions.push(ValueCondition {
        path: ValuePath::context("vaultAllowed"),
        op: PredicateOp::Equal,
        value: Some(Value::Bool(true)),
    });
    let k = build_kernel(
        vec![
            meaning_pattern("vault.exact", ["open the vault door"]),
            meaning_pattern("vault.general", ["open vault"]),
        ],
        vec![
            behavior("vault.exact.behavior", "vault.exact", blocked),
            behavior(
                "vault.general.behavior",
                "vault.general",
                ResponseDefinition::text("vault.general.response", "en", "runner up"),
            ),
        ],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        ConversationConfig::default(),
    );
    let out = k.respond(ConversationTurnRequest::utterance(
        "open the vault door",
        GvyaState::default(),
    ));
    assert_eq!(out.mode, ConversationMode::Answer);
    assert_eq!(
        out.meaning.as_ref().map(|meaning| meaning.id.as_str()),
        Some("vault.general")
    );
    assert_eq!(first_text(&out), Some("runner up"));
    assert!(
        out.trace
            .events
            .iter()
            .any(|event| event.code.as_str() == "conversation.response.candidate_ineligible")
    );
    assert!(
        out.trace
            .events
            .iter()
            .any(|event| event.code.as_str() == "conversation.response.reranked")
    );
}

#[test]
fn response_ineligible_structural_winner_is_not_replaced_by_semantic_runner_up() {
    let mut explicit = meaning_pattern("vault.explicit", ["unrelated example"]);
    explicit
        .patterns
        .push(LocalizedStructuralPattern::new("en", "open the vault door"));
    let mut blocked = ResponseDefinition::text("vault.blocked", "en", "explicit");
    blocked.conditions.push(ValueCondition {
        path: ValuePath::context("vaultAllowed"),
        op: PredicateOp::Equal,
        value: Some(Value::Bool(true)),
    });
    let k = build_kernel(
        vec![
            explicit,
            meaning_pattern("vault.semantic", ["open the vault door"]),
        ],
        vec![
            behavior("vault.explicit.behavior", "vault.explicit", blocked),
            behavior(
                "vault.semantic.behavior",
                "vault.semantic",
                ResponseDefinition::text("vault.semantic.response", "en", "semantic runner up"),
            ),
        ],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        ConversationConfig::default(),
    );
    let out = k.respond(ConversationTurnRequest::utterance(
        "open the vault door",
        GvyaState::default(),
    ));
    assert_ne!(
        out.meaning.as_ref().map(|meaning| meaning.id.as_str()),
        Some("vault.semantic")
    );
    assert!(
        !out.trace
            .events
            .iter()
            .any(|event| event.code.as_str() == "conversation.response.reranked")
    );
}

#[test]
fn response_rerank_refuses_to_hide_ambiguity_between_runner_ups() {
    let mut blocked = ResponseDefinition::text("vault.blocked", "en", "top candidate");
    blocked.conditions.push(ValueCondition {
        path: ValuePath::context("vaultAllowed"),
        op: PredicateOp::Equal,
        value: Some(Value::Bool(true)),
    });
    let k = build_kernel(
        vec![
            meaning_pattern("vault.exact", ["open the vault door"]),
            meaning_pattern("vault.runner.b", ["open vault"]),
            meaning_pattern("vault.runner.c", ["open vault"]),
        ],
        vec![
            behavior("vault.exact.behavior", "vault.exact", blocked),
            behavior(
                "vault.runner.b.behavior",
                "vault.runner.b",
                ResponseDefinition::text("vault.runner.b.response", "en", "B"),
            ),
            behavior(
                "vault.runner.c.behavior",
                "vault.runner.c",
                ResponseDefinition::text("vault.runner.c.response", "en", "C"),
            ),
        ],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        ConversationConfig::default(),
    );
    let out = k.respond(ConversationTurnRequest::utterance(
        "open the vault door",
        GvyaState::default(),
    ));
    assert_ne!(
        out.meaning.as_ref().map(|meaning| meaning.id.as_str()),
        Some("vault.runner.b")
    );
    assert_ne!(
        out.meaning.as_ref().map(|meaning| meaning.id.as_str()),
        Some("vault.runner.c")
    );
    assert!(
        out.trace
            .events
            .iter()
            .any(|event| event.code.as_str() == "conversation.response.rerank_ambiguous")
    );
}

#[test]
fn operational_task_cue_cannot_be_stolen_by_contextual_continuation() {
    let k = build_kernel(
        vec![
            meaning_pattern("inspect", ["inspect object"]),
            meaning_pattern("send", ["send it"]),
        ],
        vec![
            behavior(
                "inspect.behavior",
                "inspect",
                ResponseDefinition::text("inspect.response", "en", "Inspect"),
            ),
            behavior(
                "send.behavior",
                "send",
                ResponseDefinition::text("send.response", "en", "Send"),
            ),
        ],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        ConversationConfig::default(),
    );
    let first = k.respond(ConversationTurnRequest::utterance(
        "inspect object",
        GvyaState::default(),
    ));
    let second = k.respond(ConversationTurnRequest::utterance("send it", first.state));
    assert_eq!(second.mode, ConversationMode::Answer);
    assert_eq!(
        second.meaning.as_ref().map(|meaning| meaning.id.as_str()),
        Some("send")
    );
    assert_eq!(first_text(&second), Some("Send"));
}

#[test]
fn generic_followup_phrase_continues_only_after_standalone_semantics_stay_weak() {
    let k = build_kernel(
        vec![meaning_pattern("inspect", ["inspect object"])],
        vec![behavior(
            "inspect.behavior",
            "inspect",
            ResponseDefinition::text("inspect.response", "en", "More detail"),
        )],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        ConversationConfig::default(),
    );
    let first = k.respond(ConversationTurnRequest::utterance(
        "inspect object",
        GvyaState::default(),
    ));
    let second = k.respond(ConversationTurnRequest::utterance(
        "tell me more",
        first.state,
    ));
    assert_eq!(second.mode, ConversationMode::Continuation);
    assert_eq!(
        second.meaning.as_ref().map(|meaning| meaning.id.as_str()),
        Some("inspect")
    );
    assert_eq!(first_text(&second), Some("More detail"));
    assert!(
        second
            .trace
            .events
            .iter()
            .any(|event| event.code.as_str() == "conversation.continuation.generic")
    );
}

#[test]
fn exact_global_meaning_escapes_followup_scope_after_scope_miss() {
    let mut yes = behavior(
        "yes.behavior",
        "yes",
        ResponseDefinition::text("yes.response", "en", "Yes"),
    );
    yes.followup_scope = Some(FollowupId::new("confirm"));
    let k = build_kernel(
        vec![
            meaning_pattern("yes", ["yes"]),
            meaning_pattern("hello", ["hello"]),
        ],
        vec![
            yes,
            behavior(
                "hello.behavior",
                "hello",
                ResponseDefinition::text("hello.response", "en", "Hello"),
            ),
        ],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        ConversationConfig::default(),
    );
    let mut state = GvyaState::default();
    state.conversation.active_followup = Some(gvya_model::ActiveFollowup {
        id: FollowupId::new("confirm"),
        ttl: 2,
        source_behavior: None,
    });
    let out = k.respond(ConversationTurnRequest::utterance("hello", state));
    assert_eq!(out.mode, ConversationMode::Answer);
    assert_eq!(
        out.meaning.as_ref().map(|meaning| meaning.id.as_str()),
        Some("hello")
    );
    assert_eq!(first_text(&out), Some("Hello"));
    assert_eq!(
        out.state
            .conversation
            .active_followup
            .as_ref()
            .map(|followup| followup.ttl),
        Some(1)
    );
}

#[test]
fn direct_address_social_phrase_remains_answerable_without_stealing_task_context() {
    let k = kernel();
    let social = k.respond(ConversationTurnRequest::utterance(
        "hello friend",
        GvyaState::default(),
    ));
    assert_eq!(social.mode, ConversationMode::Answer);
    assert_eq!(first_text(&social), Some("Hello"));
    let task = k.respond(ConversationTurnRequest::utterance(
        "hello friend open settings",
        GvyaState::default(),
    ));
    assert_ne!(
        task.meaning.as_ref().map(|meaning| meaning.id.as_str()),
        Some("hello")
    );
}

#[test]
fn unresolved_turn_without_authored_fallback_is_silent() {
    let k = kernel();
    let first = k.respond(ConversationTurnRequest::utterance(
        "zxqv",
        GvyaState::default(),
    ));
    assert_eq!(first.mode, ConversationMode::Silent);
    assert_eq!(first_text(&first), None);
    let second = k.respond(ConversationTurnRequest::utterance(
        "still zxqv",
        first.state,
    ));
    assert_eq!(second.mode, ConversationMode::Silent);
    assert_eq!(second.state.conversation.repair.consecutive, 2);
    assert_eq!(first_text(&second), None);
}

#[test]
fn followup_ttl_two_survives_one_miss_then_matches() {
    let mut ask_response = ResponseDefinition::text("ask.response", "en", "Want me to continue?");
    ask_response.opens_followup = Some(FollowupDirective {
        id: FollowupId::new("confirm"),
        ttl: 2,
        refresh_if_same: false,
    });
    let ask = behavior("ask.behavior", "ask", ask_response);
    let mut yes = behavior(
        "yes.behavior",
        "yes",
        ResponseDefinition::text("yes.response", "en", "Okay"),
    );
    yes.followup_scope = Some(FollowupId::new("confirm"));
    let k = build_kernel(
        vec![
            meaning_pattern("ask", ["continue"]),
            meaning_pattern("yes", ["yes"]),
        ],
        vec![ask, yes],
        Vec::new(),
        Vec::new(),
        vec![ResponseDefinition::text(
            "repeat.response",
            "en",
            "You already said that.",
        )],
        ConversationConfig::default(),
    );
    let opened = k.respond(ConversationTurnRequest::utterance(
        "continue",
        GvyaState::default(),
    ));
    assert_eq!(
        opened
            .state
            .conversation
            .active_followup
            .as_ref()
            .map(|f| f.ttl),
        Some(2)
    );
    let miss = k.respond(ConversationTurnRequest::utterance(
        "something unrelated",
        opened.state,
    ));
    assert_eq!(
        miss.state
            .conversation
            .active_followup
            .as_ref()
            .map(|f| f.ttl),
        Some(1)
    );
    let yes = k.respond(ConversationTurnRequest::utterance("yes", miss.state));
    assert_eq!(yes.mode, ConversationMode::Followup);
    assert!(yes.state.conversation.active_followup.is_none());
    assert_eq!(first_text(&yes), Some("Okay"));
}

#[test]
fn shared_meaning_uses_distinct_default_and_followup_behaviors() {
    let mut ask_response = ResponseDefinition::text("ask.response", "en", "Confirm?");
    ask_response.opens_followup = Some(FollowupDirective {
        id: FollowupId::new("confirm"),
        ttl: 2,
        refresh_if_same: false,
    });
    let ask = behavior("ask.behavior", "ask", ask_response);
    let global_yes = behavior(
        "yes.global.behavior",
        "yes",
        ResponseDefinition::text("yes.global.response", "en", "General yes"),
    );
    let mut scoped_yes = behavior(
        "yes.confirm.behavior",
        "yes",
        ResponseDefinition::text("yes.confirm.response", "en", "Confirmed"),
    );
    scoped_yes.followup_scope = Some(FollowupId::new("confirm"));
    let k = build_kernel(
        vec![
            meaning_pattern("ask", ["ask"]),
            meaning_pattern("yes", ["yes"]),
        ],
        vec![ask, global_yes, scoped_yes],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        ConversationConfig::default(),
    );

    let global = k.respond(ConversationTurnRequest::utterance(
        "yes",
        GvyaState::default(),
    ));
    assert_eq!(global.mode, ConversationMode::Answer);
    assert_eq!(
        global.behavior.as_ref().map(BehaviorId::as_str),
        Some("yes.global.behavior")
    );

    let opened = k.respond(ConversationTurnRequest::utterance(
        "ask",
        GvyaState::default(),
    ));
    let scoped = k.respond(ConversationTurnRequest::utterance("yes", opened.state));
    assert_eq!(scoped.mode, ConversationMode::Followup);
    assert_eq!(
        scoped.behavior.as_ref().map(BehaviorId::as_str),
        Some("yes.confirm.behavior")
    );
    assert_eq!(first_text(&scoped), Some("Confirmed"));
}

#[test]
fn followup_ttl_one_is_still_eligible_for_current_turn() {
    let mut ask_response = ResponseDefinition::text("ask.response", "en", "Confirm?");
    ask_response.opens_followup = Some(FollowupDirective {
        id: FollowupId::new("confirm"),
        ttl: 1,
        refresh_if_same: false,
    });
    let ask = behavior("ask.behavior", "ask", ask_response);
    let mut yes = behavior(
        "yes.behavior",
        "yes",
        ResponseDefinition::text("yes.response", "en", "Confirmed"),
    );
    yes.followup_scope = Some(FollowupId::new("confirm"));
    let k = build_kernel(
        vec![
            meaning_pattern("ask", ["ask"]),
            meaning_pattern("yes", ["yes"]),
        ],
        vec![ask, yes],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        ConversationConfig::default(),
    );
    let opened = k.respond(ConversationTurnRequest::utterance(
        "ask",
        GvyaState::default(),
    ));
    let yes = k.respond(ConversationTurnRequest::utterance("yes", opened.state));
    assert_eq!(yes.mode, ConversationMode::Followup);
    assert!(yes.state.conversation.active_followup.is_none());
}

#[test]
fn global_repeat_does_not_consume_followup_and_miss_ticks_ttl() {
    let k = build_kernel(
        vec![meaning_pattern("hello", ["hello"])],
        vec![behavior(
            "hello.behavior",
            "hello",
            ResponseDefinition::text("hello.response", "en", "Hello"),
        )],
        Vec::new(),
        Vec::new(),
        vec![ResponseDefinition::text(
            "repeat.response",
            "en",
            "I heard you.",
        )],
        ConversationConfig::default(),
    );
    let mut state = GvyaState::default();
    state.conversation.active_followup = Some(gvya_model::ActiveFollowup {
        id: FollowupId::new("confirm"),
        ttl: 2,
        source_behavior: None,
    });
    state.conversation.recent_user_messages = vec!["again".into(), "again".into(), "again".into()];
    let out = k.respond(ConversationTurnRequest::utterance("again", state));
    assert_eq!(out.mode, ConversationMode::RepeatFallback);
    assert_eq!(
        out.state
            .conversation
            .active_followup
            .as_ref()
            .map(|f| f.ttl),
        Some(1)
    );
}

#[test]
fn topic_scoped_meaning_is_not_fresh_but_resolves_after_activation() {
    let mut activate = behavior(
        "alarm.behavior",
        "alarm",
        ResponseDefinition::text("alarm.response", "en", "Alarm topic"),
    );
    activate.topic = Some(TopicId::new("alarm"));
    activate.activates_topic = true;
    let mut why = behavior(
        "why.behavior",
        "why",
        ResponseDefinition::text("why.response", "en", "Because it detected smoke."),
    );
    why.topic = Some(TopicId::new("alarm"));
    why.topic_scoped = true;
    let k = build_kernel(
        vec![
            meaning_pattern("alarm", ["alarm"]),
            meaning_pattern("why", ["why"]),
        ],
        vec![activate, why],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        ConversationConfig::default(),
    );
    let fresh = k.respond(ConversationTurnRequest::utterance(
        "why",
        GvyaState::default(),
    ));
    assert_eq!(fresh.mode, ConversationMode::Silent);
    let topic = k.respond(ConversationTurnRequest::utterance(
        "alarm",
        GvyaState::default(),
    ));
    assert_eq!(
        topic
            .state
            .conversation
            .active_topic
            .as_ref()
            .map(|t| t.id.as_str()),
        Some("alarm")
    );
    let scoped = k.respond(ConversationTurnRequest::utterance("why", topic.state));
    assert!(matches!(
        scoped.mode,
        ConversationMode::Answer | ConversationMode::TopicContext
    ));
    assert_eq!(first_text(&scoped), Some("Because it detected smoke."));
}

#[test]
fn accepted_behavior_on_current_topic_refreshes_topic_ttl() {
    let mut activate = behavior(
        "alarm.behavior",
        "alarm",
        ResponseDefinition::text("alarm.response", "en", "Alarm"),
    );
    activate.topic = Some(TopicId::new("alarm"));
    activate.activates_topic = true;
    activate.topic_ttl = Some(2);
    let mut why = behavior(
        "why.behavior",
        "why",
        ResponseDefinition::text("why.response", "en", "Because."),
    );
    why.topic = Some(TopicId::new("alarm"));
    why.topic_scoped = true;
    why.topic_ttl = Some(2);
    let k = build_kernel(
        vec![
            meaning_pattern("alarm", ["alarm"]),
            meaning_pattern("why", ["why"]),
        ],
        vec![activate, why],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        ConversationConfig::default(),
    );
    let topic = k.respond(ConversationTurnRequest::utterance(
        "alarm",
        GvyaState::default(),
    ));
    assert_eq!(
        topic
            .state
            .conversation
            .active_topic
            .as_ref()
            .map(|t| t.ttl),
        Some(2)
    );
    let scoped = k.respond(ConversationTurnRequest::utterance("why", topic.state));
    assert_eq!(
        scoped
            .state
            .conversation
            .active_topic
            .as_ref()
            .map(|t| t.ttl),
        Some(2)
    );
}

#[test]
fn response_effects_are_visible_to_same_response_template() {
    let mut response = ResponseDefinition::text("name.response", "en", "Hello {{ author.name }}");
    response.effects.push(ConversationEffect::Assign {
        target: StateTarget::Author("name".to_string()),
        value: Value::String("Ada".to_string()),
    });
    let k = build_kernel(
        vec![meaning_pattern("name", ["name"])],
        vec![behavior("name.behavior", "name", response)],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        ConversationConfig::default(),
    );
    let out = k.respond(ConversationTurnRequest::utterance(
        "name",
        GvyaState::default(),
    ));
    assert_eq!(first_text(&out), Some("Hello Ada"));
    assert_eq!(
        out.state.author.get("name"),
        Some(&Value::String("Ada".to_string()))
    );
}

#[test]
fn extra_messages_are_deterministic_and_hard_capped_at_six_total() {
    let mut response = ResponseDefinition::text("flow.response", "en", "one");
    for index in 0..10 {
        response.extra_messages.push(ExtraMessage {
            chance: 1.0,
            texts: vec![LocalizedTexts {
                language: "en".to_string(),
                variants: vec![format!("extra-{index}")],
            }],
        });
    }
    let mut config = ConversationConfig::default();
    config.max_messages_per_turn = 6;
    let k = build_kernel(
        vec![meaning_pattern("flow", ["flow"])],
        vec![behavior("flow.behavior", "flow", response)],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        config,
    );
    let mut req = ConversationTurnRequest::utterance("flow", GvyaState::default());
    req.seed = Some(7);
    let a = k.respond(req.clone());
    let b = k.respond(req);
    assert_eq!(a.response, b.response);
    assert_eq!(a.response.messages.len(), 6);
}

#[test]
fn repeat_stage_ladder_uses_repeat_annoyed_and_final_rows() {
    let normal = ResponseDefinition::text("ping.normal", "en", "normal");
    let mut repeat = ResponseDefinition::text("ping.repeat", "en", "repeat");
    repeat.repeat_stage = Some(RepeatStage::Repeat);
    let mut annoyed = ResponseDefinition::text("ping.annoyed", "en", "annoyed");
    annoyed.repeat_stage = Some(RepeatStage::Annoyed);
    let mut final_repeat = ResponseDefinition::text("ping.final", "en", "final");
    final_repeat.repeat_stage = Some(RepeatStage::Final);
    let mut b = behavior("ping.behavior", "ping", normal);
    b.responses.extend([repeat, annoyed, final_repeat]);
    let mut config = ConversationConfig::default();
    config.repeat_detection_threshold = 20;
    let k = build_kernel(
        vec![meaning_pattern("ping", ["ping"])],
        vec![b],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        config,
    );
    let one = k.respond(ConversationTurnRequest::utterance(
        "ping",
        GvyaState::default(),
    ));
    assert_eq!(first_text(&one), Some("normal"));
    let two = k.respond(ConversationTurnRequest::utterance("ping", one.state));
    assert_eq!(first_text(&two), Some("repeat"));
    let three = k.respond(ConversationTurnRequest::utterance("ping", two.state));
    assert_eq!(first_text(&three), Some("annoyed"));
    let four = k.respond(ConversationTurnRequest::utterance("ping", three.state));
    assert_eq!(first_text(&four), Some("final"));
}

#[test]
fn hint_progress_advances_through_authored_ladder() {
    let normal = ResponseDefinition::text("help.normal", "en", "Try it.");
    let mut hint_one = ResponseDefinition::text("help.h1", "en", "Hint one");
    hint_one.hint_level = Some(1);
    let mut hint_two = ResponseDefinition::text("help.h2", "en", "Hint two");
    hint_two.hint_level = Some(2);
    let mut b = behavior("help.behavior", "help", normal);
    b.responses.extend([hint_one, hint_two]);
    let k = build_kernel(
        vec![meaning_pattern("help", ["hint"])],
        vec![b],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        ConversationConfig::default(),
    );
    let mut first_req = ConversationTurnRequest::utterance("hint", GvyaState::default());
    first_req.hint = HintRequest::First;
    let first = k.respond(first_req);
    assert_eq!(first_text(&first), Some("Hint one"));
    let mut next_req = ConversationTurnRequest::utterance("hint", first.state);
    next_req.hint = HintRequest::Next;
    let next = k.respond(next_req);
    assert_eq!(first_text(&next), Some("Hint two"));
}

#[test]
fn explicit_response_and_semantic_fallbacks_are_used_without_global_language_assumption() {
    let response = ResponseDefinition {
        id: gvya_model::ResponseId::new("locale.response"),
        kind: ResponseKind::Normal,
        texts: vec![LocalizedTexts {
            language: "fa".to_string(),
            variants: vec!["سلام".to_string()],
        }],
        conditions: Vec::new(),
        hint_level: None,
        repeat_stage: None,
        effects: Vec::new(),
        opens_followup: None,
        extra_messages: Vec::new(),
        assets: Vec::new(),
        links: Vec::new(),
    };
    let k = build_kernel(
        vec![MeaningPattern::new("hello", ["hello"])],
        vec![behavior("hello.behavior", "hello", response)],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        ConversationConfig::default(),
    );
    let mut req = ConversationTurnRequest::utterance("hello", GvyaState::default());
    req.utterance.language = Some("de-DE".to_string());
    req.semantic_language_fallbacks = vec!["und".to_string()];
    req.language_policy.base_fallback = Some("fa".to_string());
    let out = k.respond(req);
    assert_eq!(first_text(&out), Some("سلام"));
}

#[test]
fn matched_sample_language_selects_response_without_host_language() {
    let mut response = ResponseDefinition::text("hello.response", "en", "Hello");
    response.texts.push(LocalizedTexts {
        language: "fa".to_string(),
        variants: vec!["سلام".to_string()],
    });
    let mut pattern = meaning_pattern("hello", ["placeholder"]);
    pattern.samples = vec![
        LocalizedSample::new("en", "hello"),
        LocalizedSample::new("fa", "سلام"),
    ];
    let k = build_kernel(
        vec![pattern],
        vec![behavior("hello.behavior", "hello", response)],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        ConversationConfig::default(),
    );

    let out = k.respond(KernelTurnRequest::utterance("سلام", GvyaState::default()));
    assert_eq!(first_text(&out), Some("سلام"));
    assert_eq!(
        out.state.conversation.active_language.as_deref(),
        Some("fa")
    );
}

#[test]
fn matched_language_switches_session_and_persists_across_fallback() {
    let mut response = ResponseDefinition::text("hello.response", "en", "Hello");
    response.texts.push(LocalizedTexts {
        language: "fa".to_owned(),
        variants: vec!["سلام".to_owned()],
    });
    let mut fallback = ResponseDefinition::text("fallback.response", "en", "Fallback");
    fallback.texts.push(LocalizedTexts {
        language: "fa".to_owned(),
        variants: vec!["نامفهوم".to_owned()],
    });
    let mut pattern = meaning_pattern("hello", ["placeholder"]);
    pattern.samples = vec![
        LocalizedSample::new("en", "hello"),
        LocalizedSample::new("fa", "سلام"),
    ];
    let kernel = build_kernel(
        vec![pattern],
        vec![behavior("hello.behavior", "hello", response)],
        Vec::new(),
        vec![fallback],
        Vec::new(),
        ConversationConfig::default(),
    );

    let first = kernel.respond(KernelTurnRequest::utterance("سلام", GvyaState::default()));
    assert_eq!(first_text(&first), Some("سلام"));
    assert_eq!(
        first.state.conversation.active_language.as_deref(),
        Some("fa")
    );

    let second = kernel.respond(KernelTurnRequest::utterance("--#--", first.state));
    assert_eq!(first_text(&second), Some("نامفهوم"));
    assert_eq!(
        second.state.conversation.active_language.as_deref(),
        Some("fa")
    );

    let third = kernel.respond(KernelTurnRequest::utterance("hello", second.state));
    assert_eq!(first_text(&third), Some("Hello"));
    assert_eq!(
        third.state.conversation.active_language.as_deref(),
        Some("en")
    );
}

#[test]
fn simple_math_result_is_explicitly_projected_into_system_template_context() {
    let k = build_kernel(
        Vec::new(),
        Vec::new(),
        Vec::new(),
        vec![ResponseDefinition::text(
            "math.fallback",
            "en",
            "{{ system.mathResult | unknown }}",
        )],
        Vec::new(),
        ConversationConfig::default(),
    );
    let out = k.respond(ConversationTurnRequest::utterance(
        "2 + 3",
        GvyaState::default(),
    ));
    assert_eq!(first_text(&out), Some("5"));
}

#[test]
fn opening_can_activate_topic_and_render_response() {
    let opening = OpeningDefinition {
        id: gvya_model::OpeningId::new("start"),
        topic: Some(TopicId::new("welcome")),
        topic_ttl: Some(4),
        responses: vec![ResponseDefinition::text("start.response", "en", "Welcome")],
    };
    let k = build_kernel(
        Vec::new(),
        Vec::new(),
        vec![opening],
        Vec::new(),
        Vec::new(),
        ConversationConfig::default(),
    );
    let out = k.open(ConversationOpenRequest {
        language: Some("en".to_string()),
        context: ContextSnapshot {
            values: BTreeMap::new(),
            visible_references: Vec::new(),
            available_capabilities: Vec::new(),
        },
        state: GvyaState::default(),
        system: BTreeMap::new(),
        language_policy: LanguagePolicy::default(),
        seed: Some(1),
    });
    assert_eq!(out.mode, ConversationMode::Opening);
    assert_eq!(first_text(&out), Some("Welcome"));
    assert_eq!(
        out.state
            .conversation
            .active_topic
            .as_ref()
            .map(|t| t.id.as_str()),
        Some("welcome")
    );
    assert_eq!(
        out.state.conversation.active_topic.as_ref().map(|t| t.ttl),
        Some(4)
    );
}

#[test]
fn fallback_behavior_conditions_drive_personality_and_apply_effects() {
    let mut response = ResponseDefinition::text(
        "angry.fallback.response",
        "en",
        "I am not in the mood for this.",
    );
    response.effects.push(ConversationEffect::Increment {
        target: StateTarget::Author("angryFallbacks".to_string()),
        delta: 1.0,
    });
    response.opens_followup = Some(FollowupDirective {
        id: FollowupId::new("angry.fallback.followup"),
        ttl: 3,
        refresh_if_same: false,
    });
    let fallback = FallbackBehavior {
        id: BehaviorId::new("angry.unknown"),
        trigger: FallbackTrigger::Unresolved,
        priority: 100,
        conditions: vec![ValueCondition {
            path: ValuePath::author("anger"),
            op: PredicateOp::GreaterOrEqual,
            value: Some(Value::Number(70.0)),
        }],
        responses: vec![response],
    };
    let semantic = SemanticCatalog::new(Vec::new()).expect("semantic catalog");
    let catalog = ConversationCatalog::new(Vec::new(), Vec::new(), Vec::new(), vec![fallback])
        .expect("conversation catalog");
    let k = ConversationKernel::new(
        semantic,
        empty_test_profiles(),
        SemanticConfig::default(),
        catalog,
        ConversationConfig::default(),
    )
    .expect("valid conversation config");
    let mut state = GvyaState::default();
    state.author.insert("anger".into(), Value::Number(80.0));
    let out = k.respond(ConversationTurnRequest::utterance("unknown words", state));
    assert_eq!(out.mode, ConversationMode::Fallback);
    assert_eq!(
        out.behavior.as_ref().map(BehaviorId::as_str),
        Some("angry.unknown")
    );
    assert_eq!(first_text(&out), Some("I am not in the mood for this."));
    assert_eq!(out.state.conversation.repair.consecutive, 1);
    assert_eq!(
        out.state.author.get("angryFallbacks"),
        Some(&Value::Number(1.0))
    );
    assert_eq!(
        out.state
            .conversation
            .active_followup
            .as_ref()
            .map(|f| f.id.as_str()),
        Some("angry.fallback.followup")
    );
}

#[test]
fn last_topic_survives_active_topic_expiry_for_templates() {
    let opening = OpeningDefinition {
        id: gvya_model::OpeningId::new("start"),
        topic: Some(TopicId::new("maintenance")),
        topic_ttl: Some(1),
        responses: vec![ResponseDefinition::text("start.response", "en", "Ready")],
    };
    let k = build_kernel(
        Vec::new(),
        Vec::new(),
        vec![opening],
        vec![ResponseDefinition::text(
            "fallback.topic",
            "en",
            "{{ conversation.lastTopic | none }}",
        )],
        Vec::new(),
        ConversationConfig::default(),
    );
    let opened = k.open(ConversationOpenRequest {
        language: Some("en".to_string()),
        context: ContextSnapshot {
            values: BTreeMap::new(),
            visible_references: Vec::new(),
            available_capabilities: Vec::new(),
        },
        state: GvyaState::default(),
        system: BTreeMap::new(),
        language_policy: LanguagePolicy::default(),
        seed: Some(1),
    });
    assert_eq!(
        opened
            .state
            .conversation
            .active_topic
            .as_ref()
            .map(|t| t.ttl),
        Some(1)
    );
    let out = k.respond(ConversationTurnRequest::utterance("unknown", opened.state));
    assert!(out.state.conversation.active_topic.is_none());
    assert_eq!(
        out.state
            .conversation
            .last_topic
            .as_ref()
            .map(TopicId::as_str),
        Some("maintenance")
    );
    assert_eq!(first_text(&out), Some("maintenance"));
}

#[test]
fn configured_style_lexicon_is_wired_into_turn_state_and_templates() {
    let semantic = SemanticCatalog::new(vec![meaning_pattern("hello", ["hello please"])])
        .expect("semantic catalog");
    let catalog = ConversationCatalog::new(
        vec![behavior(
            "hello.behavior",
            "hello",
            ResponseDefinition::text(
                "hello.response",
                "en",
                "{{ conversation.userStyle.formality }}",
            ),
        )],
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .expect("conversation catalog")
    .with_style_lexicon(StyleLexicon {
        formal_terms: vec!["please".to_string()],
        informal_terms: vec!["yo".to_string()],
    });
    let k = ConversationKernel::new(
        semantic,
        empty_test_profiles(),
        SemanticConfig::default(),
        catalog,
        ConversationConfig::default(),
    )
    .expect("valid conversation config");
    let out = k.respond(ConversationTurnRequest::utterance(
        "hello please",
        GvyaState::default(),
    ));
    assert_eq!(
        out.state.conversation.user_style.formality,
        gvya_model::Formality::Formal
    );
    assert_eq!(first_text(&out), Some("formal"));
}

#[test]
fn duplicate_links_are_not_emitted_twice() {
    let mut response = ResponseDefinition::text("links.response", "en", "See this");
    response.links = vec![
        ResponseLink {
            label: "A".into(),
            url: "https://example.com/X".into(),
        },
        ResponseLink {
            label: "B".into(),
            url: "https://EXAMPLE.com/x".into(),
        },
    ];
    let k = build_kernel(
        vec![meaning_pattern("links", ["links"])],
        vec![behavior("links.behavior", "links", response)],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        ConversationConfig::default(),
    );
    let out = k.respond(ConversationTurnRequest::utterance(
        "links",
        GvyaState::default(),
    ));
    let links = out.response.messages[0]
        .items
        .iter()
        .filter(|item| matches!(item, ResponseItem::Link { .. }))
        .count();
    assert_eq!(links, 1);
}

#[test]
fn below_threshold_candidate_answers_only_when_behavior_is_explicitly_repair_eligible() {
    let semantic =
        SemanticCatalog::new(vec![meaning_pattern("gvya.about", ["tell me about gvya"])]).unwrap();
    let mut repair = behavior(
        "gvya.about.behavior",
        "gvya.about",
        ResponseDefinition::text("about", "en", "about gvya"),
    );
    repair.repair_continuation_candidate = true;
    let catalog = ConversationCatalog::new(
        vec![repair],
        Vec::new(),
        Vec::new(),
        vec![FallbackBehavior {
            id: BehaviorId::new("fallback"),
            trigger: FallbackTrigger::Unresolved,
            priority: 0,
            conditions: Vec::new(),
            responses: vec![ResponseDefinition::text(
                "fallback.response",
                "en",
                "fallback",
            )],
        }],
    )
    .unwrap();
    let mut semantic_config = SemanticConfig::default();
    semantic_config.resolution_threshold = 0.99;
    let mut config = ConversationConfig::default();
    config.repair_candidate_min_score = 0.01;
    let k = ConversationKernel::new(
        semantic.clone(),
        conversation_test_profile(),
        semantic_config.clone(),
        catalog,
        config.clone(),
    )
    .unwrap();
    let out = k.respond(ConversationTurnRequest::utterance(
        "gvya",
        GvyaState::default(),
    ));
    assert_eq!(out.mode, ConversationMode::RepairContinuation);
    assert_eq!(first_text(&out), Some("about gvya"));

    let strict = behavior(
        "gvya.about.strict",
        "gvya.about",
        ResponseDefinition::text("strict", "en", "strict"),
    );
    let catalog = ConversationCatalog::new(
        vec![strict],
        Vec::new(),
        Vec::new(),
        vec![FallbackBehavior {
            id: BehaviorId::new("fallback"),
            trigger: FallbackTrigger::Unresolved,
            priority: 0,
            conditions: Vec::new(),
            responses: vec![ResponseDefinition::text(
                "fallback.response2",
                "en",
                "fallback",
            )],
        }],
    )
    .unwrap();
    let k = ConversationKernel::new(
        semantic,
        conversation_test_profile(),
        semantic_config,
        catalog,
        config,
    )
    .unwrap();
    let out = k.respond(ConversationTurnRequest::utterance(
        "gvya",
        GvyaState::default(),
    ));
    assert_eq!(out.mode, ConversationMode::Fallback);
    assert_eq!(
        out.state
            .conversation
            .repair
            .last_candidate
            .as_ref()
            .map(MeaningId::as_str),
        Some("gvya.about")
    );
}

#[test]
fn repair_candidate_cannot_bypass_normal_behavior_eligibility() {
    let semantic =
        SemanticCatalog::new(vec![meaning_pattern("secret", ["tell me secret"])]).unwrap();
    let mut repair = behavior(
        "secret.behavior",
        "secret",
        ResponseDefinition::text("secret.response", "en", "secret"),
    );
    repair.repair_continuation_candidate = true;
    repair.requires_values.push(ValueRequirement {
        path: ValuePath::author("allowed"),
        value: Value::Bool(true),
    });
    let catalog = ConversationCatalog::new(
        vec![repair],
        Vec::new(),
        Vec::new(),
        vec![FallbackBehavior {
            id: BehaviorId::new("fallback"),
            trigger: FallbackTrigger::Unresolved,
            priority: 0,
            conditions: Vec::new(),
            responses: vec![ResponseDefinition::text(
                "fallback.response",
                "en",
                "fallback",
            )],
        }],
    )
    .unwrap();
    let mut semantic_config = SemanticConfig::default();
    semantic_config.resolution_threshold = 0.99;
    let mut config = ConversationConfig::default();
    config.repair_candidate_min_score = 0.01;
    let k = ConversationKernel::new(
        semantic,
        conversation_test_profile(),
        semantic_config,
        catalog,
        config,
    )
    .unwrap();
    let out = k.respond(ConversationTurnRequest::utterance(
        "secret",
        GvyaState::default(),
    ));
    assert_eq!(out.mode, ConversationMode::Fallback);
}

#[test]
fn repair_candidate_preserves_required_entity_slot_binding() {
    let mut pattern = meaning_pattern("temperature.set", ["set temperature 22"]);
    pattern.slots.push(SlotSpec {
        name: "temperature".into(),
        kind: SlotKind::Number,
        required: true,
        elicitation: vec![ElicitationPrompt::new("en", "What temperature?")],
    });
    let semantic = SemanticCatalog::new(vec![pattern]).unwrap();
    let mut repair = behavior(
        "temperature.behavior",
        "temperature.set",
        ResponseDefinition::text("temperature.response", "en", "ok"),
    );
    repair.repair_continuation_candidate = true;
    let catalog = ConversationCatalog::new(
        vec![repair],
        Vec::new(),
        Vec::new(),
        vec![FallbackBehavior {
            id: BehaviorId::new("fallback"),
            trigger: FallbackTrigger::Unresolved,
            priority: 0,
            conditions: Vec::new(),
            responses: vec![ResponseDefinition::text(
                "fallback.response",
                "en",
                "fallback",
            )],
        }],
    )
    .unwrap();
    let mut semantic_config = SemanticConfig::default();
    semantic_config.resolution_threshold = 1.0;
    let mut config = ConversationConfig::default();
    config.repair_candidate_min_score = 0.01;
    let k = ConversationKernel::new(
        semantic,
        conversation_test_profile(),
        semantic_config,
        catalog,
        config,
    )
    .unwrap();
    let out = k.respond(ConversationTurnRequest::utterance(
        "temperature 22",
        GvyaState::default(),
    ));
    assert_eq!(out.mode, ConversationMode::RepairContinuation);
    let meaning = out.meaning.expect("repair meaning");
    assert!(
        meaning
            .slots
            .iter()
            .any(|slot| slot.name == "temperature" && slot.value == Value::Number(22.0))
    );
}

#[test]
fn generic_continuation_can_resume_prior_recorded_repair_candidate() {
    let semantic =
        SemanticCatalog::new(vec![meaning_pattern("gvya.about", ["about gvya"])]).unwrap();
    let mut repair = behavior(
        "gvya.about.behavior",
        "gvya.about",
        ResponseDefinition::text("about", "en", "about gvya"),
    );
    repair.repair_continuation_candidate = true;
    let catalog =
        ConversationCatalog::new(vec![repair], Vec::new(), Vec::new(), Vec::new()).unwrap();
    let k = ConversationKernel::new(
        semantic,
        conversation_test_profile(),
        SemanticConfig::default(),
        catalog,
        ConversationConfig::default(),
    )
    .unwrap();
    let mut state = GvyaState::default();
    state.conversation.repair.consecutive = 1;
    state.conversation.repair.last_mode = Some("fallback".into());
    state.conversation.repair.last_candidate = Some(MeaningId::new("gvya.about"));
    let out = k.respond(ConversationTurnRequest::utterance("tell me more", state));
    assert_eq!(out.mode, ConversationMode::RepairContinuation);
    assert_eq!(first_text(&out), Some("about gvya"));
}

#[test]
fn prior_repair_candidate_with_required_slots_does_not_resume_without_rebinding() {
    let mut pattern = meaning_pattern("temperature.set", ["set temperature 22"]);
    pattern.slots.push(SlotSpec {
        name: "temperature".into(),
        kind: SlotKind::Number,
        required: true,
        elicitation: vec![ElicitationPrompt::new("en", "What temperature?")],
    });
    let semantic = SemanticCatalog::new(vec![pattern]).unwrap();
    let mut candidate = behavior(
        "temperature.behavior",
        "temperature.set",
        ResponseDefinition::text("temperature.response", "en", "setting it"),
    );
    candidate.repair_continuation_candidate = true;
    let catalog = ConversationCatalog::new(
        vec![candidate],
        Vec::new(),
        Vec::new(),
        vec![FallbackBehavior {
            id: BehaviorId::new("fallback"),
            trigger: FallbackTrigger::Unresolved,
            priority: 0,
            conditions: Vec::new(),
            responses: vec![ResponseDefinition::text(
                "fallback.response",
                "en",
                "fallback",
            )],
        }],
    )
    .unwrap();
    let k = ConversationKernel::new(
        semantic,
        conversation_test_profile(),
        SemanticConfig::default(),
        catalog,
        ConversationConfig::default(),
    )
    .unwrap();
    let mut state = GvyaState::default();
    state.conversation.repair.consecutive = 1;
    state.conversation.repair.last_mode = Some("fallback".into());
    state.conversation.repair.last_candidate = Some(MeaningId::new("temperature.set"));
    let out = k.respond(ConversationTurnRequest::utterance("tell me more", state));
    assert_eq!(out.mode, ConversationMode::Fallback);
    assert_eq!(first_text(&out), Some("fallback"));
}

#[test]
fn behavior_repeat_threshold_can_delay_identical_input_repeat_stage() {
    let normal = ResponseDefinition::text("ping.normal", "en", "normal");
    let mut repeat = ResponseDefinition::text("ping.repeat", "en", "repeat");
    repeat.repeat_stage = Some(RepeatStage::Repeat);
    let mut annoyed = ResponseDefinition::text("ping.annoyed", "en", "annoyed");
    annoyed.repeat_stage = Some(RepeatStage::Annoyed);
    let mut final_repeat = ResponseDefinition::text("ping.final", "en", "final");
    final_repeat.repeat_stage = Some(RepeatStage::Final);
    let mut b = behavior("ping.behavior", "ping", normal);
    b.responses.extend([repeat, annoyed, final_repeat]);
    b.repeat_same_input_after = Some(4);
    b.repeat_same_meaning_after = Some(4);
    let mut config = ConversationConfig::default();
    config.repeat_detection_threshold = 2;
    let k = build_kernel(
        vec![meaning_pattern("ping", ["ping"])],
        vec![b],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        config,
    );
    let one = k.respond(ConversationTurnRequest::utterance(
        "ping",
        GvyaState::default(),
    ));
    assert_eq!(first_text(&one), Some("normal"));
    let two = k.respond(ConversationTurnRequest::utterance("ping", one.state));
    assert_eq!(first_text(&two), Some("normal"));
    let three = k.respond(ConversationTurnRequest::utterance("ping", two.state));
    assert_eq!(first_text(&three), Some("normal"));
    let four = k.respond(ConversationTurnRequest::utterance("ping", three.state));
    assert_eq!(first_text(&four), Some("repeat"));
}
