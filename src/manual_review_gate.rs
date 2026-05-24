use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewGateStage {
    Created,
    SourceClassified,
    SelectorChecked,
    ContextChecked,
    IndependentVerificationChecked,
    OperatorDecisionMade,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewGateDecision {
    Pending,
    Reject,
    KeepAsQuestionable,
    RequireMoreVerification,
    PromoteToProbable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SelectorMatchQuality {
    NotChecked,
    NoMatch,
    SimilarOnly,
    ExactMatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewableSourceTrust {
    Unknown,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualReviewGateCard {
    pub review_id: String,
    pub selector_type: String,
    pub selector_masked: String,
    pub source_id: String,
    pub stage: ReviewGateStage,
    pub source_trust: ReviewableSourceTrust,
    pub selector_match_quality: SelectorMatchQuality,
    pub has_date_hint: bool,
    pub has_context_near_selector: bool,
    pub has_independent_allowed_confirmation: bool,
    pub decision: ReviewGateDecision,
    pub confidence_cap: u8,
    pub raw_record_stored: bool,
    pub raw_record_visible: bool,
    pub contributes_to_main_confidence: bool,
    pub identity_confirmation_allowed: bool,
    pub operator_notes_sanitized: Option<String>,
    pub required_manual_steps: Vec<String>,
    pub decision_rules: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManualReviewGateReport {
    pub cards: Vec<ManualReviewGateCard>,
    pub pending_count: usize,
    pub rejected_count: usize,
    pub questionable_count: usize,
    pub more_verification_count: usize,
    pub probable_count: usize,
}

pub fn create_manual_review_card(
    review_id: &str,
    selector_type: &str,
    selector_masked: &str,
    source_id: &str,
) -> ManualReviewGateCard {
    ManualReviewGateCard {
        review_id: review_id.to_string(),
        selector_type: selector_type.to_string(),
        selector_masked: selector_masked.to_string(),
        source_id: source_id.to_string(),
        stage: ReviewGateStage::Created,
        source_trust: ReviewableSourceTrust::Unknown,
        selector_match_quality: SelectorMatchQuality::NotChecked,
        has_date_hint: false,
        has_context_near_selector: false,
        has_independent_allowed_confirmation: false,
        decision: ReviewGateDecision::Pending,
        confidence_cap: 0,
        raw_record_stored: false,
        raw_record_visible: false,
        contributes_to_main_confidence: false,
        identity_confirmation_allowed: false,
        operator_notes_sanitized: None,
        required_manual_steps: vec![
            "Classify the source before using the signal.".to_string(),
            "Confirm whether the selector match is exact, similar, or absent.".to_string(),
            "Check whether the source has a visible date or time hint.".to_string(),
            "Check whether there is meaningful context near the selector.".to_string(),
            "Look for independent confirmation only in allowed public or user-provided sources.".to_string(),
            "Record only sanitized notes; do not copy raw records into the project.".to_string(),
        ],
        decision_rules: vec![
            "No exact selector match => Reject.".to_string(),
            "Exact match without context/date/independent confirmation => KeepAsQuestionable or RequireMoreVerification.".to_string(),
            "Exact match plus independent allowed confirmation => PromoteToProbable.".to_string(),
            "Untrusted source alone must never confirm identity.".to_string(),
        ],
        warnings: vec![
            "This card is an operator review gate, not an automated lookup.".to_string(),
            "Raw records are not stored or shown.".to_string(),
            "Presence of a selector in an untrusted source does not prove current ownership or identity.".to_string(),
        ],
    }
}

pub fn classify_source(card: &mut ManualReviewGateCard, source_trust: ReviewableSourceTrust) {
    card.source_trust = source_trust;
    card.stage = ReviewGateStage::SourceClassified;
    recalculate_decision(card);
}

pub fn confirm_selector_match(card: &mut ManualReviewGateCard, quality: SelectorMatchQuality) {
    card.selector_match_quality = quality;
    card.stage = ReviewGateStage::SelectorChecked;
    recalculate_decision(card);
}

pub fn confirm_context(
    card: &mut ManualReviewGateCard,
    has_date_hint: bool,
    has_context_near_selector: bool,
    sanitized_notes: Option<String>,
) {
    card.has_date_hint = has_date_hint;
    card.has_context_near_selector = has_context_near_selector;
    card.operator_notes_sanitized = sanitized_notes.map(|notes| sanitize_operator_notes(&notes));
    card.stage = ReviewGateStage::ContextChecked;
    recalculate_decision(card);
}

pub fn confirm_independent_verification(card: &mut ManualReviewGateCard, has_confirmation: bool) {
    card.has_independent_allowed_confirmation = has_confirmation;
    card.stage = ReviewGateStage::IndependentVerificationChecked;
    recalculate_decision(card);
}

pub fn force_operator_decision(card: &mut ManualReviewGateCard, decision: ReviewGateDecision, sanitized_notes: Option<String>) {
    card.decision = decision;
    card.operator_notes_sanitized = sanitized_notes.map(|notes| sanitize_operator_notes(&notes));
    card.stage = ReviewGateStage::OperatorDecisionMade;
    apply_decision_caps(card);
}

fn recalculate_decision(card: &mut ManualReviewGateCard) {
    card.decision = match card.selector_match_quality {
        SelectorMatchQuality::NoMatch => ReviewGateDecision::Reject,
        SelectorMatchQuality::SimilarOnly => ReviewGateDecision::RequireMoreVerification,
        SelectorMatchQuality::NotChecked => ReviewGateDecision::Pending,
        SelectorMatchQuality::ExactMatch => {
            if card.has_independent_allowed_confirmation {
                ReviewGateDecision::PromoteToProbable
            } else if card.has_date_hint && card.has_context_near_selector {
                ReviewGateDecision::RequireMoreVerification
            } else {
                ReviewGateDecision::KeepAsQuestionable
            }
        }
    };
    apply_decision_caps(card);
}

fn apply_decision_caps(card: &mut ManualReviewGateCard) {
    match card.decision {
        ReviewGateDecision::Pending => {
            card.confidence_cap = 0;
            card.contributes_to_main_confidence = false;
            card.identity_confirmation_allowed = false;
        }
        ReviewGateDecision::Reject => {
            card.confidence_cap = 0;
            card.contributes_to_main_confidence = false;
            card.identity_confirmation_allowed = false;
        }
        ReviewGateDecision::KeepAsQuestionable => {
            card.confidence_cap = 20;
            card.contributes_to_main_confidence = false;
            card.identity_confirmation_allowed = false;
        }
        ReviewGateDecision::RequireMoreVerification => {
            card.confidence_cap = 35;
            card.contributes_to_main_confidence = false;
            card.identity_confirmation_allowed = false;
        }
        ReviewGateDecision::PromoteToProbable => {
            card.confidence_cap = 60;
            card.contributes_to_main_confidence = true;
            card.identity_confirmation_allowed = false;
        }
    }
    card.raw_record_stored = false;
    card.raw_record_visible = false;
}

pub fn build_manual_review_gate_report(cards: Vec<ManualReviewGateCard>) -> ManualReviewGateReport {
    ManualReviewGateReport {
        pending_count: cards.iter().filter(|card| card.decision == ReviewGateDecision::Pending).count(),
        rejected_count: cards.iter().filter(|card| card.decision == ReviewGateDecision::Reject).count(),
        questionable_count: cards.iter().filter(|card| card.decision == ReviewGateDecision::KeepAsQuestionable).count(),
        more_verification_count: cards.iter().filter(|card| card.decision == ReviewGateDecision::RequireMoreVerification).count(),
        probable_count: cards.iter().filter(|card| card.decision == ReviewGateDecision::PromoteToProbable).count(),
        cards,
    }
}

pub fn save_manual_review_gate_report(report: &ManualReviewGateReport, path: &str) -> Result<(), String> {
    let json = serde_json::to_string_pretty(report).map_err(|err| format!("serialize manual review gate report: {}", err))?;
    std::fs::write(path, json).map_err(|err| format!("write {}: {}", path, err))
}

fn sanitize_operator_notes(notes: &str) -> String {
    notes
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .take(8)
        .collect::<Vec<_>>()
        .join(" | ")
        .chars()
        .take(800)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match_without_confirmation_stays_questionable() {
        let mut card = create_manual_review_card("r1", "phone", "+000***00", "manual_source");
        classify_source(&mut card, ReviewableSourceTrust::Low);
        confirm_selector_match(&mut card, SelectorMatchQuality::ExactMatch);
        assert_eq!(card.decision, ReviewGateDecision::KeepAsQuestionable);
        assert!(!card.contributes_to_main_confidence);
        assert!(!card.identity_confirmation_allowed);
    }

    #[test]
    fn independent_confirmation_promotes_only_to_probable() {
        let mut card = create_manual_review_card("r2", "phone", "+000***00", "manual_source");
        confirm_selector_match(&mut card, SelectorMatchQuality::ExactMatch);
        confirm_context(&mut card, true, true, Some("visible date and context checked".to_string()));
        confirm_independent_verification(&mut card, true);
        assert_eq!(card.decision, ReviewGateDecision::PromoteToProbable);
        assert!(card.contributes_to_main_confidence);
        assert!(!card.identity_confirmation_allowed);
        assert_eq!(card.confidence_cap, 60);
        assert!(!card.raw_record_stored);
        assert!(!card.raw_record_visible);
    }

    #[test]
    fn no_match_rejects() {
        let mut card = create_manual_review_card("r3", "email", "u***@example.test", "manual_source");
        confirm_selector_match(&mut card, SelectorMatchQuality::NoMatch);
        assert_eq!(card.decision, ReviewGateDecision::Reject);
        assert_eq!(card.confidence_cap, 0);
    }
}
