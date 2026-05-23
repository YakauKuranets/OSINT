use serde::{Serialize, Deserialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityType {
    Nickname,
    Email,
    Phone,
    BankIdentifier,
    DateOfBirth,
    FullName,
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
    PublicOSINT,
    UnverifiedDump,
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