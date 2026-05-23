use crate::evidence::{build_evidence_observation, EvidenceInput};
use crate::models::{EntityType, EvidenceRecord, ObservationRecord, SensitivityClass, SourceClass};
use crate::sanitize::{sanitize_text, SanitizeOptions};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramChatSummary {
    pub chat_id: String,
    pub name: String,
    pub chat_type: String,
    pub messages_count: usize,
    pub extracted_observations: usize,
    pub first_message_date: Option<String>,
    pub last_message_date: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelegramExtractedCounts {
    pub emails: usize,
    pub phones: usize,
    pub usernames: usize,
    pub urls: usize,
    pub chats: usize,
    pub groups: usize,
    pub channels: usize,
    pub private_chats: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramExportReport {
    pub source_path: String,
    pub chats_analyzed: usize,
    pub messages_analyzed: usize,
    pub extracted_counts: TelegramExtractedCounts,
    pub chat_summaries: Vec<TelegramChatSummary>,
    pub evidences: Vec<EvidenceRecord>,
    pub observations: Vec<ObservationRecord>,
}

#[derive(Debug, Clone)]
struct ExtractedItem {
    entity_type: EntityType,
    value: String,
    confidence: u8,
    sensitivity: SensitivityClass,
}

pub fn analyze_telegram_export(path: impl AsRef<Path>) -> Result<TelegramExportReport, String> {
    let input_path = resolve_telegram_result_path(path.as_ref())?;
    let data = std::fs::read_to_string(&input_path)
        .map_err(|err| format!("read {}: {}", input_path.display(), err))?;
    let root: Value = serde_json::from_str(&data)
        .map_err(|err| format!("parse {} as JSON: {}", input_path.display(), err))?;

    let chats = root
        .get("chats")
        .and_then(|v| v.get("list"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Telegram result.json does not contain chats.list".to_string())?;

    let mut report = TelegramExportReport {
        source_path: input_path.to_string_lossy().to_string(),
        chats_analyzed: 0,
        messages_analyzed: 0,
        extracted_counts: TelegramExtractedCounts::default(),
        chat_summaries: Vec::new(),
        evidences: Vec::new(),
        observations: Vec::new(),
    };

    let mut global_seen: HashSet<(EntityType, String)> = HashSet::new();

    for chat in chats {
        let chat_id = value_to_string(chat.get("id")).unwrap_or_else(|| "unknown".to_string());
        let name = value_to_string(chat.get("name")).unwrap_or_else(|| "unknown".to_string());
        let chat_type = value_to_string(chat.get("type")).unwrap_or_else(|| "unknown".to_string());
        let messages = match chat.get("messages").and_then(|v| v.as_array()) {
            Some(messages) => messages,
            None => continue,
        };

        report.chats_analyzed += 1;
        report.extracted_counts.chats += 1;
        match chat_type.as_str() {
            "public_channel" | "private_channel" | "channel" => report.extracted_counts.channels += 1,
            "public_supergroup" | "private_supergroup" | "group" => report.extracted_counts.groups += 1,
            "personal_chat" | "saved_messages" | "private" => report.extracted_counts.private_chats += 1,
            _ => {}
        }

        let mut chat_extracted = 0;
        let mut first_date: Option<String> = None;
        let mut last_date: Option<String> = None;

        for message in messages {
            if message.get("type").and_then(|v| v.as_str()) != Some("message") {
                continue;
            }

            report.messages_analyzed += 1;
            let date = value_to_string(message.get("date"));
            if first_date.is_none() {
                first_date = date.clone();
            }
            if date.is_some() {
                last_date = date;
            }

            let text = extract_message_text(message.get("text"));
            if text.trim().is_empty() {
                continue;
            }

            let sanitized = sanitize_text(
                &text,
                &SanitizeOptions { max_chars: 500, ..SanitizeOptions::default() },
            );
            let context = format!(
                "telegram_chat={} type={} date={:?} text={}",
                name, chat_type, last_date, sanitized.value
            );

            for item in extract_items_from_text(&text) {
                let normalized_value = normalize_item_value(&item.value, &item.entity_type);
                let dedupe_key = (item.entity_type.clone(), normalized_value);
                if !global_seen.insert(dedupe_key) {
                    continue;
                }

                let pair = build_evidence_observation(EvidenceInput {
                    source_id: format!("telegram_export:{}", chat_id),
                    source_class: SourceClass::AuthorizedExport,
                    entity_type: item.entity_type.clone(),
                    raw_value: item.value.clone(),
                    raw_context: context.clone(),
                    confidence: item.confidence,
                    sensitivity: item.sensitivity,
                    tags: vec!["telegram_export".to_string(), chat_type.clone()],
                });

                match item.entity_type {
                    EntityType::Email => report.extracted_counts.emails += 1,
                    EntityType::Phone => report.extracted_counts.phones += 1,
                    EntityType::Username => report.extracted_counts.usernames += 1,
                    EntityType::Url => report.extracted_counts.urls += 1,
                    _ => {}
                }

                report.evidences.push(pair.evidence);
                report.observations.push(pair.observation);
                chat_extracted += 1;
            }
        }

        report.chat_summaries.push(TelegramChatSummary {
            chat_id,
            name,
            chat_type,
            messages_count: messages.len(),
            extracted_observations: chat_extracted,
            first_message_date: first_date,
            last_message_date: last_date,
        });
    }

    report.chat_summaries.sort_by(|a, b| {
        b.extracted_observations
            .cmp(&a.extracted_observations)
            .then_with(|| b.messages_count.cmp(&a.messages_count))
    });

    Ok(report)
}

pub fn save_telegram_export_report(report: &TelegramExportReport, path: &str) -> Result<(), String> {
    let json = serde_json::to_string_pretty(report)
        .map_err(|err| format!("serialize telegram export report: {}", err))?;
    std::fs::write(path, json).map_err(|err| format!("write {}: {}", path, err))
}

pub fn observations_as_entity_nodes(report: &TelegramExportReport, limit: usize) -> Vec<crate::models::EntityNode> {
    let mut nodes = Vec::new();
    let mut seen = HashSet::new();

    for obs in &report.observations {
        if nodes.len() >= limit {
            break;
        }
        if !matches!(obs.entity_type, EntityType::Email | EntityType::Phone | EntityType::Username | EntityType::Url) {
            continue;
        }
        let value = if obs.normalized_value.is_empty() {
            obs.value_masked.clone()
        } else {
            obs.normalized_value.clone()
        };
        if value.is_empty() || value.contains("[redacted]") {
            continue;
        }
        let key = format!("{:?}:{}", obs.entity_type, value);
        if seen.insert(key) {
            nodes.push(crate::models::EntityNode {
                value,
                entity_type: obs.entity_type.clone(),
                first_seen: obs.seen_at,
            });
        }
    }

    nodes
}

fn resolve_telegram_result_path(path: &Path) -> Result<PathBuf, String> {
    if path.is_file() {
        return Ok(path.to_path_buf());
    }

    let result_json = path.join("result.json");
    if result_json.is_file() {
        return Ok(result_json);
    }

    Err(format!(
        "Telegram export path must be result.json file or directory containing result.json: {}",
        path.display()
    ))
}

fn value_to_string(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn extract_message_text(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return String::new();
    };

    match value {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .map(text_fragment_to_string)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(""),
        Value::Object(obj) => obj
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        _ => String::new(),
    }
}

fn text_fragment_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Object(obj) => obj
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        Value::Array(items) => items.iter().map(text_fragment_to_string).collect::<Vec<_>>().join(""),
        _ => String::new(),
    }
}

fn extract_items_from_text(text: &str) -> Vec<ExtractedItem> {
    let mut items = Vec::new();
    let mut seen = HashSet::new();

    for raw in text.split_whitespace() {
        let token = clean_token(raw);
        if token.is_empty() {
            continue;
        }

        if is_url(&token) {
            push_item(&mut items, &mut seen, EntityType::Url, normalize_url(&token), 55, SensitivityClass::PublicLow);
            if let Some(username) = username_from_url(&token) {
                push_item(&mut items, &mut seen, EntityType::Username, username, 50, SensitivityClass::PublicLow);
            }
            continue;
        }

        if is_email(&token) {
            push_item(&mut items, &mut seen, EntityType::Email, token.to_lowercase(), 70, SensitivityClass::Personal);
            continue;
        }

        if let Some(username) = username_from_at_token(&token) {
            push_item(&mut items, &mut seen, EntityType::Username, username, 50, SensitivityClass::PublicLow);
            continue;
        }

        if let Some(phone) = phone_from_token(&token) {
            push_item(&mut items, &mut seen, EntityType::Phone, phone, 60, SensitivityClass::Personal);
        }
    }

    items
}

fn push_item(
    items: &mut Vec<ExtractedItem>,
    seen: &mut HashSet<(EntityType, String)>,
    entity_type: EntityType,
    value: String,
    confidence: u8,
    sensitivity: SensitivityClass,
) {
    let normalized = normalize_item_value(&value, &entity_type);
    if normalized.is_empty() {
        return;
    }
    if seen.insert((entity_type.clone(), normalized)) {
        items.push(ExtractedItem { entity_type, value, confidence, sensitivity });
    }
}

fn clean_token(raw: &str) -> String {
    raw.trim_matches(|c: char| {
        matches!(c, ',' | ';' | ':' | ')' | '(' | '[' | ']' | '{' | '}' | '"' | '\'' | '<' | '>' | '!' | '?' | '…')
    })
    .to_string()
}

fn is_url(token: &str) -> bool {
    token.starts_with("http://") || token.starts_with("https://") || token.starts_with("t.me/") || token.starts_with("telegram.me/")
}

fn normalize_url(token: &str) -> String {
    if token.starts_with("t.me/") || token.starts_with("telegram.me/") {
        format!("https://{}", token)
    } else {
        token.to_string()
    }
}

fn username_from_url(token: &str) -> Option<String> {
    let lowered = token.to_lowercase();
    let marker = if lowered.contains("t.me/") {
        "t.me/"
    } else if lowered.contains("telegram.me/") {
        "telegram.me/"
    } else {
        return None;
    };

    let start = lowered.find(marker)? + marker.len();
    let rest = &token[start..];
    let username = rest
        .split(|c| matches!(c, '/' | '?' | '&' | '#'))
        .next()
        .unwrap_or_default();
    validate_username(username).map(|s| s.to_string())
}

fn username_from_at_token(token: &str) -> Option<String> {
    if !token.starts_with('@') || token.contains('@') && token.matches('@').count() > 1 {
        return None;
    }
    validate_username(token.trim_start_matches('@')).map(|s| s.to_string())
}

fn validate_username(username: &str) -> Option<&str> {
    if username.len() < 3 || username.len() > 64 {
        return None;
    }
    if username.starts_with("seed_") || username.contains(':') {
        return None;
    }
    if username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
    {
        Some(username)
    } else {
        None
    }
}

fn is_email(token: &str) -> bool {
    let parts: Vec<&str> = token.split('@').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return false;
    }
    let domain = parts[1];
    domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && token.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '-' | '+'))
}

