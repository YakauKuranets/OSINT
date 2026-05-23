use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityType {
    Nickname,
    Username,
    Email,
    Phone,
    Country,
    BankIdentifier,
    DateOfBirth,
    FullName,
    Domain,
    IpAddress,
    Url,
    SocialProfile,
    Organization,
    BreachName,
    FileHash,
    CryptoWallet,
    LocationHint,
    DataSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityNode {
    pub value: String,
    pub entity_type: EntityType,
    pub first_seen: u64,
}

impl EntityNode {
    pub fn new(value: &str, entity_type: EntityType) -> Self {
        let value = value.trim().to_lowercase();
        EntityNode {
            value,
            entity_type,
            first_seen: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityLink {
    pub source_node_value: String,
    pub target_node_value: String,
    pub weight_modifier: i16,
    pub metadata: SourceMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceClass {
    VerifiedRegistry,
    VerifiedOfficial,
    PublicOSINT,
    AuthorizedExport,
    LocalImport,
    DirtyPublicData,
    AIDerived,
    UnverifiedDump,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SensitivityClass {
    PublicLow,
    Personal,
    Sensitive,
    Financial,
    Secret,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObservationStatus {
    DirtyHypothesis,
    Weak,
    Possible,
    Probable,
    Confirmed,
    Conflicted,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceRegistryEntry {
    pub source_id: String,
    pub source_type: String,
    pub source_class: SourceClass,
    pub access_type: String,
    pub trust_level: u8,
    pub requires_sandbox: bool,
    pub can_create_hypothesis: bool,
    pub can_confirm_identity: bool,
    pub allow_sensitive_values_in_report: bool,
    pub allowed_entity_types: Vec<EntityType>,
    pub forbidden_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRecord {
    pub evidence_id: String,
    pub source_id: String,
    pub source_class: SourceClass,
    pub raw_sha256: String,
    pub captured_at: u64,
    pub normalized_snippet: String,
    pub confidence: u8,
    pub sensitivity: SensitivityClass,
    pub dirty_flag: bool,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationRecord {
    pub observation_id: String,
    pub evidence_id: String,
    pub entity_type: EntityType,
    pub value_masked: String,
    pub value_hash: String,
    pub normalized_value: String,
    pub sensitivity: SensitivityClass,
    pub confidence: u8,
    pub source_class: SourceClass,
    pub dirty_flag: bool,
    pub status: ObservationStatus,
    pub seen_at: u64,
}

pub struct DirtyDataPolicy;

impl DirtyDataPolicy {
    pub fn is_dirty_source(source_class: SourceClass) -> bool {
        matches!(source_class, SourceClass::DirtyPublicData | SourceClass::UnverifiedDump)
    }

    pub fn can_confirm_identity(source_class: SourceClass) -> bool {
        !matches!(
            source_class,
            SourceClass::DirtyPublicData | SourceClass::UnverifiedDump | SourceClass::AIDerived
        )
    }

    pub fn default_status(source_class: SourceClass, confidence: u8) -> ObservationStatus {
        if Self::is_dirty_source(source_class) {
            return ObservationStatus::DirtyHypothesis;
        }

        if source_class == SourceClass::AIDerived {
            return ObservationStatus::Weak;
        }

        match confidence {
            0..=24 => ObservationStatus::Weak,
            25..=49 => ObservationStatus::Possible,
            50..=79 => ObservationStatus::Probable,
            _ => ObservationStatus::Confirmed,
        }
    }

    pub fn must_mask_raw_value(sensitivity: SensitivityClass) -> bool {
        matches!(
            sensitivity,
            SensitivityClass::Personal
                | SensitivityClass::Sensitive
                | SensitivityClass::Financial
                | SensitivityClass::Secret
        )
    }

    pub fn may_store_raw_value(sensitivity: SensitivityClass) -> bool {
        !matches!(sensitivity, SensitivityClass::Financial | SensitivityClass::Secret)
    }

    pub fn mask_value(value: &str, sensitivity: SensitivityClass) -> String {
        match sensitivity {
            SensitivityClass::PublicLow => value.to_string(),
            SensitivityClass::Secret => "[secret-redacted]".to_string(),
            SensitivityClass::Financial => Self::mask_keep_last(value, 4),
            SensitivityClass::Personal | SensitivityClass::Sensitive => Self::mask_keep_last(value, 4),
        }
    }

    fn mask_keep_last(value: &str, keep: usize) -> String {
        let chars: Vec<char> = value.chars().collect();
        if chars.is_empty() {
            return String::new();
        }
        if chars.len() <= keep {
            return "*".repeat(chars.len());
        }
        let visible: String = chars[chars.len() - keep..].iter().collect();
        format!("{}{}", "*".repeat(chars.len() - keep), visible)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMetadata {
    pub source_id: String,
    pub class: SourceClass,
    pub import_timestamp: u64,
    pub data_actual_year: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityProfile {
    pub root_entity: EntityNode,
    pub associated_nodes: HashMap<String, EntityNode>,
    pub active_links: Vec<EntityLink>,
    pub calculated_confidence: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionEvidence {
    pub signal: String,
    pub weight: i16,
    pub source_id: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionReport {
    pub score: u8,
    pub level: String,
    pub matched_selectors: Vec<String>,
    pub evidences: Vec<ResolutionEvidence>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirty_source_creates_hypothesis_only() {
        let status = DirtyDataPolicy::default_status(SourceClass::DirtyPublicData, 95);
        assert_eq!(status, ObservationStatus::DirtyHypothesis);
        assert!(!DirtyDataPolicy::can_confirm_identity(SourceClass::DirtyPublicData));
    }

    #[test]
    fn verified_source_can_be_confirmed() {
        let status = DirtyDataPolicy::default_status(SourceClass::VerifiedOfficial, 90);
        assert_eq!(status, ObservationStatus::Confirmed);
        assert!(DirtyDataPolicy::can_confirm_identity(SourceClass::VerifiedOfficial));
    }

    #[test]
    fn financial_value_is_masked_and_not_stored_raw() {
        let masked = DirtyDataPolicy::mask_value("1234567890123456", SensitivityClass::Financial);
        assert_eq!(masked, "************3456");
        assert!(!DirtyDataPolicy::may_store_raw_value(SensitivityClass::Financial));
    }

    #[test]
    fn secret_value_is_redacted() {
        let masked = DirtyDataPolicy::mask_value("token-abc", SensitivityClass::Secret);
        assert_eq!(masked, "[secret-redacted]");
        assert!(!DirtyDataPolicy::may_store_raw_value(SensitivityClass::Secret));
    }
}
