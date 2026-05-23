use crate::models::{EntityType, SourceClass, SourceRegistryEntry};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct SourceRegistry {
    entries: HashMap<String, SourceRegistryEntry>,
}

impl SourceRegistry {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register(Self::default_entry(
            "verified_official",
            "official_api_or_registry",
            SourceClass::VerifiedOfficial,
            "official_or_controlled_public_source",
            90,
            false,
            true,
            true,
        ));
        registry.register(Self::default_entry(
            "verified_registry",
            "verified_registry",
            SourceClass::VerifiedRegistry,
            "verified_registry_or_reputable_api",
            85,
            false,
            true,
            true,
        ));
        registry.register(Self::default_entry(
            "public_osint",
            "public_web_or_messenger",
            SourceClass::PublicOSINT,
            "publicly_available_source",
            60,
            false,
            true,
            true,
        ));
        registry.register(Self::default_entry(
            "authorized_export",
            "authorized_export",
            SourceClass::AuthorizedExport,
            "user_authorized_export_or_backup",
            75,
            true,
            true,
            true,
        ));
        registry.register(Self::default_entry(
            "local_import",
            "local_file_import",
            SourceClass::LocalImport,
            "local_runtime_import",
            50,
            true,
            true,
            false,
        ));
        registry.register(Self::default_entry(
            "dirty_public_data",
            "dirty_public_file_or_dump",
            SourceClass::DirtyPublicData,
            "publicly_available_unverified_dirty_data",
            20,
            true,
            true,
            false,
        ));
        registry.register(Self::default_entry(
            "ai_derived",
            "ai_extracted_or_inferred",
            SourceClass::AIDerived,
            "derived_from_model_output",
            25,
            false,
            true,
            false,
        ));
        registry.register(Self::default_entry(
            "unverified_dump",
            "unverified_local_dump",
            SourceClass::UnverifiedDump,
            "unverified_imported_data",
            15,
            true,
            true,
            false,
        ));
        registry
    }

    pub fn register(&mut self, entry: SourceRegistryEntry) {
        self.entries.insert(entry.source_id.clone(), entry);
    }

    pub fn get(&self, source_id: &str) -> Option<&SourceRegistryEntry> {
        self.entries.get(source_id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn trust_level(&self, source_id: &str) -> Option<u8> {
        self.get(source_id).map(|entry| entry.trust_level)
    }

    pub fn requires_sandbox(&self, source_id: &str) -> bool {
        self.get(source_id)
            .map(|entry| entry.requires_sandbox)
            .unwrap_or(true)
    }

    pub fn can_confirm_identity(&self, source_id: &str) -> bool {
        self.get(source_id)
            .map(|entry| entry.can_confirm_identity)
            .unwrap_or(false)
    }

    pub fn can_create_hypothesis(&self, source_id: &str) -> bool {
        self.get(source_id)
            .map(|entry| entry.can_create_hypothesis)
            .unwrap_or(false)
    }

    pub fn class_for(&self, source_id: &str) -> Option<SourceClass> {
        self.get(source_id).map(|entry| entry.source_class)
    }

    pub fn allow_sensitive_values_in_report(&self, source_id: &str) -> bool {
        self.get(source_id)
            .map(|entry| entry.allow_sensitive_values_in_report)
            .unwrap_or(false)
    }

    pub fn default_entry(
        source_id: &str,
        source_type: &str,
        source_class: SourceClass,
        access_type: &str,
        trust_level: u8,
        requires_sandbox: bool,
        can_create_hypothesis: bool,
        can_confirm_identity: bool,
    ) -> SourceRegistryEntry {
        SourceRegistryEntry {
            source_id: source_id.to_string(),
            source_type: source_type.to_string(),
            source_class,
            access_type: access_type.to_string(),
            trust_level: trust_level.min(100),
            requires_sandbox,
            can_create_hypothesis,
            can_confirm_identity,
            allow_sensitive_values_in_report: false,
            allowed_entity_types: vec![
                EntityType::Nickname,
                EntityType::Username,
                EntityType::Email,
                EntityType::Phone,
                EntityType::Country,
                EntityType::DateOfBirth,
                EntityType::FullName,
                EntityType::Domain,
                EntityType::IpAddress,
                EntityType::Url,
                EntityType::SocialProfile,
                EntityType::Organization,
                EntityType::BreachName,
                EntityType::FileHash,
                EntityType::CryptoWallet,
                EntityType::LocationHint,
                EntityType::DataSource,
            ],
            forbidden_fields: vec![
                "raw_password".to_string(),
                "session_cookie".to_string(),
                "auth_token".to_string(),
                "private_key".to_string(),
            ],
        }
    }
}

pub fn classify_unknown_source(source_id: &str) -> SourceRegistryEntry {
    let lowered = source_id.to_lowercase();
    if lowered.contains("verified") || lowered.contains("registry") || lowered.contains("official") {
        SourceRegistry::default_entry(
            source_id,
            "auto_classified_verified",
            SourceClass::VerifiedRegistry,
            "auto_classified_source_id",
            70,
            false,
            true,
            true,
        )
    } else if lowered.contains("public") || lowered.contains("osint") {
        SourceRegistry::default_entry(
            source_id,
            "auto_classified_public",
            SourceClass::PublicOSINT,
            "auto_classified_source_id",
            55,
            false,
            true,
            true,
        )
    } else if lowered.contains("ai") || lowered.contains("llm") || lowered.contains("model") {
        SourceRegistry::default_entry(
            source_id,
            "auto_classified_ai",
            SourceClass::AIDerived,
            "auto_classified_source_id",
            25,
            false,
            true,
            false,
        )
    } else if lowered.contains("dirty") || lowered.contains("dump") || lowered.contains("leak") {
        SourceRegistry::default_entry(
            source_id,
            "auto_classified_dirty",
            SourceClass::DirtyPublicData,
            "auto_classified_source_id",
            20,
            true,
            true,
            false,
        )
    } else {
        SourceRegistry::default_entry(
            source_id,
            "auto_classified_local_import",
            SourceClass::LocalImport,
            "auto_classified_source_id",
            40,
            true,
            true,
            false,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registry_contains_core_classes() {
        let registry = SourceRegistry::with_defaults();
        assert!(registry.len() >= 8);
        assert_eq!(registry.class_for("dirty_public_data"), Some(SourceClass::DirtyPublicData));
        assert_eq!(registry.class_for("verified_official"), Some(SourceClass::VerifiedOfficial));
    }

    #[test]
    fn dirty_public_data_requires_sandbox_and_cannot_confirm() {
        let registry = SourceRegistry::with_defaults();
        assert!(registry.requires_sandbox("dirty_public_data"));
        assert!(registry.can_create_hypothesis("dirty_public_data"));
        assert!(!registry.can_confirm_identity("dirty_public_data"));
    }

    #[test]
    fn verified_official_can_confirm_identity() {
        let registry = SourceRegistry::with_defaults();
        assert!(!registry.requires_sandbox("verified_official"));
        assert!(registry.can_confirm_identity("verified_official"));
        assert_eq!(registry.trust_level("verified_official"), Some(90));
    }

    #[test]
    fn unknown_dump_source_is_classified_as_dirty() {
        let entry = classify_unknown_source("old_public_leak_dump_2020");
        assert_eq!(entry.source_class, SourceClass::DirtyPublicData);
        assert!(entry.requires_sandbox);
        assert!(!entry.can_confirm_identity);
    }
}
