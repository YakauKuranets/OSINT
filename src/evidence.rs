use crate::models::{
    DirtyDataPolicy, EntityType, EvidenceRecord, ObservationRecord, ObservationStatus,
    SensitivityClass, SourceClass,
};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct EvidenceInput {
    pub source_id: String,
    pub source_class: SourceClass,
    pub entity_type: EntityType,
    pub raw_value: String,
    pub raw_context: String,
    pub confidence: u8,
    pub sensitivity: SensitivityClass,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct EvidenceObservationPair {
    pub evidence: EvidenceRecord,
    pub observation: ObservationRecord,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn sha256_hex(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn normalize_value(value: &str, entity_type: &EntityType) -> String {
    match entity_type {
        EntityType::Phone => value.chars().filter(|c| c.is_ascii_digit()).collect(),
        EntityType::Email => value.trim().to_lowercase(),
        EntityType::Nickname | EntityType::Username => value.trim().trim_start_matches('@').to_lowercase(),
        EntityType::Domain | EntityType::Url => value.trim().to_lowercase(),
        _ => value.trim().to_string(),
    }
}

fn make_id(prefix: &str, seed: &str) -> String {
    let hash = sha256_hex(seed);
    format!("{}_{}", prefix, &hash[..16])
}

fn sanitize_snippet(context: &str, sensitivity: SensitivityClass) -> String {
    let compact = context
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    let shortened = if compact.chars().count() > 240 {
        compact.chars().take(240).collect::<String>()
    } else {
        compact
    };

    if DirtyDataPolicy::must_mask_raw_value(sensitivity) {
        DirtyDataPolicy::mask_value(&shortened, sensitivity)
    } else {
        shortened
    }
}

pub fn build_evidence_observation(input: EvidenceInput) -> EvidenceObservationPair {
    let captured_at = now_unix();
    let normalized_value = normalize_value(&input.raw_value, &input.entity_type);
    let value_hash = sha256_hex(&normalized_value);
    let dirty_flag = DirtyDataPolicy::is_dirty_source(input.source_class);
    let status = DirtyDataPolicy::default_status(input.source_class, input.confidence);
    let stored_value = if DirtyDataPolicy::may_store_raw_value(input.sensitivity) {
        normalized_value.clone()
    } else {
        String::new()
    };
    let value_masked = DirtyDataPolicy::mask_value(&input.raw_value, input.sensitivity);
    let raw_sha256 = sha256_hex(&format!("{}::{}", input.source_id, input.raw_context));
    let evidence_seed = format!("{}::{}::{}", input.source_id, raw_sha256, captured_at);
    let observation_seed = format!("{}::{}::{}", input.source_id, value_hash, captured_at);

    let evidence = EvidenceRecord {
        evidence_id: make_id("ev", &evidence_seed),
        source_id: input.source_id.clone(),
        source_class: input.source_class,
        raw_sha256,
        captured_at,
        normalized_snippet: sanitize_snippet(&input.raw_context, input.sensitivity),
        confidence: input.confidence,
        sensitivity: input.sensitivity,
        dirty_flag,
        tags: input.tags,
    };

    let observation = ObservationRecord {
        observation_id: make_id("obs", &observation_seed),
        evidence_id: evidence.evidence_id.clone(),
        entity_type: input.entity_type,
        value_masked,
        value_hash,
        normalized_value: stored_value,
        sensitivity: input.sensitivity,
        confidence: input.confidence,
        source_class: input.source_class,
        dirty_flag,
        status,
        seen_at: captured_at,
    };

    EvidenceObservationPair { evidence, observation }
}

pub fn force_hypothesis_for_dirty(status: ObservationStatus, source_class: SourceClass) -> ObservationStatus {
    if DirtyDataPolicy::is_dirty_source(source_class) {
        ObservationStatus::DirtyHypothesis
    } else {
        status
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_is_stable() {
        assert_eq!(sha256_hex("abc"), sha256_hex("abc"));
        assert_ne!(sha256_hex("abc"), sha256_hex("abcd"));
    }

    #[test]
    fn phone_is_normalized_to_digits() {
        assert_eq!(normalize_value("+375 (29) 123-45-67", &EntityType::Phone), "375291234567");
    }

    #[test]
    fn dirty_input_creates_dirty_hypothesis() {
        let pair = build_evidence_observation(EvidenceInput {
            source_id: "dirty_public_dump_001".to_string(),
            source_class: SourceClass::DirtyPublicData,
            entity_type: EntityType::Email,
            raw_value: "User@Example.COM".to_string(),
            raw_context: "User@Example.COM appeared in old dump".to_string(),
            confidence: 95,
            sensitivity: SensitivityClass::Personal,
            tags: vec!["test".to_string()],
        });

        assert!(pair.evidence.dirty_flag);
        assert!(pair.observation.dirty_flag);
        assert_eq!(pair.observation.status, ObservationStatus::DirtyHypothesis);
    }

    #[test]
    fn secret_value_is_not_stored_raw() {
        let pair = build_evidence_observation(EvidenceInput {
            source_id: "local_test".to_string(),
            source_class: SourceClass::LocalImport,
            entity_type: EntityType::DataSource,
            raw_value: "token-abc".to_string(),
            raw_context: "token-abc".to_string(),
            confidence: 20,
            sensitivity: SensitivityClass::Secret,
            tags: vec![],
        });

        assert_eq!(pair.observation.value_masked, "[secret-redacted]");
        assert!(pair.observation.normalized_value.is_empty());
    }
}
