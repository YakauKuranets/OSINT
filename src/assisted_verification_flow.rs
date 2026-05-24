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
pub struct AssistedPlatformProfile {
    pub platform: AssistedPlatform,
    pub display_name: String,
    pub verification_mode: String,
    pub operator_entrypoint: String,
    pub platform_specific_checks: Vec<String>,
    pub platform_specific_risks: Vec<String>,
    pub allowed_result_categories: Vec<String>,
    pub promotion_requirements: Vec<String>,
    pub max_confidence_without_independent_source: u8,
    pub raw_data_stored: bool,
    pub automated_account_discovery: bool,
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
    pub platform_specific_checks: Vec<String>,
    pub forbidden_actions: Vec<String>,
    pub raw_data_stored: bool,
    pub automated_account_discovery: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistedVerificationFlow {
    pub flow_id: String,
    pub selector_type: String,
    pub selector_masked: String,
    pub platform_profiles: Vec<AssistedPlatformProfile>,
    pub steps: Vec<AssistedVerificationStep>,
    pub decision: AssistedDecision,
    pub confidence_cap: u8,
    pub contributes_to_main_confidence: bool,
    pub identity_confirmation_allowed: bool,
    pub raw_data_stored: bool,
    pub automated_account_discovery: bool,
    pub global_warnings: Vec<String>,
}

pub fn build_phone_assisted_verification_flow(raw_phone: &str) -> AssistedVerificationFlow {
    let selector_masked = mask_phone(raw_phone);
    let platforms = assisted_messenger_platforms();
    let platform_profiles = platforms.iter().cloned().map(platform_profile).collect::<Vec<_>>();

    let mut steps = vec![AssistedVerificationStep {
        step_id: "normalize_phone".to_string(),
        platform: AssistedPlatform::PublicWeb,
        kind: AssistedStepKind::NormalizeSelector,
        status: AssistedStepStatus::CompletedByOperator,
        selector_masked: selector_masked.clone(),
        official_action_hint: None,
        operator_instruction: "Phone selector was normalized and masked. Raw value is not stored in this flow.".to_string(),
        required_operator_input: vec!["confirm selector belongs to current authorized case scope".to_string()],
        platform_specific_checks: vec!["Only the masked selector is stored in the flow artifact.".to_string()],
        forbidden_actions: default_forbidden_actions(),
        raw_data_stored: false,
        automated_account_discovery: false,
    }];

    for profile in &platform_profiles {
        steps.push(open_official_interface_step(profile, &selector_masked));
        steps.push(operator_manual_check_step(profile, &selector_masked));
        steps.push(context_assessment_step(profile, &selector_masked));
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
            "stable_url_or_user_export_reference_if_available".to_string(),
            "sanitized_summary_no_raw_profile_data".to_string(),
        ],
        platform_specific_checks: vec![
            "Do not use messenger presence alone as the independent source.".to_string(),
            "Prefer public marketplace/job/profile/registry/user-provided export with visible context.".to_string(),
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
        required_operator_input: vec![
            "decision".to_string(),
            "operator_initials".to_string(),
            "checked_at".to_string(),
            "sanitized_note".to_string(),
            "independent_confirmation_found_yes_no".to_string(),
        ],
        platform_specific_checks: vec![
            "Promote only to CorroboratedLead when an allowed independent source exists.".to_string(),
            "Never mark identity as confirmed from messenger/social presence alone.".to_string(),
        ],
        forbidden_actions: default_forbidden_actions(),
        raw_data_stored: false,
        automated_account_discovery: false,
    });

