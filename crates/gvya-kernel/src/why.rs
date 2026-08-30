//! Progressive-disclosure Why model built from canonical semantic/conversation/capability traces.
//!
//! Why is a first-class projection over stable trace codes. It deliberately does not expose a raw
//! log wall as the primary author experience; raw traces remain available separately.

use std::collections::{BTreeMap, BTreeSet};

use gvya_model::{
    Trace, TraceEvent, TraceId, WhyEntry, WhyEntryStatus, WhyReport, WhySection, WhySectionId,
    WhySectionKind,
};

const MAX_EVENTS: usize = 256;
const MAX_SECTION_ENTRIES: usize = 96;
const MAX_SUMMARY_ENTRIES: usize = 12;

#[must_use]
pub fn build_why_report(traces: &[&Trace]) -> WhyReport {
    let mut trace_ids = Vec::new();
    let mut seen_trace_ids = BTreeSet::new();
    let mut grouped: BTreeMap<WhySectionKind, Vec<WhyEntry>> = BTreeMap::new();
    let mut rejected_count = 0usize;
    let mut seen_events = 0usize;
    let mut summary_entries = Vec::new();

    for trace in traces {
        if seen_trace_ids.insert(trace.id.as_str().to_owned()) {
            trace_ids.push(TraceId::new(trace.id.as_str()));
        }
        for event in &trace.events {
            if seen_events >= MAX_EVENTS {
                break;
            }
            seen_events += 1;
            let status = status_for_event(event);
            if status == WhyEntryStatus::Rejected {
                rejected_count += 1;
            }
            let entry = WhyEntry {
                code: event.code.clone(),
                status,
                summary: event.summary.clone(),
                visibility: event.visibility,
                details: event.details.clone(),
            };
            if summary_worthy(event) && summary_entries.len() < MAX_SUMMARY_ENTRIES {
                summary_entries.push(entry.clone());
            }
            let kind = section_for_event(event, status);
            let entries = grouped.entry(kind).or_default();
            if entries.len() >= MAX_SECTION_ENTRIES {
                continue;
            }
            entries.push(entry);
        }
    }

    let mut sections = Vec::new();
    if !summary_entries.is_empty() {
        sections.push(WhySection {
            id: WhySectionId::new("summary"),
            kind: WhySectionKind::Summary,
            title: "Summary".to_owned(),
            entries: summary_entries,
        });
    }
    for kind in section_order() {
        let Some(entries) = grouped.remove(&kind) else {
            continue;
        };
        if entries.is_empty() {
            continue;
        }
        sections.push(WhySection {
            id: WhySectionId::new(section_id(kind)),
            kind,
            title: section_title(kind).to_owned(),
            entries,
        });
    }

    let headline = if rejected_count > 0 {
        format!("Decision completed with {rejected_count} rejected or blocked path(s).")
    } else if seen_events == 0 {
        "No decision trace is available.".to_owned()
    } else {
        "Decision completed without a rejected path in the visible trace.".to_owned()
    };

    WhyReport {
        headline,
        sections,
        trace_ids,
        rejected_count,
    }
}

fn section_order() -> [WhySectionKind; 7] {
    [
        WhySectionKind::Rejections,
        WhySectionKind::Understanding,
        WhySectionKind::Conversation,
        WhySectionKind::Capability,
        WhySectionKind::Context,
        WhySectionKind::Selected,
        WhySectionKind::Other,
    ]
}

fn summary_worthy(event: &TraceEvent) -> bool {
    let code = event.code.as_str();
    code.starts_with("semantic.decision.")
        || matches!(
            code,
            "conversation.response.selected"
                | "capability.admitted"
                | "capability.confirmation_required"
                | "capability.policy_rejected"
                | "capability.unavailable"
                | "capability.result_accepted"
                | "capability.result_rejected"
                | "capability.result_schema_rejected"
        )
}

