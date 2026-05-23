use crate::models::{IdentityProfile, EntityType};
use std::fs;

pub struct DorkGenerator;

impl DorkGenerator {
    /// Генерирует продвинутые Google/Yandex запросы на основе собранного профиля
    pub fn generate_dorks(profile: &IdentityProfile, output_path: &str) -> Vec<String> {
        let mut dorks = Vec::new();

        // 1. Дорки для корневого узла
        Self::append_dorks_for_entity(&profile.root_entity.value, &profile.root_entity.entity_type, &mut dorks);

        // 2. Дорки для всех найденных связей (номера, почты, никнеймы)
        for (value, node) in &profile.associated_nodes {
            // Исключаем системные теги вроде registered:whatsapp
            if !value.starts_with("registered:") && !value.starts_with("breach:") {
                Self::append_dorks_for_entity(value, &node.entity_type, &mut dorks);
            }
        }

        // 3. Перекрестные дорки (ищем совпадения, где почта и телефон засветились на одной странице)
        let emails: Vec<&String> = profile.associated_nodes.iter()
            .filter(|(_, n)| n.entity_type == EntityType::Email)
            .map(|(k, _)| k)
            .collect();

        let phones: Vec<&String> = profile.associated_nodes.iter()
            .filter(|(_, n)| n.entity_type == EntityType::Phone)
            .map(|(k, _)| k)
            .collect();

        if !emails.is_empty() && !phones.is_empty() {
            dorks.push(format!("\"{}\" AND \"{}\"", emails[0], phones[0]));
        }

        // Сохраняем в файл для удобного копирования
        Self::save_to_file(&dorks, output_path);

        dorks
    }

    fn append_dorks_for_entity(value: &str, entity_type: &EntityType, dorks: &mut Vec<String>) {
        match entity_type {
            EntityType::Email => {
                // Ищем утечки в открытых текстовиках и логах
                dorks.push(format!("\"{}\" ext:txt OR ext:log OR ext:csv", value));
                // Ищем упоминания в Pastebin (часто сливают базы)
                dorks.push(format!("site:pastebin.com \"{}\"", value));
                // Поиск по слитым резюме и базам
                dorks.push(format!("\"{}\" inurl:resume OR inurl:cv", value));
            }
            EntityType::Phone => {
                // Если телефон нормализован (начинается с +), делаем вариации
                let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
                if digits.len() >= 11 {
                    // Вариация со скобками и пробелами: +375 (25) 799
                    let var1 = format!("{} ({}) {}", &digits[0..3], &digits[3..5], &digits[5..8]);
                    dorks.push(format!("\"{}\" OR \"{}\"", value, var1));
                }
                // Поиск по доскам объявлений (Авито, Куфар, OLX)
                dorks.push(format!("\"{}\" site:avito.ru OR site:kufar.by OR site:olx.ua", value));
                // Поиск по базам мошенников или GetContact логам
                dorks.push(format!("\"{}\" мошенник OR spam OR \"кто звонил\"", value));
            }
            EntityType::Nickname => {
                // Ищем брошенные профили на форумах
                dorks.push(format!("intitle:\"Профиль {} \" OR inurl:\"user/{}\"", value, value));
                // Ищем посты на Reddit / Pikabu
                dorks.push(format!("site:reddit.com OR site:pikabu.ru \"{}\"", value));
            }
            _ => {
                dorks.push(format!("\"{}\"", value));
            }
        }
    }

    fn save_to_file(dorks: &[String], output_path: &str) {
        let content = dorks.join("\n");
        if let Err(e) = fs::write(output_path, content) {
            eprintln!("[!] Ошибка сохранения дорков: {}", e);
        }
    }
}

// =====================================================================
// ТРЕХУРОВНЕВОЕ ТЕСТИРОВАНИЕ (Three-Level Testing Rule)
// =====================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{EntityNode, EntityType};
    use std::collections::HashMap;
    use std::path::Path;

    // 1. BACKEND TEST: Проверка логики генерации вариаций
    #[test]
    fn backend_test_phone_variations() {
        let mut dorks = Vec::new();
        DorkGenerator::append_dorks_for_entity("+375257997676", &EntityType::Phone, &mut dorks);
        // Должен сгенерировать вариацию с пробелами
        assert!(dorks[0].contains("375 (25) 799"));
    }

    // 2. FRONTEND TEST: Проверка исключения мусорных узлов
    #[test]
    fn frontend_test_exclude_system_nodes() {
        let mut profile = IdentityProfile {
            root_entity: EntityNode { value: "test@test.com".to_string(), entity_type: EntityType::Email, first_seen: 0 },
            associated_nodes: HashMap::new(),
            active_links: Vec::new(),
            calculated_confidence: 100,
        };
        // Добавляем системный узел, который не должен гуглиться
        profile.associated_nodes.insert(
            "registered:whatsapp".to_string(),
            EntityNode { value: "registered:whatsapp".to_string(), entity_type: EntityType::Nickname, first_seen: 0 }
        );

        let dorks = DorkGenerator::generate_dorks(&profile, "test_dorks.txt");
        let has_system_dork = dorks.iter().any(|d| d.contains("registered:whatsapp"));
        assert!(!has_system_dork, "Генератор не должен делать дорки на системные теги");
        let _ = fs::remove_file("test_dorks.txt");
    }

    // 3. E2E TEST: Сквозная генерация файла
    #[test]
    fn e2e_test_dork_file_generation() {
        let profile = IdentityProfile {
            root_entity: EntityNode { value: "target_nick".to_string(), entity_type: EntityType::Nickname, first_seen: 0 },
            associated_nodes: HashMap::new(),
            active_links: Vec::new(),
            calculated_confidence: 100,
        };

        let path = "e2e_test_dorks.txt";
        let dorks = DorkGenerator::generate_dorks(&profile, path);

        assert!(Path::new(path).exists());
        assert!(!dorks.is_empty());

        // Уборка за собой
        let _ = fs::remove_file(path);
    }
}
