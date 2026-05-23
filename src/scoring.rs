use crate::models::{IdentityProfile, ResolutionEvidence, ResolutionReport, SourceClass};

#[derive(Debug, Clone)]
pub struct SourceHealth {
    pub source_id: String,
    pub links: usize,
    pub avg_weight: f32,
    pub reliability: String,
}

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

pub fn suggest_next_steps(profile: &IdentityProfile) -> Vec<String> {
    let mut has_email = false;
    let mut has_phone = false;
    let mut has_nickname = false;
    let mut has_full_name = false;
    let mut has_country = false;

    for node in profile.associated_nodes.values() {
        match node.entity_type {
            crate::models::EntityType::Email => has_email = true,
            crate::models::EntityType::Phone => has_phone = true,
            crate::models::EntityType::Nickname => has_nickname = true,
            crate::models::EntityType::FullName => has_full_name = true,
            crate::models::EntityType::Country => has_country = true,
            _ => {}
        }
    }

    let mut steps = Vec::new();
    if has_nickname && !has_email {
        steps.push("Расширить поиск по никнейму в соцсетях, чтобы найти email/контакты".to_string());
    }
    if has_email && !has_phone {
        steps.push("Проверить email через утечки/регистрации для извлечения связанных телефонов".to_string());
    }
    if has_phone && !has_full_name {
        steps.push("Углубить phone-intel (carrier/geo/профили объявлений) для получения ФИО".to_string());
    }
    if has_full_name && !has_country {
        steps.push("Добавить страну/регион для повышения точности сопоставления личности".to_string());
    }
    if steps.is_empty() {
        steps.push("Запустить второй каскад: у вас уже достаточно связей для глубокой корреляции".to_string());
    }
    steps
}

pub fn source_health_summary(profile: &IdentityProfile) -> Vec<SourceHealth> {
    let mut stats: std::collections::HashMap<String, (usize, i32)> = std::collections::HashMap::new();
    for link in &profile.active_links {
        let entry = stats.entry(link.metadata.source_id.clone()).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += link.weight_modifier as i32;
    }

    let mut items: Vec<SourceHealth> = stats
        .into_iter()
        .map(|(source_id, (links, total_weight))| {
            let avg_weight = if links == 0 { 0.0 } else { total_weight as f32 / links as f32 };
            let reliability = if links >= 8 && avg_weight >= 20.0 {
                "high"
            } else if links >= 3 && avg_weight >= 10.0 {
                "medium"
            } else {
                "low"
            }
            .to_string();

            SourceHealth {
                source_id,
                links,
                avg_weight,
                reliability,
            }
        })
        .collect();

    items.sort_by(|a, b| {
        b.links
            .cmp(&a.links)
            .then_with(|| b.avg_weight.partial_cmp(&a.avg_weight).unwrap_or(std::cmp::Ordering::Equal))
    });
    items
}