fn section_for_event(event: &TraceEvent, status: WhyEntryStatus) -> WhySectionKind {
    if status == WhyEntryStatus::Rejected {
        return WhySectionKind::Rejections;
    }
    let code = event.code.as_str();
    if code.starts_with("semantic.") {
        return WhySectionKind::Understanding;
    }
    if code.starts_with("capability.") {
        return WhySectionKind::Capability;
    }
    if code.starts_with("conversation.topic") || code.starts_with("conversation.followup") {
        return WhySectionKind::Context;
    }
    if code.starts_with("conversation.response") || code.starts_with("conversation.opening") {
        return WhySectionKind::Selected;
    }
    if code.starts_with("conversation.") {
        return WhySectionKind::Conversation;
    }
    WhySectionKind::Other
}

fn status_for_event(event: &TraceEvent) -> WhyEntryStatus {
    let code = event.code.as_str();
    if contains_any(
        code,
        &[
            "rejected",
            "unavailable",
            "ineligible",
            "unresolved",
            "stale",
            "declined",
            "ambiguous",
            ".miss",
            "blocked",
        ],
    ) {
        return WhyEntryStatus::Rejected;
    }
    if code.contains("confirmation_required") || code.ends_with(".required") {
        return WhyEntryStatus::Required;
    }
    if contains_any(code, &["response.selected", "opening.selected"]) {
        return WhyEntryStatus::Selected;
    }
    if contains_any(code, &[".accepted", ".admitted", ".resolved", ".bound"]) {
        return WhyEntryStatus::Accepted;
    }
    WhyEntryStatus::Information
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn section_id(kind: WhySectionKind) -> &'static str {
    match kind {
        WhySectionKind::Summary => "summary",
        WhySectionKind::Rejections => "rejections",
        WhySectionKind::Understanding => "understanding",
        WhySectionKind::Conversation => "conversation",
        WhySectionKind::Capability => "capability",
        WhySectionKind::Context => "context",
        WhySectionKind::Selected => "selected",
        WhySectionKind::Other => "other",
    }
}

fn section_title(kind: WhySectionKind) -> &'static str {
    match kind {
        WhySectionKind::Summary => "Summary",
        WhySectionKind::Rejections => "Rejected / blocked paths",
        WhySectionKind::Understanding => "Understanding",
        WhySectionKind::Conversation => "Conversation",
        WhySectionKind::Capability => "Capabilities",
        WhySectionKind::Context => "Conversation context",
        WhySectionKind::Selected => "Selected response",
        WhySectionKind::Other => "Other diagnostics",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gvya_model::{TraceCode, TraceEvent, TraceVisibility};

    fn event(code: &str, phase: &str, summary: &str) -> TraceEvent {
        TraceEvent {
            code: TraceCode::new(code),
            phase: phase.to_owned(),
            summary: summary.to_owned(),
            visibility: TraceVisibility::Author,
            details: BTreeMap::new(),
        }
    }

    #[test]
    fn rejections_are_promoted_before_detail_sections() {
        let trace = Trace {
            id: TraceId::new("turn-1"),
            events: vec![
                event("semantic.evidence.ranked", "semantic", "ranked"),
                event("capability.policy_rejected", "capability", "denied"),
                event("conversation.response.selected", "conversation", "selected"),
            ],
        };
        let report = build_why_report(&[&trace]);
        assert_eq!(report.rejected_count, 1);
        assert_eq!(report.sections[0].kind, WhySectionKind::Summary);
        assert!(
            report
                .sections
                .iter()
                .any(|section| section.kind == WhySectionKind::Rejections)
        );
        assert!(
            report
                .sections
                .iter()
                .any(|section| section.kind == WhySectionKind::Understanding)
        );
        assert!(
            report
                .sections
                .iter()
                .any(|section| section.kind == WhySectionKind::Selected)
        );
    }

    #[test]
    fn trace_identity_is_deduplicated_without_dropping_events() {
        let trace = Trace {
            id: TraceId::new("same"),
            events: vec![event(
                "conversation.topic.preference",
                "conversation",
                "topic",
            )],
        };
        let report = build_why_report(&[&trace, &trace]);
        assert_eq!(report.trace_ids.len(), 1);
        assert_eq!(report.sections[0].entries.len(), 2);
    }
}
