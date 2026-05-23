use crate::models::{IdentityProfile, ResolutionEvidence, ResolutionReport, SourceClass};

const CURRENT_YEAR: u32 = 2026;

pub fn evaluate_profile(profile: &mut IdentityProfile) {
    if profile.associated_nodes.is_empty() {
        profile.calculated_confidence = 0;
        return;
    }

    let mut total_score: i32 = 30; // базовое доверие

    // 1. Уникальные источники (кросс-верификация)
    let mut unique_sources = std::collections::HashSet::new();
    for link in &profile.active_links {
        unique_sources.insert(link.metadata.source_id.clone());
    }
    total_score += (unique_sources.len() as i32) * 20; // увеличили бонус до 20

    // 2. Анализ каждой связи
    for link in &profile.active_links {
        total_score += link.weight_modifier as i32;

        match link.metadata.class {
            SourceClass::VerifiedRegistry => total_score += 10,
            SourceClass::PublicOSINT => total_score += 0,
            SourceClass::UnverifiedDump => total_score -= 5, // было -15, стало -5
        }

        if link.metadata.data_actual_year > 0 && link.metadata.data_actual_year <= CURRENT_YEAR {
            let data_age = CURRENT_YEAR - link.metadata.data_actual_year;
            if data_age <= 1 {
                total_score += 10;
            } else if data_age > 5 {
                total_score -= 20;
            }
        }
    }

    profile.calculated_confidence = total_score.clamp(0, 100) as u8;
}

pub fn build_resolution_report(profile: &IdentityProfile) -> ResolutionReport {
    let mut matched_selectors = std::collections::HashSet::new();
    let mut evidences = Vec::new();

    for link in &profile.active_links {
        matched_selectors.insert(link.source_node_value.clone());
        matched_selectors.insert(link.target_node_value.clone());

        evidences.push(ResolutionEvidence {
            signal: format!("{:?}->{:?}", link.source_node_value, link.target_node_value),
            weight: link.weight_modifier,
            source_id: link.metadata.source_id.clone(),
            note: format!(
                "class={:?}, year={}",
                link.metadata.class, link.metadata.data_actual_year
            ),
        });
    }

    let level = match profile.calculated_confidence {
        0..=34 => "low",
        35..=69 => "medium",
        _ => "high",
    }
    .to_string();

    ResolutionReport {
        score: profile.calculated_confidence,
        level,
        matched_selectors: matched_selectors.into_iter().collect(),
        evidences,
    }
}
