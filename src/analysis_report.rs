use crate::conflicts::ConflictReport;
use crate::models::{IdentityProfile, ResolutionReport};
use crate::scoring::SourceHealth;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisReport {
    pub generated_at: u64,
    pub root_entity: String,
    pub root_entity_type: String,
    pub confidence: u8,
    pub associated_nodes_count: usize,
    pub active_links_count: usize,
    pub source_health: Vec<SourceHealth>,
    pub next_steps: Vec<String>,
    pub resolution_report: ResolutionReport,
    pub conflict_report: ConflictReport,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn build_analysis_report(
    profile: &IdentityProfile,
    resolution_report: ResolutionReport,
    conflict_report: ConflictReport,
    source_health: Vec<SourceHealth>,
    next_steps: Vec<String>,
) -> AnalysisReport {
    AnalysisReport {
        generated_at: now_unix(),
        root_entity: profile.root_entity.value.clone(),
        root_entity_type: format!("{:?}", profile.root_entity.entity_type),
        confidence: profile.calculated_confidence,
        associated_nodes_count: profile.associated_nodes.len(),
        active_links_count: profile.active_links.len(),
        source_health,
        next_steps,
        resolution_report,
        conflict_report,
    }
}

pub fn save_analysis_report(report: &AnalysisReport, path: &str) -> Result<(), String> {
    let json = serde_json::to_string_pretty(report)
        .map_err(|err| format!("serialize analysis report: {}", err))?;
    std::fs::write(path, json).map_err(|err| format!("write {}: {}", path, err))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conflicts::ConflictReport;
    use crate::models::{EntityNode, EntityType, IdentityProfile, ResolutionReport};
    use std::collections::HashMap;

    #[test]
    fn builds_report_summary_counts() {
        let profile = IdentityProfile {
            root_entity: EntityNode {
                value: "root".to_string(),
                entity_type: EntityType::Nickname,
                first_seen: 0,
            },
            associated_nodes: HashMap::from([(
                "a@example.com".to_string(),
                EntityNode {
                    value: "a@example.com".to_string(),
                    entity_type: EntityType::Email,
                    first_seen: 0,
                },
            )]),
            active_links: vec![],
            calculated_confidence: 42,
        };

        let report = build_analysis_report(
            &profile,
            ResolutionReport {
                score: 42,
                level: "medium".to_string(),
                matched_selectors: vec![],
                evidences: vec![],
            },
            ConflictReport::default(),
            vec![],
            vec!["next".to_string()],
        );

        assert_eq!(report.confidence, 42);
        assert_eq!(report.associated_nodes_count, 1);
        assert_eq!(report.active_links_count, 0);
        assert_eq!(report.next_steps, vec!["next".to_string()]);
    }
}