    AssistedVerificationFlow {
        flow_id: stable_flow_id(&selector_masked),
        selector_type: "phone".to_string(),
        selector_masked,
        platform_profiles,
        steps,
        decision: AssistedDecision::Pending,
        confidence_cap: 0,
        contributes_to_main_confidence: false,
        identity_confirmation_allowed: false,
        raw_data_stored: false,
        automated_account_discovery: false,
        global_warnings: vec![
            "This is assisted verification, not hidden automated probing.".to_string(),
            "Official clients/interfaces may be opened for the operator, but account discovery is not automated.".to_string(),
            "Operator records only sanitized result categories, not raw profile data.".to_string(),
            "Messenger presence never proves current ownership or identity by itself.".to_string(),
            "MAX is handled as a separate manual messenger verification target.".to_string(),
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
    flow.automated_account_discovery = false;
}

fn assisted_messenger_platforms() -> Vec<AssistedPlatform> {
    vec![
        AssistedPlatform::Telegram,
        AssistedPlatform::Viber,
        AssistedPlatform::WhatsApp,
        AssistedPlatform::Vk,
        AssistedPlatform::Max,
    ]
}

fn platform_profile(platform: AssistedPlatform) -> AssistedPlatformProfile {
    let display_name = platform_name(&platform);
    let mut profile = AssistedPlatformProfile {
        platform: platform.clone(),
        display_name: display_name.clone(),
        verification_mode: "manual_assisted_official_interface_only".to_string(),
        operator_entrypoint: format!("Open the official {} app/client manually for the current authorized selector only.", display_name),
        platform_specific_checks: vec![
            "exact selector match category".to_string(),
            "public display context if visible".to_string(),
            "freshness/date hint if visible".to_string(),
            "ambiguity or reassignment risk".to_string(),
        ],
        platform_specific_risks: vec![
            "presence on platform does not prove current owner".to_string(),
            "display names and avatars can be outdated or misleading".to_string(),
            "phone numbers can be reassigned".to_string(),
        ],
        allowed_result_categories: vec![
            "not_found".to_string(),
            "exact_found_needs_context".to_string(),
            "similar_only".to_string(),
            "inconclusive".to_string(),
            "rejected".to_string(),
        ],
        promotion_requirements: vec![
            "exact selector match recorded as category only".to_string(),
            "sanitized context note without raw profile data".to_string(),
            "independent allowed-source confirmation before contributing to main confidence".to_string(),
        ],
        max_confidence_without_independent_source: 25,
        raw_data_stored: false,
        automated_account_discovery: false,
    };

    match platform {
        AssistedPlatform::Telegram => {
            profile.platform_specific_checks.extend([
                "check public username/display name only if visible in the official client".to_string(),
                "do not export chats or contacts for this check".to_string(),
            ]);
            profile.platform_specific_risks.extend([
                "phone privacy settings may hide account presence".to_string(),
                "username can change independently from phone".to_string(),
            ]);
        }
        AssistedPlatform::Viber => {
            profile.platform_specific_checks.extend([
                "check official Viber client result category only".to_string(),
                "do not save contact card or profile image".to_string(),
            ]);
            profile.platform_specific_risks.extend([
                "Viber contact visibility depends on device/contact state".to_string(),
                "local contact naming may contaminate interpretation".to_string(),
            ]);
        }
        AssistedPlatform::WhatsApp => {
            profile.platform_specific_checks.extend([
                "check official WhatsApp client result category only".to_string(),
                "do not use wa.me automation for bulk checks".to_string(),
            ]);
            profile.platform_specific_risks.extend([
                "business/personal accounts can be ambiguous".to_string(),
                "availability can vary by privacy settings and app state".to_string(),
            ]);
        }
        AssistedPlatform::Vk => {
            profile.platform_specific_checks.extend([
                "check only lawful public VK interface/profile context".to_string(),
                "record public profile context as sanitized category only".to_string(),
            ]);
            profile.platform_specific_risks.extend([
                "VK phone/email search behavior and visibility can change".to_string(),
                "public profile can be fake, abandoned, or reused".to_string(),
            ]);
        }
        AssistedPlatform::Max => {
            profile.platform_specific_checks.extend([
                "check official MAX client manually".to_string(),
                "treat unstable UX/platform behavior as inconclusive unless independently confirmed".to_string(),
            ]);
            profile.platform_specific_risks.extend([
                "MAX results may be noisy, unstable, or incomplete".to_string(),
                "MAX presence alone must stay operator-review only".to_string(),
            ]);
        }
        AssistedPlatform::PublicWeb | AssistedPlatform::Other(_) => {}
    }
    profile
}

fn open_official_interface_step(profile: &AssistedPlatformProfile, selector_masked: &str) -> AssistedVerificationStep {
    AssistedVerificationStep {
        step_id: format!("open_official_{}", normalized_platform_id(&profile.display_name)),
        platform: profile.platform.clone(),
        kind: AssistedStepKind::OpenOfficialInterface,
        status: AssistedStepStatus::ReadyForOperator,
        selector_masked: selector_masked.to_string(),
        official_action_hint: Some(profile.operator_entrypoint.clone()),
        operator_instruction: format!("Open {} through the official client or lawful public interface and prepare to check only the current authorized selector.", profile.display_name),
        required_operator_input: vec!["opened_yes_no".to_string(), "blocked_or_unavailable_yes_no".to_string()],
        platform_specific_checks: profile.platform_specific_checks.clone(),
        forbidden_actions: default_forbidden_actions(),
        raw_data_stored: false,
        automated_account_discovery: false,
    }
}

fn operator_manual_check_step(profile: &AssistedPlatformProfile, selector_masked: &str) -> AssistedVerificationStep {
    AssistedVerificationStep {
        step_id: format!("manual_check_{}", normalized_platform_id(&profile.display_name)),
        platform: profile.platform.clone(),
        kind: AssistedStepKind::OperatorManualCheck,
        status: AssistedStepStatus::ReadyForOperator,
        selector_masked: selector_masked.to_string(),
        official_action_hint: None,
        operator_instruction: format!("Manually check whether the selector appears reachable or associated in {}. Record only category: found / not_found / inconclusive / rejected.", profile.display_name),
        required_operator_input: vec![
            "result_category".to_string(),
            "exact_match_yes_no".to_string(),
            "sanitized_summary_no_raw_data".to_string(),
        ],
        platform_specific_checks: profile.platform_specific_checks.clone(),
        forbidden_actions: default_forbidden_actions(),
        raw_data_stored: false,
        automated_account_discovery: false,
    }
}

fn context_assessment_step(profile: &AssistedPlatformProfile, selector_masked: &str) -> AssistedVerificationStep {
    AssistedVerificationStep {
        step_id: format!("context_assessment_{}", normalized_platform_id(&profile.display_name)),
        platform: profile.platform.clone(),
        kind: AssistedStepKind::ContextAssessment,
        status: AssistedStepStatus::ReadyForOperator,
        selector_masked: selector_masked.to_string(),
        official_action_hint: None,
        operator_instruction: format!("Assess whether the {} result has lawful public context: public username, public display name, date hint, avatar consistency, or independent source. Do not copy private profile data.", profile.display_name),
        required_operator_input: vec![
            "has_context_yes_no".to_string(),
            "has_date_or_freshness_hint_yes_no".to_string(),
            "independent_context_needed_yes_no".to_string(),
        ],
        platform_specific_checks: profile.platform_specific_checks.clone(),
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

fn normalized_platform_id(name: &str) -> String {
    name.to_lowercase().replace(' ', "_")
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
    fn flow_contains_all_five_platform_profiles_and_no_automated_discovery() {
        let flow = build_phone_assisted_verification_flow("+000000000000");
        assert_eq!(flow.platform_profiles.len(), 5);
        assert!(flow.platform_profiles.iter().any(|p| p.platform == AssistedPlatform::Telegram));
        assert!(flow.platform_profiles.iter().any(|p| p.platform == AssistedPlatform::Viber));
        assert!(flow.platform_profiles.iter().any(|p| p.platform == AssistedPlatform::WhatsApp));
        assert!(flow.platform_profiles.iter().any(|p| p.platform == AssistedPlatform::Vk));
        assert!(flow.platform_profiles.iter().any(|p| p.platform == AssistedPlatform::Max));
        assert!(flow.platform_profiles.iter().all(|p| !p.raw_data_stored));
        assert!(flow.platform_profiles.iter().all(|p| !p.automated_account_discovery));
        assert!(flow.steps.iter().all(|s| !s.raw_data_stored));
        assert!(flow.steps.iter().all(|s| !s.automated_account_discovery));
        assert!(!flow.identity_confirmation_allowed);
        assert!(!flow.raw_data_stored);
        assert!(!flow.automated_account_discovery);
    }

    #[test]
    fn each_platform_has_three_operator_steps() {
        let flow = build_phone_assisted_verification_flow("+000000000000");
        for platform in assisted_messenger_platforms() {
            let platform_steps = flow.steps.iter().filter(|step| step.platform == platform).collect::<Vec<_>>();
            assert_eq!(platform_steps.len(), 3);
            assert!(platform_steps.iter().any(|s| s.kind == AssistedStepKind::OpenOfficialInterface));
            assert!(platform_steps.iter().any(|s| s.kind == AssistedStepKind::OperatorManualCheck));
            assert!(platform_steps.iter().any(|s| s.kind == AssistedStepKind::ContextAssessment));
        }
    }

    #[test]
    fn max_profile_has_specific_risk_notes() {
        let profile = platform_profile(AssistedPlatform::Max);
        assert!(profile.platform_specific_checks.iter().any(|v| v.contains("MAX")));
        assert!(profile.platform_specific_risks.iter().any(|v| v.contains("MAX")));
        assert_eq!(profile.max_confidence_without_independent_source, 25);
        assert!(!profile.raw_data_stored);
        assert!(!profile.automated_account_discovery);
    }

    #[test]
    fn corroborated_lead_caps_confidence() {
        let mut flow = build_phone_assisted_verification_flow("+000000000000");
        apply_assisted_decision(&mut flow, AssistedDecision::CorroboratedLead);
        assert_eq!(flow.confidence_cap, 55);
        assert!(flow.contributes_to_main_confidence);
        assert!(!flow.identity_confirmation_allowed);
        assert!(!flow.raw_data_stored);
        assert!(!flow.automated_account_discovery);
    }
}
