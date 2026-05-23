mod models;
mod parser;
mod scoring;
mod engine;
mod visualizer;
mod dork_generator;
mod social_spider;
mod ai_core;
mod enumerator;

use axum::{
    routing::{post, get},
    extract::State,
    Json,
    Router,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::net::TcpListener;
use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

// Состояние приложения: движок под защитой асинхронного Mutex
struct AppState {
    engine: Mutex<engine::AnalysisEngine>,
}

#[derive(Deserialize)]
struct ExpandRequest {
    target: String
}

// --- STIX-структуры ---
#[derive(Serialize, Deserialize)]
struct StixIndicator {
    id: String, spec_version: String, created: String, modified: String,
    name: String, pattern: String, pattern_type: String, labels: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct StixIdentity {
    id: String, spec_version: String, created: String, modified: String,
    name: String, identity_class: String, description: String,
}

#[derive(Serialize, Deserialize)]
struct StixRelationship {
    id: String, spec_version: String, created: String, modified: String,
    relationship_type: String, source_ref: String, target_ref: String,
}

fn profile_to_stix(profile: &models::IdentityProfile) -> (Vec<StixIndicator>, Vec<StixIdentity>, Vec<StixRelationship>) {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let timestamp = format!("{}", now);
    let mut indicators = Vec::new();
    let mut identities = Vec::new();
    let mut relationships = Vec::new();

    let root_id = format!("identity--{}", &profile.root_entity.value);
    identities.push(StixIdentity {
        id: root_id.clone(), spec_version: "2.1".to_string(), created: timestamp.clone(), modified: timestamp.clone(),
        name: profile.root_entity.value.clone(), identity_class: "individual".to_string(), description: format!("Root entity {:?}", profile.root_entity.entity_type),
    });

    for (idx, (value, node)) in profile.associated_nodes.iter().enumerate() {
        let node_id = format!("identity--{}", value);
        identities.push(StixIdentity {
            id: node_id.clone(), spec_version: "2.1".to_string(), created: timestamp.clone(), modified: timestamp.clone(),
            name: value.clone(), identity_class: "individual".to_string(), description: format!("Associated entity {:?}", node.entity_type),
        });

        indicators.push(StixIndicator {
            id: format!("indicator--{}", idx), spec_version: "2.1".to_string(), created: timestamp.clone(), modified: timestamp.clone(),
            name: format!("Indicator for {}", value), pattern: format!("[entity:value = '{}']", value), pattern_type: "stix".to_string(), labels: vec![format!("{:?}", node.entity_type)],
        });

        relationships.push(StixRelationship {
            id: format!("relationship--{}", idx), spec_version: "2.1".to_string(), created: timestamp.clone(), modified: timestamp.clone(),
            relationship_type: "related-to".to_string(), source_ref: root_id.clone(), target_ref: node_id,
        });
    }
    (indicators, identities, relationships)
}

#[tokio::main]
async fn main() {
    println!("==================================================");
    println!("     📊 X-GEN OSINT PLATFORM v3.0 [NEURO]         ");
    println!("==================================================");

    // 1. ВОЗВРАЩАЕМ РУЧНОЙ ВВОД ЦЕЛИ
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
        models::EntityType::Phone
    } else if value.contains('@') && value.contains('.') {
        models::EntityType::Email
    } else {
        models::EntityType::Nickname
    };

    println!("[*] Автоматически определен тип селектора: {:?}", entity_type);
    println!("--------------------------------------------------");
    println!("[+] X-GEN Absolute OSINT Protocol активирован. Нейро-ядро разблокировано.");
    println!("[*] Запуск сквозного каскадного поиска для: {}\n", value);

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let start_target = models::EntityNode { value: value.clone(), entity_type, first_seen: now };

    let mut engine_instance = engine::AnalysisEngine::new(start_target, "dumps");

    // 2. Первичный прогон
    engine_instance.resolve_cascade().await;

    // 3. ВОЗВРАЩАЕМ ИИ-АНАЛИТИКА MISTRAL
    println!("\n[*] Запуск ИИ-аналитика (Mistral:7b) для составления сводки...");
    let mut profile_text = format!("Target: {}\n", engine_instance.final_profile.root_entity.value);
    for (val, node) in &engine_instance.final_profile.associated_nodes {
        profile_text.push_str(&format!("[{:?}] {}\n", node.entity_type, val));
    }

    let summary = engine_instance.ai_core.investigator_summarize(&profile_text).await;
    match summary {
        Some(text) => {
            println!("--- AI Executive Summary ---");
            println!("{}", text);
            let _ = std::fs::write("ai_summary.txt", &text);
        }
        None => println!("  [AI] Не удалось получить аналитическую сводку."),
    }

    // Сохраняем STIX, Дорки и HTML перед запуском сервера
    let (indicators, identities, relationships) = profile_to_stix(&engine_instance.final_profile);
    let stix_bundle = serde_json::json!({ "type": "bundle", "objects": { "indicators": indicators, "identities": identities, "relationships": relationships }});
    if let Ok(json_str) = serde_json::to_string_pretty(&stix_bundle) {
        let _ = std::fs::write("stix_report.json", &json_str);
    }
    let _ = crate::dork_generator::generate_dorks(&engine_instance.final_profile, "dorks.txt");
    crate::visualizer::generate_html_report(&engine_instance.final_profile, "report.html");

    // 4. ЗАПУСК WEB-СЕРВЕРА
    let shared_state = Arc::new(AppState {
        engine: Mutex::new(engine_instance)
    });

    let app = Router::new()
        .route("/expand", post(expand_handler))
        .route("/", get(|| async {
            axum::response::Html(include_str!("../report.html"))
        }))
        .with_state(shared_state);

    println!("\n==================================================");
    println!("[+] Платформа успешно переведена в режим сервера!");
    println!("[*] Открой браузер: http://127.0.0.1:3000");
    println!("==================================================");

    let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

#[axum::debug_handler]
async fn expand_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ExpandRequest>
) -> impl IntoResponse {
    let mut machine = state.engine.lock().await;

    println!("[Web] Запуск дополнительного поиска для узла: {}", payload.target);

    let _node = models::EntityNode::new(&payload.target, models::EntityType::Nickname);

    // Запускаем каскад для узла
    machine.resolve_cascade().await;

    // Обновляем визуализацию
    crate::visualizer::generate_html_report(&machine.final_profile, "report.html");

    Json(serde_json::json!({
        "status": "success",
        "message": "Граф обновлен. Обновите страницу в браузере!"
    }))
}