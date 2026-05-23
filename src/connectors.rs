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

pub struct ConnectorRegistry {
    social: SocialSpiderConnector,
    email: EmailBreachConnector,
}

impl ConnectorRegistry {
    pub fn new() -> Self {
        Self {
            social: SocialSpiderConnector,
            email: EmailBreachConnector,
        }
    }

    pub fn collect_seed_observations(&self, seeds: &[EntityNode], timestamp: u64) -> Vec<Observation> {
        let mut observations = Vec::new();
        for seed in seeds {
            if self.social.supports(&seed.entity_type) {
                observations.extend(self.social.collect(&seed.value, timestamp));
            }
            if self.email.supports(&seed.entity_type) {
                observations.extend(self.email.collect(&seed.value, timestamp));
            }
        }
        observations
    }
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

pub struct TelegramConnector;

impl Connector for TelegramConnector {
    fn id(&self) -> &'static str {
        "telegram"
    }

    fn kind(&self) -> &'static str {
        "messenger"
    }

    fn supports(&self, entity_type: &EntityType) -> bool {
        matches!(entity_type, EntityType::Nickname | EntityType::Phone)
    }
}

impl TelegramConnector {
    pub fn collect_telegram_info(&self, info: &[String], timestamp: u64) -> Vec<Observation> {
        info.iter()
            .map(|entry| {
                let entity_type = if entry.starts_with("tg_phone:") {
                    EntityType::Phone
                } else {
                    EntityType::Nickname
                };

                let value = if let Some(stripped) = entry.strip_prefix("tg_phone:") {
                    stripped.to_string()
                } else {
                    entry.clone()
                };

                Observation {
                    value,
                    entity_type,
                    source_id: self.id().to_string(),
                    timestamp,
                    confidence: 70,
                    evidence_snippet: entry.clone(),
                    connector_kind: self.kind().to_string(),
                }
            })
            .collect()
    }
}

pub struct BrokerConnector;

impl Connector for BrokerConnector {
    fn id(&self) -> &'static str {
        "broker"
    }

    fn kind(&self) -> &'static str {
        "broker"
    }

    fn supports(&self, entity_type: &EntityType) -> bool {
        matches!(entity_type, EntityType::Email | EntityType::Phone)
    }
}

impl BrokerConnector {
    pub fn collect_nodes(
        &self,
        nodes: &[EntityNode],
        source_id: &str,
        timestamp: u64,
    ) -> Vec<Observation> {
        nodes
            .iter()
            .map(|n| Observation {
                value: n.value.clone(),
                entity_type: n.entity_type.clone(),
                source_id: source_id.to_string(),
                timestamp,
                confidence: 70,
                evidence_snippet: format!("broker_entity={}", n.value),
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
