use crate::models::{EntityLink, EntityNode, EntityType, IdentityProfile, SourceClass};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const CURRENT_YEAR: u32 = 2026;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictKind {
    DirtyOnlyEvidence,
    WeakSingleSourceLink,
    StaleSource,
    SourceClassConflict,
    DuplicateEntity,
    AiOnlyEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictFinding {
    pub kind: ConflictKind,
    pub severity: ConflictSeverity,
    pub entity_value: String,
    pub source_ids: Vec<String>,
    pub message: String,
    pub recommended_action: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConflictReport {
    pub findings: Vec<ConflictFinding>,
}

impl ConflictReport {
    pub fn has_high_risk(&self) -> bool {
        self.findings.iter().any(|finding| {
            matches!(finding.severity, ConflictSeverity::High | ConflictSeverity::Critical)
        })
    }

    pub fn severity_score(&self) -> u8 {
        let score: u32 = self
            .findings
            .iter()
            .map(|finding| match finding.severity {
                ConflictSeverity::Info => 1,
                ConflictSeverity::Low => 5,
                ConflictSeverity::Medium => 15,
                ConflictSeverity::High => 30,
                ConflictSeverity::Critical => 50,
            })
            .sum();
        score.min(100) as u8
    }

    pub fn count_by_severity(&self, severity: ConflictSeverity) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.severity == severity)
            .count()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ConflictEngine;

impl ConflictEngine {
    pub fn analyze(profile: &IdentityProfile) -> ConflictReport {
        let mut findings = Vec::new();
        findings.extend(Self::detect_dirty_only_evidence(profile));
        findings.extend(Self::detect_ai_only_evidence(profile));
        findings.extend(Self::detect_weak_single_source_links(profile));
        findings.extend(Self::detect_stale_sources(profile));
        findings.extend(Self::detect_source_class_conflicts(profile));
        findings.extend(Self::detect_duplicate_entities(profile));
        findings.sort_by(|a, b| severity_rank(b.severity).cmp(&severity_rank(a.severity)));
        ConflictReport { findings }
    }

    fn detect_dirty_only_evidence(profile: &IdentityProfile) -> Vec<ConflictFinding> {
        let mut by_entity: HashMap<String, Vec<&EntityLink>> = HashMap::new();
        for link in &profile.active_links {
            by_entity.entry(link.target_node_value.clone()).or_default().push(link);
        }

        by_entity
            .into_iter()
            .filter_map(|(entity, links)| {
                if links.is_empty() {
                    return None;
                }
                let all_dirty = links.iter().all(|link| is_dirty(link.metadata.class));
                let has_strong_weight = links.iter().any(|link| link.weight_modifier >= 25);
                if all_dirty {
                    Some(ConflictFinding {
                        kind: ConflictKind::DirtyOnlyEvidence,
                        severity: if has_strong_weight { ConflictSeverity::High } else { ConflictSeverity::Medium },
                        entity_value: entity,
                        source_ids: unique_source_ids(&links),
                        message: "Сущность подтверждается только грязными/непроверенными источниками".to_string(),
                        recommended_action: "Оставить как гипотезу и искать независимое подтверждение через PublicOSINT/Verified источники".to_string(),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    fn detect_ai_only_evidence(profile: &IdentityProfile) -> Vec<ConflictFinding> {
        let mut by_entity: HashMap<String, Vec<&EntityLink>> = HashMap::new();
        for link in &profile.active_links {
            by_entity.entry(link.target_node_value.clone()).or_default().push(link);
        }

        by_entity
            .into_iter()
            .filter_map(|(entity, links)| {
                if !links.is_empty() && links.iter().all(|link| link.metadata.class == SourceClass::AIDerived) {
                    Some(ConflictFinding {
                        kind: ConflictKind::AiOnlyEvidence,
                        severity: ConflictSeverity::Medium,
                        entity_value: entity,
                        source_ids: unique_source_ids(&links),
                        message: "Сущность получена только из AI-derived вывода".to_string(),
                        recommended_action: "Не включать как факт; подтвердить через evidence из реального источника".to_string(),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    fn detect_weak_single_source_links(profile: &IdentityProfile) -> Vec<ConflictFinding> {
        let mut by_entity: HashMap<String, Vec<&EntityLink>> = HashMap::new();
        for link in &profile.active_links {
            by_entity.entry(link.target_node_value.clone()).or_default().push(link);
        }

        by_entity
            .into_iter()
            .filter_map(|(entity, links)| {
                if links.len() == 1 {
                    let link = links[0];
                    let weak_source = matches!(
                        link.metadata.class,
                        SourceClass::DirtyPublicData
                            | SourceClass::UnverifiedDump
                            | SourceClass::AIDerived
                            | SourceClass::LocalImport
                    );
                    if weak_source || link.weight_modifier < 10 {
                        return Some(ConflictFinding {
                            kind: ConflictKind::WeakSingleSourceLink,
                            severity: ConflictSeverity::Low,
                            entity_value: entity,
                            source_ids: vec![link.metadata.source_id.clone()],
                            message: "Связь основана только на одном слабом источнике или низком весе".to_string(),
                            recommended_action: "Искать второй независимый источник или оставить связь как weak/possible".to_string(),
                        });
                    }
                }
                None
            })
            .collect()
    }

    fn detect_stale_sources(profile: &IdentityProfile) -> Vec<ConflictFinding> {
        let mut findings = Vec::new();
        let mut seen = HashSet::new();

        for link in &profile.active_links {
            if link.metadata.data_actual_year == 0 || link.metadata.data_actual_year > CURRENT_YEAR {
                continue;
            }
            let age = CURRENT_YEAR - link.metadata.data_actual_year;
            if age > 5 {
                let key = format!("{}::{}", link.target_node_value, link.metadata.source_id);
                if seen.insert(key) {
                    findings.push(ConflictFinding {
                        kind: ConflictKind::StaleSource,
                        severity: ConflictSeverity::Medium,
                        entity_value: link.target_node_value.clone(),
                        source_ids: vec![link.metadata.source_id.clone()],
                        message: format!("Источник устарел: возраст данных примерно {} лет", age),
                        recommended_action: "Понизить confidence и проверить свежими public/verified источниками".to_string(),
                    });
                }
            }
        }

        findings
    }

    fn detect_source_class_conflicts(profile: &IdentityProfile) -> Vec<ConflictFinding> {
        let mut classes_by_entity: HashMap<String, HashSet<SourceClass>> = HashMap::new();
        let mut sources_by_entity: HashMap<String, HashSet<String>> = HashMap::new();

        for link in &profile.active_links {
            classes_by_entity
                .entry(link.target_node_value.clone())
                .or_default()
                .insert(link.metadata.class);
            sources_by_entity
                .entry(link.target_node_value.clone())
                .or_default()
                .insert(link.metadata.source_id.clone());
        }

        classes_by_entity
            .into_iter()
            .filter_map(|(entity, classes)| {
                let has_verified = classes.iter().any(|class| is_verified(*class));
                let has_dirty = classes.iter().any(|class| is_dirty(*class));
                let has_ai = classes.contains(&SourceClass::AIDerived);
                if has_verified && (has_dirty || has_ai) {
                    Some(ConflictFinding {
                        kind: ConflictKind::SourceClassConflict,
                        severity: ConflictSeverity::Info,
                        entity_value: entity.clone(),
                        source_ids: sources_by_entity
                            .get(&entity)
                            .map(|set| sorted_vec(set))
                            .unwrap_or_default(),
                        message: "Одна сущность встречается в источниках разного класса доверия".to_string(),
                        recommended_action: "В отчете разделить verified/public/dirty evidence и не смешивать их без пояснения".to_string(),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    fn detect_duplicate_entities(profile: &IdentityProfile) -> Vec<ConflictFinding> {
        let mut normalized: HashMap<(EntityType, String), Vec<&EntityNode>> = HashMap::new();
        for node in profile.associated_nodes.values() {
            let key = (node.entity_type.clone(), normalize_entity_value(&node.value, &node.entity_type));
            normalized.entry(key).or_default().push(node);
        }

        normalized
            .into_iter()
            .filter_map(|((entity_type, normalized_value), nodes)| {
                let unique_values: HashSet<String> = nodes.iter().map(|node| node.value.clone()).collect();
                if unique_values.len() > 1 {
                    Some(ConflictFinding {
                        kind: ConflictKind::DuplicateEntity,
                        severity: ConflictSeverity::Low,
                        entity_value: format!("{:?}:{}", entity_type, normalized_value),
                        source_ids: Vec::new(),
                        message: "Найдены разные записи, которые нормализуются в одну сущность".to_string(),
                        recommended_action: "Склеить aliases или хранить как варианты одной сущности".to_string(),
                    })
                } else {
                    None
                }
            })
            .collect()
    }
}

fn is_dirty(source_class: SourceClass) -> bool {
    matches!(source_class, SourceClass::DirtyPublicData | SourceClass::UnverifiedDump)
}

fn is_verified(source_class: SourceClass) -> bool {
    matches!(source_class, SourceClass::VerifiedOfficial | SourceClass::VerifiedRegistry)
}

fn unique_source_ids(links: &[&EntityLink]) -> Vec<String> {
    let mut set = HashSet::new();
    for link in links {
        set.insert(link.metadata.source_id.clone());
    }
    sorted_vec(&set)
}

fn sorted_vec(set: &HashSet<String>) -> Vec<String> {
    let mut values: Vec<String> = set.iter().cloned().collect();
    values.sort();
    values
}

fn severity_rank(severity: ConflictSeverity) -> u8 {
    match severity {
        ConflictSeverity::Info => 0,
        ConflictSeverity::Low => 1,
        ConflictSeverity::Medium => 2,
        ConflictSeverity::High => 3,
        ConflictSeverity::Critical => 4,
    }
}

fn normalize_entity_value(value: &str, entity_type: &EntityType) -> String {
    match entity_type {
        EntityType::Phone => value.chars().filter(|c| c.is_ascii_digit()).collect(),
        EntityType::Email => value.trim().to_lowercase(),
        EntityType::Nickname | EntityType::Username => value.trim().trim_start_matches('@').to_lowercase(),
        EntityType::Domain | EntityType::Url => value.trim().to_lowercase(),
        _ => value.trim().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{EntityLink, EntityNode, IdentityProfile, SourceMetadata};

    fn link(target: &str, class: SourceClass, source_id: &str, weight: i16, year: u32) -> EntityLink {
        EntityLink {
            source_node_value: "root".to_string(),
            target_node_value: target.to_string(),
            weight_modifier: weight,
            metadata: SourceMetadata {
                source_id: source_id.to_string(),
                class,
                import_timestamp: 0,
                data_actual_year: year,
            },
        }
    }

    fn profile(links: Vec<EntityLink>) -> IdentityProfile {
        let root = EntityNode { value: "root".to_string(), entity_type: EntityType::Nickname, first_seen: 0 };
        let mut associated_nodes = HashMap::new();
        for link in &links {
            associated_nodes.insert(
                link.target_node_value.clone(),
                EntityNode { value: link.target_node_value.clone(), entity_type: EntityType::Email, first_seen: 0 },
            );
        }
        IdentityProfile {
            root_entity: root,
            associated_nodes,
            active_links: links,
            calculated_confidence: 0,
        }
    }

    #[test]
    fn dirty_only_evidence_is_flagged() {
        let report = ConflictEngine::analyze(&profile(vec![
            link("a@example.com", SourceClass::DirtyPublicData, "dirty", 30, 2026),
        ]));
        assert!(report.findings.iter().any(|f| f.kind == ConflictKind::DirtyOnlyEvidence));
    }

    #[test]
    fn stale_source_is_flagged() {
        let report = ConflictEngine::analyze(&profile(vec![
            link("a@example.com", SourceClass::PublicOSINT, "old_public", 10, 2018),
        ]));
        assert!(report.findings.iter().any(|f| f.kind == ConflictKind::StaleSource));
    }

    #[test]
    fn verified_and_dirty_mix_is_source_class_conflict() {
        let report = ConflictEngine::analyze(&profile(vec![
            link("a@example.com", SourceClass::VerifiedOfficial, "official", 20, 2026),
            link("a@example.com", SourceClass::DirtyPublicData, "dirty", 10, 2026),
        ]));
        assert!(report.findings.iter().any(|f| f.kind == ConflictKind::SourceClassConflict));
    }
}
