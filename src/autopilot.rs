use crate::discovery::{self, DiscoveryReport};
use crate::models::{EntityNode, EntityType};
use crate::public_search::{self, PublicSearchReport};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutopilotCycleReport {
    pub cycle: usize,
    pub input_seed_count: usize,
    pub new_discovery_nodes: usize,
    pub new_public_search_nodes: usize,
    pub total_seed_count_after_cycle: usize,
    pub discovery_report: DiscoveryReport,
    pub public_search_report: PublicSearchReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutopilotReport {
    pub generated_at: u64,
    pub max_cycles: usize,
    pub initial_seed_count: usize,
    pub final_seed_count: usize,
    pub total_new_nodes: usize,
    pub cycles: Vec<AutopilotCycleReport>,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub async fn run_autonomous_osint(seeds: &mut Vec<EntityNode>) -> AutopilotReport {
    let max_cycles = std::env::var("OSINT_AUTOPILOT_CYCLES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(2)
        .clamp(1, 5);
    let per_cycle_new_limit = std::env::var("OSINT_AUTOPILOT_NEW_LIMIT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(80);

    let initial_seed_count = seeds.len();
    let mut seen = HashSet::new();
    dedupe_seeds_in_place(seeds, &mut seen);

    let mut cycles = Vec::new();
    let mut total_new_nodes = 0usize;

    for cycle_idx in 1..=max_cycles {
        let input_seed_count = seeds.len();
        let snapshot = seeds.clone();

        let discovery_report = discovery::run_public_discovery_for_seeds(&snapshot).await;
        let discovery_nodes = discovery::observations_as_entity_nodes(&discovery_report, per_cycle_new_limit);
        let new_discovery_nodes = append_unique_nodes(seeds, discovery_nodes, &mut seen, per_cycle_new_limit);

        let search_snapshot = seeds.clone();
        let public_search_report = public_search::run_public_search_for_seeds(&search_snapshot).await;
        let public_search_nodes = public_search::observations_as_entity_nodes(&public_search_report, per_cycle_new_limit);
        let new_public_search_nodes = append_unique_nodes(seeds, public_search_nodes, &mut seen, per_cycle_new_limit);

        let cycle_new = new_discovery_nodes + new_public_search_nodes;
        total_new_nodes += cycle_new;

        cycles.push(AutopilotCycleReport {
            cycle: cycle_idx,
            input_seed_count,
            new_discovery_nodes,
            new_public_search_nodes,
            total_seed_count_after_cycle: seeds.len(),
            discovery_report,
            public_search_report,
        });

        if cycle_new == 0 {
            break;
        }
    }

    AutopilotReport {
        generated_at: now_unix(),
        max_cycles,
        initial_seed_count,
        final_seed_count: seeds.len(),
        total_new_nodes,
        cycles,
    }
}

pub fn save_autopilot_report(report: &AutopilotReport, path: &str) -> Result<(), String> {
    let json = serde_json::to_string_pretty(report)
        .map_err(|err| format!("serialize autopilot report: {}", err))?;
    std::fs::write(path, json).map_err(|err| format!("write {}: {}", path, err))
}

fn dedupe_seeds_in_place(seeds: &mut Vec<EntityNode>, seen: &mut HashSet<String>) {
    seeds.retain(|node| seen.insert(node_key(node)));
}

fn append_unique_nodes(
    seeds: &mut Vec<EntityNode>,
    nodes: Vec<EntityNode>,
    seen: &mut HashSet<String>,
    limit: usize,
) -> usize {
    let mut added = 0usize;
    for node in nodes {
        if added >= limit {
            break;
        }
        if should_autopilot_expand(&node) && seen.insert(node_key(&node)) {
            seeds.push(node);
            added += 1;
        }
    }
    added
}

fn should_autopilot_expand(node: &EntityNode) -> bool {
    match node.entity_type {
        EntityType::Email | EntityType::Phone | EntityType::Username | EntityType::Nickname | EntityType::Domain | EntityType::Url => {
            let value = node.value.trim();
            !value.is_empty()
                && !value.contains("[redacted]")
                && !value.starts_with("seed_")
                && value.len() <= 256
        }
        _ => false,
    }
}

fn node_key(node: &EntityNode) -> String {
    format!("{:?}:{}", node.entity_type, normalize_value(&node.value, &node.entity_type))
}

fn normalize_value(value: &str, entity_type: &EntityType) -> String {
    match entity_type {
        EntityType::Phone => value.chars().filter(|c| c.is_ascii_digit()).collect(),
        EntityType::Email => value.trim().to_lowercase(),
        EntityType::Username | EntityType::Nickname => value.trim().trim_start_matches('@').to_lowercase(),
        EntityType::Url | EntityType::Domain => value.trim().to_lowercase(),
        _ => value.trim().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedupe_removes_duplicate_seed() {
        let mut seeds = vec![
            EntityNode { value: "@Test".to_string(), entity_type: EntityType::Username, first_seen: 0 },
            EntityNode { value: "test".to_string(), entity_type: EntityType::Username, first_seen: 0 },
        ];
        let mut seen = HashSet::new();
        dedupe_seeds_in_place(&mut seeds, &mut seen);
        assert_eq!(seeds.len(), 1);
    }

    #[test]
    fn autopilot_does_not_expand_redacted_values() {
        let node = EntityNode { value: "[redacted]".to_string(), entity_type: EntityType::Email, first_seen: 0 };
        assert!(!should_autopilot_expand(&node));
    }
}
