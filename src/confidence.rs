use crate::models::{IdentityProfile, SourceClass};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const CURRENT_YEAR: u32 = 2026;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfidenceRuleKind {
    NoLinks,
    SingleSource,
    DirtyOnly,
    AiOnly,
    LocalOnly,
    NoVerifiedSources,
    WeakEvidenceDominant,
    StaleEvidenceDominant,
    LowIndependence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceGuardrail {
    pub kind: ConfidenceRuleKind,
    pub cap: u8,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceReport {
    pub original_score: u8,
    pub adjusted_score: u8,
    pub unique_sources: usize,
    pub unique_source_classes: usize,
    pub verified_sources: usize,
    pub public_sources: usize,
    pub dirty_sources: usize,
    pub ai_sources: usize,
    pub local_sources: usize,
    pub total_links: usize,
    pub weak_links: usize,
    pub stale_links: usize,
    pub applied_guardrails: Vec<ConfidenceGuardrail>,
}

impl ConfidenceReport {
    pub fn was_capped(&self) -> bool {
        self.adjusted_score < self.original_score
    }
}

pub fn apply_confidence_guardrails(profile: &mut IdentityProfile) -> ConfidenceReport {
    let report = analyze_confidence(profile);
    profile.calculated_confidence = report.adjusted_score;
    report
}

pub fn analyze_confidence(profile: &IdentityProfile) -> ConfidenceReport {
    let original_score = profile.calculated_confidence;
    let mut unique_sources = HashSet::new();
    let mut unique_classes = HashSet::new();
    let mut class_counts: HashMap<SourceClass, usize> = HashMap::new();
    let mut verified_sources = HashSet::new();
    let mut public_sources = HashSet::new();
    let mut dirty_sources = HashSet::new();
    let mut ai_sources = HashSet::new();
    let mut local_sources = HashSet::new();
    let mut weak_links = 0usize;
    let mut stale_links = 0usize;

    for link in &profile.active_links {
        unique_sources.insert(link.metadata.source_id.clone());
        unique_classes.insert(link.metadata.class);
        *class_counts.entry(link.metadata.class).or_insert(0) += 1;

        match link.metadata.class {
            SourceClass::VerifiedOfficial | SourceClass::VerifiedRegistry => {
                verified_sources.insert(link.metadata.source_id.clone());
            }
            SourceClass::PublicOSINT | SourceClass::AuthorizedExport => {
                public_sources.insert(link.metadata.source_id.clone());
            }
            SourceClass::DirtyPublicData | SourceClass::UnverifiedDump => {
                dirty_sources.insert(link.metadata.source_id.clone());
            }
            SourceClass::AIDerived => {
                ai_sources.insert(link.metadata.source_id.clone());
            }
            SourceClass::LocalImport => {
                local_sources.insert(link.metadata.source_id.clone());
            }
        }

        if link.weight_modifier < 10 {
            weak_links += 1;
        }
        if link.metadata.data_actual_year > 0 && link.metadata.data_actual_year <= CURRENT_YEAR {
            if CURRENT_YEAR - link.metadata.data_actual_year > 5 {
                stale_links += 1;
            }
        }
    }

    let total_links = profile.active_links.len();
    let mut guardrails = Vec::new();

    if total_links == 0 {
        guardrails.push(ConfidenceGuardrail {
            kind: ConfidenceRuleKind::NoLinks,
            cap: 20,
            reason: "Нет активных связей/evidence links; нельзя считать профиль подтвержденным".to_string(),
        });
    }

    if total_links > 0 && unique_sources.len() <= 1 {
        guardrails.push(ConfidenceGuardrail {
            kind: ConfidenceRuleKind::SingleSource,
            cap: 55,
            reason: "Все связи идут из одного источника; нет независимой кросс-проверки".to_string(),
        });
    }

    if total_links > 0 && dirty_sources.len() > 0 && dirty_sources.len() == unique_sources.len() {
        guardrails.push(ConfidenceGuardrail {
            kind: ConfidenceRuleKind::DirtyOnly,
            cap: 35,
            reason: "Все источники dirty/unverified; это только гипотеза, не подтверждение".to_string(),
        });
    }

    if total_links > 0 && ai_sources.len() > 0 && ai_sources.len() == unique_sources.len() {
        guardrails.push(ConfidenceGuardrail {
            kind: ConfidenceRuleKind::AiOnly,
            cap: 30,
            reason: "Все связи AI-derived; AI не является первичным evidence".to_string(),
        });
    }

    if total_links > 0 && local_sources.len() > 0 && local_sources.len() == unique_sources.len() {
        guardrails.push(ConfidenceGuardrail {
            kind: ConfidenceRuleKind::LocalOnly,
            cap: 60,
            reason: "Все связи из local/import/вводимых данных; нет внешнего публичного подтверждения".to_string(),
        });
    }

    if total_links > 0 && verified_sources.is_empty() {
        let cap = if public_sources.len() >= 2 { 85 } else { 70 };
        guardrails.push(ConfidenceGuardrail {
            kind: ConfidenceRuleKind::NoVerifiedSources,
            cap,
            reason: "Нет verified official/registry источников; высокий confidence ограничен".to_string(),
        });
    }

    if total_links >= 3 && weak_links * 2 >= total_links {
        guardrails.push(ConfidenceGuardrail {
            kind: ConfidenceRuleKind::WeakEvidenceDominant,
            cap: 65,
            reason: "Большинство связей слабые по весу; нельзя поднимать confidence до максимума".to_string(),
        });
    }

    if total_links >= 3 && stale_links * 2 >= total_links {
        guardrails.push(ConfidenceGuardrail {
            kind: ConfidenceRuleKind::StaleEvidenceDominant,
            cap: 60,
            reason: "Большинство связей устаревшие; требуется свежая перепроверка".to_string(),
        });
    }

    if total_links >= 4 && unique_classes.len() <= 1 {
        guardrails.push(ConfidenceGuardrail {
            kind: ConfidenceRuleKind::LowIndependence,
            cap: 75,
            reason: "Много связей, но все одного класса источника; независимость подтверждения низкая".to_string(),
        });
    }

    let adjusted_score = guardrails
        .iter()
        .map(|rule| rule.cap)
        .min()
        .map(|cap| original_score.min(cap))
        .unwrap_or(original_score);

    ConfidenceReport {
        original_score,
        adjusted_score,
        unique_sources: unique_sources.len(),
        unique_source_classes: unique_classes.len(),
        verified_sources: verified_sources.len(),
        public_sources: public_sources.len(),
        dirty_sources: dirty_sources.len(),
        ai_sources: ai_sources.len(),
        local_sources: local_sources.len(),
        total_links,
        weak_links,
        stale_links,
        applied_guardrails: guardrails,
    }
}

pub fn save_confidence_report(report: &ConfidenceReport, path: &str) -> Result<(), String> {
    let json = serde_json::to_string_pretty(report)
        .map_err(|err| format!("serialize confidence report: {}", err))?;
    std::fs::write(path, json).map_err(|err| format!("write {}: {}", path, err))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{EntityLink, EntityNode, EntityType, SourceMetadata};
    use std::collections::HashMap;

    fn profile_with_links(links: Vec<EntityLink>, score: u8) -> IdentityProfile {
        IdentityProfile {
            root_entity: EntityNode { value: "root".to_string(), entity_type: EntityType::Nickname, first_seen: 0 },
            associated_nodes: HashMap::new(),
            active_links: links,
            calculated_confidence: score,
        }
    }

    fn link(source_id: &str, class: SourceClass, weight: i16) -> EntityLink {
        EntityLink {
            source_node_value: "root".to_string(),
            target_node_value: format!("target_{}", source_id),
            weight_modifier: weight,
            metadata: SourceMetadata {
                source_id: source_id.to_string(),
                class,
                import_timestamp: 0,
                data_actual_year: CURRENT_YEAR,
            },
        }
    }

    #[test]
    fn caps_no_links() {
        let report = analyze_confidence(&profile_with_links(vec![], 100));
        assert_eq!(report.adjusted_score, 20);
    }

    #[test]
    fn caps_single_source() {
        let report = analyze_confidence(&profile_with_links(vec![link("s1", SourceClass::PublicOSINT, 30)], 100));
        assert!(report.adjusted_score <= 55);
    }

    #[test]
    fn caps_dirty_only_hard() {
        let report = analyze_confidence(&profile_with_links(
            vec![link("d1", SourceClass::DirtyPublicData, 30), link("d2", SourceClass::UnverifiedDump, 30)],
            100,
        ));
        assert_eq!(report.adjusted_score, 35);
    }

    #[test]
    fn public_without_verified_can_not_reach_100() {
        let report = analyze_confidence(&profile_with_links(
            vec![link("p1", SourceClass::PublicOSINT, 30), link("p2", SourceClass::AuthorizedExport, 30)],
            100,
        ));
        assert!(report.adjusted_score <= 85);
    }
}
