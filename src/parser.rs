use crate::models::{EntityLink, EntityNode, EntityType, SourceMetadata};
use std::time::{SystemTime, UNIX_EPOCH};

fn current_unix_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Очищает телефон от всего, кроме цифр, и проверяет длину
fn is_phone(s: &str) -> bool {
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    (7..=15).contains(&digits.len()) && (s.starts_with('+') || s.chars().all(|c| c.is_ascii_digit()))
}

/// Простая проверка email без ReDoS
fn is_email(s: &str) -> bool {
    if let Some((local, domain)) = s.split_once('@') {
        !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
    } else {
        false
    }
}

/// Проверка формата ДД.ММ.ГГГГ с валидными диапазонами чисел
fn is_valid_date(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 3 {
        return false;
    }
    if let (Ok(day), Ok(month), Ok(year)) = (
        parts[0].parse::<u32>(),
        parts[1].parse::<u32>(),
        parts[2].parse::<i32>(),
    ) {
        if day < 1 || day > 31 || month < 1 || month > 12 {
            return false;
        }
        if year >= 1920 && year <= 2026 {
            return true;
        }
    }
    false
}

/// Умный разборщик строки: принимает строку дампа и метаданные источника,
/// возвращает список изолированных сущностей и связей между ними.
pub fn parse_raw_line(line: &str, source: &SourceMetadata) -> (Vec<EntityNode>, Vec<EntityLink>) {
    let now = current_unix_time();
    let mut nodes = Vec::new();
    let mut links = Vec::new();

    // Фильтрация управляющих символов (кроме табуляции)
    let line: String = line.chars().filter(|c| !c.is_control() || *c == '\t').collect();
    let parts: Vec<String> = line
        .split(|c: char| c == ';' || c == ',' || c == '\t')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if parts.len() < 2 {
        return (nodes, links);
    }

    // Классификация частей
    for part in &parts {
        let entity_type = if is_phone(part) {
            EntityType::Phone
        } else if is_email(part) {
            EntityType::Email
        } else if is_valid_date(part) {
            EntityType::DateOfBirth
        } else if part.chars().all(|c| c.is_alphanumeric() || c == '_') {
            EntityType::Nickname
        } else {
            continue; // Пропускаем нераспознанные токены
        };

        nodes.push(EntityNode {
            value: part.clone(),
            entity_type,
            first_seen: now,
        });
    }

    // Строим связи только с первым значимым узлом (центральная точка строки)
    let center = match nodes.first() {
        Some(n) => n,
        None => return (nodes, links),
    };

    for node in nodes.iter().skip(1) {
        let weight = match (&center.entity_type, &node.entity_type) {
            (EntityType::Phone, EntityType::BankIdentifier) => 20,
            _ => 0,
        };
        links.push(EntityLink {
            source_node_value: center.value.clone(),
            target_node_value: node.value.clone(),
            weight_modifier: weight,
            metadata: source.clone(),
        });
    }

    (nodes, links)
}