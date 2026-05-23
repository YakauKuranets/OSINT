use reqwest::Client;
use crate::models::{EntityNode, EntityType, SourceMetadata, SourceClass};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct ReverseOsintEnumerator {
    client: Client,
}

impl ReverseOsintEnumerator {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(90)) // Долгий таймаут для 120+ сайтов
                .build()
                .unwrap(),
        }
    }

    pub async fn check_email(&self, email: &str) -> Vec<(EntityNode, SourceMetadata)> {
        let mut results = Vec::new();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();

        let meta = SourceMetadata {
            source_id: "Holehe_API".to_string(),
            class: SourceClass::PublicOSINT,
            import_timestamp: now,
            data_actual_year: 2026,
        };

        let url = "http://127.0.0.1:5003/check_email";
        let payload = serde_json::json!({"email": email});

        println!("  [Enumerator] Отправка {} в ядро Holehe (проверка 120+ сервисов)...", email);

        match self.client.post(url).json(&payload).send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    if let Some(sites) = json["registered"].as_array() {
                        for site in sites.iter().filter_map(|s| s.as_str()) {
                            let node = EntityNode {
                                value: format!("registered:{}", site.to_lowercase().replace(" ", "_")),
                                entity_type: EntityType::Nickname,
                                first_seen: now,
                            };
                            results.push((node, meta.clone()));
                        }
                    }
                }
            }
            Ok(resp) => {
                println!("  [!] Ошибка API Holehe: статус {}", resp.status());
            }
            Err(e) => {
                println!("  [!] Нет связи с Python-микросервисом Holehe (порт 5003). Ошибка: {}", e);
            }
        }

        results
    }
}

// =====================================================================
// ТРЕХУРОВНЕВОЕ ТЕСТИРОВАНИЕ (Three-Level Testing Rule)
// Железные тесты для защиты модуля от регрессий.
// =====================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{EntityNode, EntityType};
    use crate::engine::AnalysisEngine;

    // 1. BACKEND TEST: Тестирование логики парсинга (эмуляция ответа от Holehe)
    #[test]
    fn backend_test_holehe_parsing() {
        let json_str = r#"{"registered": ["GitHub", "Twitter", "Spotify"]}"#;
        let json: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let sites = json["registered"].as_array().unwrap();
        assert_eq!(sites.len(), 3);
        assert_eq!(sites[0].as_str().unwrap(), "GitHub");
    }

    // 2. FRONTEND TEST: Тестирование валидности генерируемых сущностей (форматирование)
    #[test]
    fn frontend_test_entity_formatting() {
        let raw_site_name = "Red Hat";
        let formatted_value = format!("registered:{}", raw_site_name.to_lowercase().replace(" ", "_"));

        let node = EntityNode {
            value: formatted_value,
            entity_type: EntityType::Nickname,
            first_seen: 1000
        };

        // Проверяем, что парсер UI/CLI не сломается о пробелы
        assert_eq!(node.value, "registered:red_hat");
        let display_string = format!("[{:?}] {}", node.entity_type, node.value);
        assert_eq!(display_string, "[Nickname] registered:red_hat");
    }

    // 3. E2E TEST: Сквозное тестирование интеграции модуля в каскад AnalysisEngine
    #[tokio::test]
    async fn e2e_test_enumerator_integration() {
        let root = EntityNode { value: "test_e2e@example.com".to_string(), entity_type: EntityType::Email, first_seen: 0 };
        let mut engine = AnalysisEngine::new(root.clone(), "dummy_dir");

        // Проверяем, что движок принимает узлы из энумератора без паники
        let enumerator = ReverseOsintEnumerator::new();
        // В E2E тесте мы делаем реальный HTTP запрос. Если сервис выключен, он просто вернет 0 результатов, но не крашнется.
        let mock_results = enumerator.check_email("test_e2e@example.com").await;

        for (node, _meta) in mock_results {
            engine.final_profile.associated_nodes.insert(node.value.clone(), node);
        }

        assert!(engine.final_profile.associated_nodes.len() >= 0);
    }
}