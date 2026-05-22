mod models;
mod parser;
mod scoring;
mod engine;
mod visualizer;
mod dork_generator;
mod social_spider;   // <-- Активируем Охотника за профилями

use models::{EntityNode, EntityType, IdentityProfile};
use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Serialize, Deserialize};

/// STIX-подобный объект Indicator (для экспорта)
#[derive(Serialize, Deserialize)]
struct StixIndicator {
    id: String,
    spec_version: String,
    created: String,
    modified: String,
    name: String,
    pattern: String,
    pattern_type: String,
    labels: Vec<String>,
}

/// STIX-подобный объект Identity
#[derive(Serialize, Deserialize)]
struct StixIdentity {
    id: String,
    spec_version: String,
    created: String,
    modified: String,
    name: String,
    identity_class: String,
    description: String,
}

/// STIX-подобный объект Relationship
#[derive(Serialize, Deserialize)]
struct StixRelationship {
    id: String,
    spec_version: String,
    created: String,
    modified: String,
    relationship_type: String,
    source_ref: String,
    target_ref: String,
}

/// Преобразует профиль в набор STIX‑объектов
fn profile_to_stix(profile: &IdentityProfile) -> (Vec<StixIndicator>, Vec<StixIdentity>, Vec<StixRelationship>) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let timestamp = format!("{}", now);
    let mut indicators = Vec::new();
    let mut identities = Vec::new();
    let mut relationships = Vec::new();

    let root_id = format!("identity--{}", &profile.root_entity.value);
    identities.push(StixIdentity {
        id: root_id.clone(),
        spec_version: "2.1".to_string(),
        created: timestamp.clone(),
        modified: timestamp.clone(),
        name: profile.root_entity.value.clone(),
        identity_class: "individual".to_string(),
        description: format!("Root entity of type {:?}", profile.root_entity.entity_type),
    });

    for (idx, (value, node)) in profile.associated_nodes.iter().enumerate() {
        let node_id = format!("identity--{}", value);
        identities.push(StixIdentity {
            id: node_id.clone(),
            spec_version: "2.1".to_string(),
            created: timestamp.clone(),
            modified: timestamp.clone(),
            name: value.clone(),
            identity_class: "individual".to_string(),
            description: format!("Associated entity of type {:?}", node.entity_type),
        });

        let indicator_id = format!("indicator--{}", idx);
        let pattern = match node.entity_type {
            models::EntityType::Email => format!("[email:value = '{}']", value),
            models::EntityType::Phone => format!("[phone:value = '{}']", value),
            _ => format!("[user:value = '{}']", value),
        };
        indicators.push(StixIndicator {
            id: indicator_id.clone(),
            spec_version: "2.1".to_string(),
            created: timestamp.clone(),
            modified: timestamp.clone(),
            name: format!("Indicator for {}", value),
            pattern,
            pattern_type: "stix".to_string(),
            labels: vec![format!("{:?}", node.entity_type)],
        });

        relationships.push(StixRelationship {
            id: format!("relationship--{}", idx),
            spec_version: "2.1".to_string(),
            created: timestamp.clone(),
            modified: timestamp.clone(),
            relationship_type: "related-to".to_string(),
            source_ref: root_id.clone(),
            target_ref: node_id,
        });
    }

    (indicators, identities, relationships)
}

#[tokio::main]
async fn main() {
    println!("==================================================");
    println!("     📊 X-GEN OSINT ENGINE CORE v2.0 [ABSOLUTE]   ");
    println!("==================================================");

    print!("\n[?] Введите цель (никнейм, телефон или email): ");
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).expect("Не удалось прочитать строку");
    let value = input.trim().to_string();

    if value.is_empty() {
        println!("[!] Ошибка: Пустой ввод. Завершение работы.");
        return;
    }

    let entity_type = if value.starts_with('+') || (value.chars().all(|c| c.is_numeric()) && value.len() >= 10) {
        EntityType::Phone
    } else if value.contains('@') && value.contains('.') {
        EntityType::Email
    } else {
        EntityType::Nickname
    };

    println!("[*] Автоматически определен тип селектора: {:?}", entity_type);
    println!("--------------------------------------------------");
    println!("[+] X-GEN Absolute OSINT Protocol активирован. Все модули разблокированы.");
    println!("[*] Запуск сквозного каскадного поиска для: {}\n", value);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let start_target = EntityNode {
        value,
        entity_type,
        first_seen: now,
    };

    let mut osint_machine = engine::AnalysisEngine::new(start_target, "dumps");
    osint_machine.resolve_cascade().await;

    // Финальный вывод
    println!("\n==================================================");
    println!("              ИТОГОВОЕ АНАЛИТИЧЕСКОЕ ДОСЬЕ        ");
    println!("==================================================");
    println!(" Целевой объект: {}", osint_machine.final_profile.root_entity.value);
    println!(" Уровень достоверности графа: {}%", osint_machine.final_profile.calculated_confidence);
    println!(" Найдено уникальных связей: {}", osint_machine.final_profile.associated_nodes.len());
    println!("--------------------------------------------------");

    if !osint_machine.final_profile.associated_nodes.is_empty() {
        println!("Верифицированные связанные данные:");
        for (val, node) in &osint_machine.final_profile.associated_nodes {
            println!("  └── [{:?}] {}", node.entity_type, val);
        }
    } else {
        println!("[-] Дополнительных связей в локальных индексах не обнаружено.");
    }

    // ЭКСПОРТ В STIX
    let (indicators, identities, relationships) = profile_to_stix(&osint_machine.final_profile);
    let stix_bundle = serde_json::json!({
        "type": "bundle",
        "objects": {
            "indicators": indicators,
            "identities": identities,
            "relationships": relationships
        }
    });
    if let Ok(json_str) = serde_json::to_string_pretty(&stix_bundle) {
        if let Err(e) = std::fs::write("stix_report.json", &json_str) {
            eprintln!("[!] Ошибка записи STIX-отчёта: {}", e);
        } else {
            println!("✅ STIX-отчёт сохранён в stix_report.json");
        }
    }

    // Генерация HTML-графа
    crate::visualizer::generate_html_report(
        &osint_machine.final_profile,
        "report.html"
    );
    println!("✅ HTML-отчёт сохранён в report.html");

    // Генерация дорков
    let dorks = crate::dork_generator::generate_dorks(
        &osint_machine.final_profile,
        "dorks.txt"
    );
    println!("✅ Дорки сохранены в dorks.txt ({} запросов)", dorks.len());

    println!("==================================================");
}