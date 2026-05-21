use crate::models::{IdentityProfile, SourceClass};

/// Текущий год для расчета временного износа данных
const CURRENT_YEAR: u32 = 2026;

/// Модуль интеллектуальной оценки профиля сущности.
/// Рассчитывает итоговый процент доверия (0-100) и обновляет профиль.
pub fn evaluate_profile(profile: &mut IdentityProfile) {
    if profile.associated_nodes.is_empty() {
        profile.calculated_confidence = 0;
        return;
    }

    let mut total_score: i32 = 30; // Базовый уровень доверия к существованию сущности

    // 1. Оценка многообразия источников (Кросс-верификация)
    // Собираем уникальные ID источников, которые подтверждают этот профиль
    let mut unique_sources = std::collections::HashSet::new();
    for link in &profile.active_links {
        unique_sources.insert(link.metadata.source_id.clone());
    }

    // За каждый независимый источник, подтверждающий связи, добавляем баллы
    total_score += (unique_sources.len() as i32) * 15;

    // 2. Анализ классов источников и весов связей
    for link in &profile.active_links {
        // Добавляем индивидуальный модификатор веса связи (например, телефон + банк = надежно)
        total_score += link.weight_modifier as i32;

        // Корректируем балл в зависимости от класса надежности источника
        match link.metadata.class {
            SourceClass::VerifiedRegistry => total_score += 10,
            SourceClass::PublicOSINT => total_score += 0,
            SourceClass::UnverifiedDump => total_score -= 15, // Серые дампы изначально снижают доверие
        }

        // 3. Расчет временного износа (Password/Data Aging)
        if link.metadata.data_actual_year > 0 && link.metadata.data_actual_year <= CURRENT_YEAR {
            let data_age = CURRENT_YEAR - link.metadata.data_actual_year;
            if data_age <= 1 {
                total_score += 10; // Данные свежие (текущий или прошлый год)
            } else if data_age > 5 {
                total_score -= 20; // Данные устарели (старше 5 лет)
            }
        }
    }

    // Жестко ограничиваем итоговый результат в диапазоне от 0 до 100%
    profile.calculated_confidence = total_score.clamp(0, 100) as u8;
}