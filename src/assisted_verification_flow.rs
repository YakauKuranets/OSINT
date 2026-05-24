use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssistedPlatform {
    Telegram,
    Viber,
    WhatsApp,
    Vk,
    Max,
    PublicWeb,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssistedStepKind {
    NormalizeSelector,
    OpenOfficialInterface,
    OperatorManualCheck,
    ContextAssessment,
    IndependentSourceCheck,
    SanitizedResultEntry,
    Decision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssistedStepStatus {
    Pending,
    ReadyForOperator,
    CompletedByOperator,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssistedDecision {
    Pending,
    NoMatch,
    SimilarOnly,
    FoundNeedsContextCheck,
    Inconclusive,
    CorroboratedLead,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistedVerificationStep {
    pub step_id: String,
    pub platform: AssistedPlatform,
    pub kind: AssistedStepKind,
    pub status: AssistedStepStatus,
    pub selector_masked: String,
    pub official_action_hint: Option<String>,
    pub operator_instruction: String,
    pub required_operator_input: Vec<String>,
    pub forbidden_actions: Vec<String>,
    pub raw_data_stored: bool,
    pub automated_account_discovery: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistedVerificationFlow {
    pub flow_id: String,
    pub selector_type: String,
    pub selector_masked: String,
    pub steps: Vec<AssistedVerificationStep>,
    pub decision: AssistedDecision,
    pub confidence_cap: u8,
    pub contributes_to_main_confidence: bool,
    pub identity_confirmation_allowed: bool,
    pub raw_data_stored: bool,
    pub global_warnings: Vec<String>,
}

pub fn build_phone_assisted_verification_flow(raw_phone: &str) -> AssistedVerificationFlow {
    let selector_masked = mask_phone(raw_phone);
    let platforms = vec![
        AssistedPlatform::Telegram,
        AssistedPlatform::Viber,
        AssistedPlatform::WhatsApp,
        AssistedPlatform::Vk,
        AssistedPlatform::Max,
    ];

    let mut steps = vec![AssistedVerificationStep {
        step_id: "normalize_phone".to_string(),
        platform: AssistedPlatform::PublicWeb,
        kind: AssistedStepKind::NormalizeSelector,
        status: AssistedStepStatus::CompletedByOperator,
        selector_masked: selector_masked.clone(),
        official_action_hint: None,
        operator_instruction: "Phone selector was normalized and masked. Raw value is not stored in this flow.".to_string(),
        required_operator_input: vec!["confirm selector belongs to current authorized case scope".to_string()],
        forbidden_actions: default_forbidden_actions(),
        raw_data_stored: false,
        automated_account_discovery: false,
    }];

    for platform in platforms {
        steps.push(open_official_interface_step(platform.clone(), &selector_masked));
        steps.push(operator_manual_check_step(platform.clone(), &selector_masked));
        steps.push(context_assessment_step(platform.clone(), &selector_masked));
    }

    steps.push(AssistedVerificationStep {
        step_id: "independent_source_check".to_string(),
        platform: AssistedPlatform::PublicWeb,
        kind: AssistedStepKind::IndependentSourceCheck,
        status: AssistedStepStatus::ReadyForOperator,
        selector_masked: selector_masked.clone(),
        official_action_hint: None,
        operator_instruction: "Check whether the same selector or linked public identity appears in allowed public sources: marketplace, job profile, official registry, public social profile, or user-provided export.".to_string(),
        required_operator_input: vec![
            "independent_confirmation_found_yes_no".to_string(),
            "allowed_source_type".to_string(),
            "sanitized_summary_no_raw_profile_data".to_string(),
        ],
        forbidden_actions: default_forbidden_actions(),
        raw_data_stored: false,
        automated_account_discovery: false,
    });

    steps.push(AssistedVerificationStep {
        step_id: "operator_decision".to_string(),
        platform: AssistedPlatform::PublicWeb,
        kind: AssistedStepKind::Decision,
        status: AssistedStepStatus::ReadyForOperator,
        selector_masked: selector_masked.clone(),
        official_action_hint: None,
        operator_instruction: "Select final assisted verification decision: NoMatch, SimilarOnly, FoundNeedsContextCheck, Inconclusive, CorroboratedLead, or Rejected.".to_string(),
        required_operator_input: vec!["decision".to_string(), "operator_initials".to_string(), "checked_at".to_string(), "sanitized_note".to_string()],
        forbidden_actions: default_forbidden_actions(),
        raw_data_stored: false,
        automated_account_discovery: false,
    });

    AssistedVerificationFlow {
        flow_id: stable_flow_id(&selector_masked),
        selector_type: "phone".to_string(),
        selector_masked,
        steps,
        decision: AssistedDecision::Pending,
        confidence_cap: 0,
        contributes_to_main_confidence: false,
        identity_confirmation_allowed: false,
        raw_data_stored: false,
        global_warnings: vec![
            "This is assisted verification, not hidden automated probing.".to_string(),
            "Official clients/interfaces may be opened for the operator, but account discovery is not automated.".to_string(),
            "Operator records only sanitized result categories, not raw profile data.".to_string(),
            "Messenger presence never proves current ownership or identity by itself.".to_string(),
        ],
    }
}

pub fn apply_assisted_decision(flow: &mut AssistedVerificationFlow, decision: AssistedDecision) {
    flow.decision = decision;
    match flow.decision {
        AssistedDecision::Pending => {
            flow.confidence_cap = 0;
            flow.contributes_to_main_confidence = false;
        }
        AssistedDecision::NoMatch | AssistedDecision::Rejected => {
            flow.confidence_cap = 0;
            flow.contributes_to_main_confidence = false;
        }
        AssistedDecision::SimilarOnly | AssistedDecision::Inconclusive => {
            flow.confidence_cap = 15;
            flow.contributes_to_main_confidence = false;
        }
        AssistedDecision::FoundNeedsContextCheck => {
            flow.confidence_cap = 25;
            flow.contributes_to_main_confidence = false;
        }
        AssistedDecision::CorroboratedLead => {
            flow.confidence_cap = 55;
            flow.contributes_to_main_confidence = true;
        }
    }
    flow.identity_confirmation_allowed = false;
    flow.raw_data_stored = false;
}

fn open_official_interface_step(platform: AssistedPlatform, selector_masked: &str) -> AssistedVerificationStep {
    let name = platform_name(&platform);
    AssistedVerificationStep {
        step_id: format!("open_official_{}", name.to_lowercase()),
        platform,
        kind: AssistedStepKind::OpenOfficialInterface,
        status: AssistedStepStatus::ReadyForOperator,
        selector_masked: selector_masked.to_string(),
        official_action_hint: Some(format!("Open the official {} app/client manually. Do not use unofficial API or automated enumeration.", name)),
        operator_instruction: format!("Open {} through the official client or lawful public interface and prepare to check only the current authorized selector.", name),
        required_operator_input: vec!["opened_yes_no".to_string(), "blocked_or_unavailable_yes_no".to_string()],
        forbidden_actions: default_forbidden_actions(),
        raw_data_stored: false,
        automated_account_discovery: false,
    }
}

fn operator_manual_check_step(platform: AssistedPlatform, selector_masked: &str) -> AssistedVerificationStep {
    let name = platform_name(&platform);
    AssistedVerificationStep {
        step_id: format!("manual_check_{}", name.to_lowercase()),
        platform,
        kind: AssistedStepKind::OperatorManualCheck,
        status: AssistedStepStatus::ReadyForOperator,
        selector_masked: selector_masked.to_string(),
        official_action_hint: None,
        operator_instruction: format!("Manually check whether the selector appears reachable or associated in {}. Record only category: found / not_found / inconclusive / rejected.", name),
        required_operator_input: vec![
            "result_category".to_string(),
            "exact_match_yes_no".to_string(),
            "sanitized_summary_no_raw_data".to_string(),
        ],
        forbidden_actions: default_forbidden_actions(),
        raw_data_stored: false,
        automated_account_discovery: false,
    }
}

fn context_assessment_step(platform: AssistedPlatform, selector_masked: &str) -> AssistedVerificationStep {
    let name = platform_name(&platform);
    AssistedVerificationStep {
        step_id: format!("context_assessment_{}", name.to_lowercase()),
        platform,
        kind: AssistedStepKind::ContextAssessment,
        status: AssistedStepStatus::ReadyForOperator,
        selector_masked: selector_masked.to_string(),
        official_action_hint: None,
        operator_instruction: format!("Assess whether the {} result has lawful public context: public username, public display name, date hint, avatar consistency, or independent source. Do not copy private profile data.", name),
        required_operator_input: vec![
            "has_context_yes_no".to_string(),
            "has_date_or_freshness_hint_yes_no".to_string(),
            "independent_context_needed_yes_no".to_string(),
        ],
        forbidden_actions: default_forbidden_actions(),
        raw_data_stored: false,
        automated_account_discovery: false,
    }
}

fn default_forbidden_actions() -> Vec<String> {
    vec![
        "unofficial API lookup".to_string(),
        "mass enumeration".to_string(),
        "contact-list scraping".to_string(),
        "bypassing platform restrictions".to_string(),
        "saving raw private profile data".to_string(),
        "treating messenger presence as identity proof".to_string(),
    ]
}

fn platform_name(platform: &AssistedPlatform) -> String {
    match platform {
        AssistedPlatform::Telegram => "Telegram".to_string(),
        AssistedPlatform::Viber => "Viber".to_string(),
        AssistedPlatform::WhatsApp => "WhatsApp".to_string(),
        AssistedPlatform::Vk => "VK".to_string(),
        AssistedPlatform::Max => "MAX".to_string(),
        AssistedPlatform::PublicWeb => "PublicWeb".to_string(),
        AssistedPlatform::Other(value) => value.clone(),
    }
}

fn mask_phone(raw_phone: &str) -> String {
    let digits: String = raw_phone.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 6 {
        return "phone_masked".to_string();
    }
    let prefix_len = digits.len().min(4);
    let suffix_len = 2usize.min(digits.len().saturating_sub(prefix_len));
    format!("+{}***{}", &digits[..prefix_len], &digits[digits.len() - suffix_len..])
}

fn stable_flow_id(selector_masked: &str) -> String {
    let mut hash = 5381u64;
    for byte in selector_masked.as_bytes() {
        hash = ((hash << 5).wrapping_add(hash)).wrapping_add(*byte as u64);
    }
    format!("assisted_phone_{:x}", hash)
}

pub fn save_assisted_verification_flow(flow: &AssistedVerificationFlow, path: &str) -> Result<(), String> {
    let json = serde_json::to_string_pretty(flow).map_err(|err| format!("serialize assisted verification flow: {}", err))?;
    std::fs::write(path, json).map_err(|err| format!("write {}: {}", path, err))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flow_contains_max_and_no_automated_discovery() {
        let flow = build_phone_assisted_verification_flow("+000000000000");
        assert!(flow.steps.iter().any(|s| s.platform == AssistedPlatform::Max));
        assert!(flow.steps.iter().all(|s| !s.raw_data_stored));
        assert!(flow.steps.iter().all(|s| !s.automated_account_discovery));
        assert!(!flow.identity_confirmation_allowed);
    }

    #[test]
    fn corroborated_lead_caps_confidence() {
        let mut flow = build_phone_assisted_verification_flow("+000000000000");
        apply_assisted_decision(&mut flow, AssistedDecision::CorroboratedLead);
        assert_eq!(flow.confidence_cap, 55);
        assert!(flow.contributes_to_main_confidence);
        assert!(!flow.identity_confirmation_allowed);
    }
}
