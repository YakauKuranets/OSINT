use crate::models::{EntityNode, EntityType, SourceMetadata, SourceClass};
use crate::sandbox_runner;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerProvider {
    pub name: String,
    pub endpoint_url: String,
    pub auth_token: Option<String>,
    pub query_type: String,
    pub response_mapping: ResponseMapping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMapping {
    pub emails_path: Option<String>,
    pub phones_path: Option<String>,
    pub nicknames_path: Option<String>,
    pub passwords_path: Option<String>,
    pub profiles_path: Option<String>,
}

pub struct BrokerResult {
    pub nodes: Vec<EntityNode>,
    pub source_meta: SourceMetadata,
}

pub struct DataBroker {
    providers: Vec<BrokerProvider>,
}

impl DataBroker {
    pub fn new(config_path: &str) -> Self {
        let providers = match std::fs::read_to_string(config_path) {
            Ok(data) => serde_json::from_str::<Vec<BrokerProvider>>(&data).unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        DataBroker { providers }
    }

    pub async fn query(&self, query_type: &str, value: &str) -> Vec<BrokerResult> {
        let mut results = Vec::new();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();

        for provider in &self.providers {
            if provider.query_type != query_type {
                continue;
            }
            let url = provider.endpoint_url.replace("{value}", value);
            let mut headers: Vec<(&str, &str)> = Vec::new();
            if let Some(token) = &provider.auth_token {
                headers.push(("Authorization", &format!("Bearer {}", token)));
            }

            // Эфемерный запрос через Docker
            if let Some(body) = sandbox_runner::execute_ephemeral(&url, "GET", &headers) {
                if let Ok(json) = serde_json::from_str::<Value>(&body) {
                    let nodes = Self::extract_nodes(&json, &provider.response_mapping, now);
                    if !nodes.is_empty() {
                        let meta = SourceMetadata {
                            source_id: provider.name.clone(),
                            class: SourceClass::UnverifiedDump,
                            import_timestamp: now,
                            data_actual_year: 2026,
                        };
                        results.push(BrokerResult { nodes, source_meta: meta });
                    }
                }
            }
        }
        results
    }

    fn extract_nodes(json: &Value, mapping: &ResponseMapping, now: u64) -> Vec<EntityNode> {
        let mut nodes = Vec::new();
        if let Some(path) = &mapping.emails_path {
            if let Some(arr) = Self::get_json_path(json, path).and_then(|v| v.as_array()) {
                for item in arr {
                    if let Some(s) = item.as_str() {
                        nodes.push(EntityNode {
                            value: s.to_string(),
                            entity_type: EntityType::Email,
                            first_seen: now,
                        });
                    }
                }
            }
        }
        if let Some(path) = &mapping.phones_path {
            if let Some(arr) = Self::get_json_path(json, path).and_then(|v| v.as_array()) {
                for item in arr {
                    if let Some(s) = item.as_str() {
                        nodes.push(EntityNode {
                            value: s.to_string(),
                            entity_type: EntityType::Phone,
                            first_seen: now,
                        });
                    }
                }
            }
        }
        if let Some(path) = &mapping.nicknames_path {
            if let Some(arr) = Self::get_json_path(json, path).and_then(|v| v.as_array()) {
                for item in arr {
                    if let Some(s) = item.as_str() {
                        nodes.push(EntityNode {
                            value: s.to_string(),
                            entity_type: EntityType::Nickname,
                            first_seen: now,
                        });
                    }
                }
            }
        }
        if let Some(path) = &mapping.passwords_path {
            if let Some(arr) = Self::get_json_path(json, path).and_then(|v| v.as_array()) {
                for item in arr {
                    if let Some(s) = item.as_str() {
                        nodes.push(EntityNode {
                            value: s.to_string(),
                            entity_type: EntityType::Nickname,
                            first_seen: now,
                        });
                    }
                }
            }
        }
        nodes
    }

    fn get_json_path<'a>(json: &'a Value, path: &str) -> Option<&'a Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = json;
        for part in parts {
            current = current.get(part)?;
        }
        Some(current)
    }
}