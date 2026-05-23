use crate::models::{EntityNode, EntityType};

#[derive(Debug, Clone)]
pub struct Observation {
    pub value: String,
    pub entity_type: EntityType,
    pub source_id: String,
    pub timestamp: u64,
    pub confidence: u8,
    pub evidence_snippet: String,
    pub connector_kind: String,
}

impl Observation {
    pub fn to_entity_node(&self) -> EntityNode {
        EntityNode {
            value: self.value.clone(),
            entity_type: self.entity_type.clone(),
            first_seen: self.timestamp,
        }
    }
}

pub trait Connector {
    fn id(&self) -> &'static str;
    fn kind(&self) -> &'static str;
    fn supports(&self, entity_type: &EntityType) -> bool;
}

pub struct SocialSpiderConnector;

impl Connector for SocialSpiderConnector {
    fn id(&self) -> &'static str {
        "social_spider"
    }

    fn kind(&self) -> &'static str {
        "social"
    }

    fn supports(&self, entity_type: &EntityType) -> bool {
        matches!(entity_type, EntityType::Nickname)
    }
}

pub struct EmailBreachConnector;

impl Connector for EmailBreachConnector {
    fn id(&self) -> &'static str {
        "email_breach"
    }

    fn kind(&self) -> &'static str {
        "breach"
    }

    fn supports(&self, entity_type: &EntityType) -> bool {
        matches!(entity_type, EntityType::Email)
    }
}

impl EmailBreachConnector {
    pub fn collect(&self, email: &str, timestamp: u64) -> Vec<Observation> {
        vec![Observation {
            value: format!("seed_email:{}", email),
            entity_type: EntityType::Email,
            source_id: self.id().to_string(),
            timestamp,
            confidence: 60,
            evidence_snippet: "connector-enabled-email-seed".to_string(),
            connector_kind: self.kind().to_string(),
        }]
    }

    pub fn collect_breaches(
        &self,
        email: &str,
        breaches: &[String],
        timestamp: u64,
    ) -> Vec<Observation> {
        breaches
            .iter()
            .map(|name| Observation {
                value: format!("breach:{}", name),
                entity_type: EntityType::Nickname,
                source_id: self.id().to_string(),
                timestamp,
                confidence: 75,
                evidence_snippet: format!("email={} matched breach={}", email, name),
                connector_kind: self.kind().to_string(),
            })
            .collect()
    }
}

pub struct PhoneIntelConnector;

impl Connector for PhoneIntelConnector {
    fn id(&self) -> &'static str {
        "phone_intel"
    }

    fn kind(&self) -> &'static str {
        "phone"
    }

    fn supports(&self, entity_type: &EntityType) -> bool {
        matches!(entity_type, EntityType::Phone)
    }
}

impl PhoneIntelConnector {
    pub fn collect_phone_traits(&self, traits: &[String], timestamp: u64) -> Vec<Observation> {
        traits
            .iter()
            .map(|t| Observation {
                value: t.clone(),
                entity_type: EntityType::Nickname,
                source_id: self.id().to_string(),
                timestamp,
                confidence: 65,
                evidence_snippet: format!("phone_trait={}", t),
                connector_kind: self.kind().to_string(),
            })
            .collect()
    }
}

impl SocialSpiderConnector {
    pub fn collect(
        &self,
        username: &str,
        timestamp: u64,
    ) -> Vec<Observation> {
        vec![Observation {
            value: format!("seed_nickname:{}", username),
            entity_type: EntityType::Nickname,
            source_id: self.id().to_string(),
            timestamp,
            confidence: 50,
            evidence_snippet: "connector-enabled-social-seed".to_string(),
            connector_kind: self.kind().to_string(),
        }]
    }
}