fn phone_from_token(token: &str) -> Option<String> {
    let has_phone_hint = token.starts_with('+') || token.chars().filter(|c| matches!(c, '-' | '(' | ')' | ' ')).count() > 0;
    let digits: String = token.chars().filter(|c| c.is_ascii_digit()).collect();
    if (7..=15).contains(&digits.len()) && (has_phone_hint || token.starts_with("375") || token.starts_with("80")) {
        Some(digits)
    } else {
        None
    }
}

fn normalize_item_value(value: &str, entity_type: &EntityType) -> String {
    match entity_type {
        EntityType::Phone => value.chars().filter(|c| c.is_ascii_digit()).collect(),
        EntityType::Email => value.trim().to_lowercase(),
        EntityType::Username => value.trim().trim_start_matches('@').to_lowercase(),
        EntityType::Url => value.trim().to_lowercase(),
        _ => value.trim().to_string(),
    }
}

pub fn chat_type_distribution(report: &TelegramExportReport) -> HashMap<String, usize> {
    let mut map = HashMap::new();
    for chat in &report.chat_summaries {
        *map.entry(chat.chat_type.clone()).or_insert(0) += 1;
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn extracts_text_from_mixed_telegram_text_array() {
        let value = serde_json::json!(["hello ", {"type":"link", "text":"https://t.me/test"}]);
        assert_eq!(extract_message_text(Some(&value)), "hello https://t.me/test");
    }

    #[test]
    fn extracts_entities_from_message_text() {
        let items = extract_items_from_text("mail me test@example.com, ping @Fro_ZzZ and https://t.me/test_user +375291234567");
        assert!(items.iter().any(|i| i.entity_type == EntityType::Email && i.value == "test@example.com"));
        assert!(items.iter().any(|i| i.entity_type == EntityType::Username && i.value == "Fro_ZzZ"));
        assert!(items.iter().any(|i| i.entity_type == EntityType::Username && i.value == "test_user"));
        assert!(items.iter().any(|i| i.entity_type == EntityType::Phone && i.value == "375291234567"));
    }

    #[test]
    fn parses_minimal_result_json() {
        let mut path = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        path.push(format!("xgen_tg_export_{}.json", unique));
        let json = serde_json::json!({
            "chats": {
                "list": [
                    {
                        "id": 1,
                        "name": "Test Group",
                        "type": "private_supergroup",
                        "messages": [
                            {"id": 1, "type": "message", "date": "2026-01-01T00:00:00", "text": "hello test@example.com @tester"}
                        ]
                    }
                ]
            }
        });
        let mut file = std::fs::File::create(&path).expect("create temp json");
        write!(file, "{}", json).expect("write temp json");

        let report = analyze_telegram_export(&path).expect("analyze export");
        assert_eq!(report.chats_analyzed, 1);
        assert_eq!(report.messages_analyzed, 1);
        assert!(report.extracted_counts.emails >= 1);
        assert!(report.extracted_counts.usernames >= 1);
        assert!(!report.evidences.is_empty());
        assert!(!report.observations.is_empty());

        let _ = std::fs::remove_file(path);
    }
}
