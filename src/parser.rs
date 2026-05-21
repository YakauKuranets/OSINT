use crate::models::{EntityLink, EntityNode, EntityType, SourceMetadata};
use std::time::{SystemTime, UNIX_EPOCH};

/// Функция для получения текущего Unix-времени в секундах
fn current_unix_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Умный разборщик строки: принимает строку дампа и метаданные источника,
/// возвращает список изолированных сущностей и связей между ними.
pub fn parse_raw_line(line: &str, source: &SourceMetadata) -> (Vec<EntityNode>, Vec<EntityLink>) {
    let mut nodes = Vec::new();
    let mut links = Vec::new();
    let now = current_unix_time();

    // Разделяем строку по распространенным разделителям (запятая, точка с запятой, таб)
    let parts: Vec<String> = line
        .split(|c| c == ';' || c == ',' || c == '\t')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    // Если в строке меньше двух элементов, связывать нечего — выходим
    if parts.len() < 2 {
        return (nodes, links);
    }

    // Временный вектор для классифицированных узлов этой конкретной строки
    let mut current_line_nodes = Vec::new();

    // 1. Анализируем каждый элемент строки и определяем его тип данных
    for part in parts {
        let entity_type = if part.starts_with('+') || (part.chars().all(|c| c.is_numeric()) && part.len() >= 10) {
            EntityType::Phone
        } else if part.contains('@') && part.contains('.') {
            EntityType::Email
        } else if part.starts_with('@') {
            EntityType::Nickname // Telegram-юзернейм
        } else if part.contains('.') && part.chars().filter(|&c| c == '.').count() == 2 {
            EntityType::DateOfBirth // Формат даты ДД.ММ.ГГГГ
        } else if part.starts_with("BANK_") || (part.len() == 8 && part.chars().all(|c| c.is_ascii_alphanumeric())) {
            EntityType::BankIdentifier
        } else {
            EntityType::Nickname // По умолчанию считаем обычным логином/никнеймом
        };

        let node = EntityNode {
            value: part,
            entity_type,
            first_seen: now,
        };
        current_line_nodes.push(node);
    }

    // 2. Строим связи по принципу «все со всеми» внутри этой строки
    for i in 0..current_line_nodes.len() {
        for j in 0..current_line_nodes.len() {
            if i != j {
                let source_node = &current_line_nodes[i];
                let target_node = &current_line_nodes[j];

                // Базовый модификатор веса связи зависит от типов данных
                let mut modifier = 0;
                if source_node.entity_type == EntityType::Phone && target_node.entity_type == EntityType::BankIdentifier {
                    modifier = 20; // Связь телефона и банка очень надежна
                }

                let link = EntityLink {
                    source_node_value: source_node.value.clone(),
                    target_node_value: target_node.value.clone(),
                    weight_modifier: modifier,
                    metadata: source.clone(),
                };
                links.push(link);
            }
        }
    }

    // Переносим созданные узлы в финальный вектор
    nodes = current_line_nodes;

    (nodes, links)
}