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

