mod models;
mod parser;
mod scoring;
mod engine;
mod visualizer;
mod dork_generator;
mod social_spider;
mod ai_core;
mod enumerator;
mod data_broker;
mod sandbox_runner;
mod connectors;
mod case_store;

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

#[derive(Default)]
struct InputSelectors {
    phone: Option<String>,
    email: Option<String>,
    nickname: Option<String>,
    full_name: Option<String>,
    dob: Option<String>,
    country: Option<String>,
}

fn ask_optional(prompt: &str) -> Option<String> {
    print!("{}", prompt);
    io::stdout().flush().ok()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok()?;
    let value = input.trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

fn collect_selectors() -> InputSelectors {
    println!("\n[?] Введите максимум исходных данных (можно пропускать поля):");
    InputSelectors {
        phone: ask_optional("  Телефон: "),
        email: ask_optional("  Email: "),
        nickname: ask_optional("  Никнейм: "),
        full_name: ask_optional("  ФИО: "),
        dob: ask_optional("  Дата рождения (ДД.ММ.ГГГГ): "),
        country: ask_optional("  Страна: "),
    }
}

fn ask_case_id() -> Option<String> {
    print!("[?] Открыть ранее сохраненный кейс? Введите case_id (или Enter чтобы пропустить): ");
    io::stdout().flush().ok()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok()?;
    let value = input.trim().to_string();
    if value.is_empty() { None } else { Some(value) }
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
    let history = case_store::recent_cases(5);
    if !history.is_empty() {
        println!("[*] Последние кейсы:");
        for line in history {
            println!("  - {}", line);
        }
        println!("--------------------------------------------------");
    }

    if let Some(case_id) = ask_case_id() {
        match case_store::read_case_snapshot(&case_id) {
            Some(snapshot) => {
                println!("[*] Найден кейс {}", case_id);
                println!("{}", snapshot);
                println!("--------------------------------------------------");
            }
            None => println!("[!] Кейс {} не найден.", case_id),
        }
    }
    println!("     📊 X-GEN OSINT PLATFORM v3.0 [NEURO]         ");
    println!("==================================================");

    // 1. СБОР ИСХОДНЫХ СЕЛЕКТОРОВ
    let selectors = collect_selectors();

    let mut seeds: Vec<models::EntityNode> = Vec::new();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    if let Some(v) = selectors.phone.clone() { seeds.push(models::EntityNode { value: v, entity_type: models::EntityType::Phone, first_seen: now }); }
    if let Some(v) = selectors.email.clone() { seeds.push(models::EntityNode { value: v, entity_type: models::EntityType::Email, first_seen: now }); }
    if let Some(v) = selectors.nickname.clone() { seeds.push(models::EntityNode { value: v, entity_type: models::EntityType::Nickname, first_seen: now }); }
    if let Some(v) = selectors.full_name.clone() { seeds.push(models::EntityNode { value: v, entity_type: models::EntityType::FullName, first_seen: now }); }
    if let Some(v) = selectors.dob.clone() { seeds.push(models::EntityNode { value: v, entity_type: models::EntityType::DateOfBirth, first_seen: now }); }
    if let Some(v) = selectors.country.clone() { seeds.push(models::EntityNode { value: v, entity_type: models::EntityType::Country, first_seen: now }); }

    let mut registry = connectors::ConnectorRegistry::new();
    let mut connector_seeds = Vec::new();
    let observations = registry.collect_seed_observations(&seeds, now);
    for obs in observations {
        connector_seeds.push(obs.to_entity_node());
    }
    seeds.extend(connector_seeds);

    if seeds.is_empty() {
        println!("[!] Ошибка: не введено ни одного валидного селектора. Завершение работы.");
        return;
    }

    println!("--------------------------------------------------");
    println!("[+] X-GEN Absolute OSINT Protocol активирован. Нейро-ядро разблокировано.");
    println!("[*] Запуск сквозного каскадного поиска для {} стартовых селекторов\n", seeds.len());

    let start_target = seeds.remove(0);

    let mut engine_instance = engine::AnalysisEngine::new(start_target, "dumps");
    for seed in seeds {
        engine_instance.task_queue.push_back(seed);
    }

    // 2. Первичный прогон
    engine_instance.resolve_cascade().await;
    scoring::evaluate_profile(&mut engine_instance.final_profile);

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
    let _ = crate::dork_generator::DorkGenerator::generate_dorks(&engine_instance.final_profile, "dorks.txt");
    crate::visualizer::generate_html_report(&engine_instance.final_profile, "report.html");
    let resolution_report = scoring::build_resolution_report(&engine_instance.final_profile);
    if let Ok(report_json) = serde_json::to_string_pretty(&resolution_report) {
        let _ = std::fs::write("resolution_report.json", report_json);
    }

    if let Some(case_id) = case_store::persist_case_snapshot(&engine_instance.final_profile) {
        println!("[*] Case snapshot сохранен: {}", case_id);
    }

    let next_steps = scoring::suggest_next_steps(&engine_instance.final_profile);
    println!("\n[*] Рекомендованные следующие шаги:");
    for (idx, step) in next_steps.iter().enumerate() {
        println!("  {}. {}", idx + 1, step);
    }

    let source_health = scoring::source_health_summary(&engine_instance.final_profile);
    println!("\n[*] Надежность источников (top-5):");
    for src in source_health.iter().take(5) {
        println!(
            "  - {} | links={} | avg_weight={:.1} | reliability={}",
            src.source_id, src.links, src.avg_weight, src.reliability
        );
    }

    println!("\n[?] Найдено связей: {} | confidence: {}", engine_instance.final_profile.active_links.len(), engine_instance.final_profile.calculated_confidence);
    print!("[?] Продолжить поиск по найденным корреляциям? (yes/no): ");
    io::stdout().flush().unwrap();
    let mut decision = String::new();
    io::stdin().read_line(&mut decision).ok();
    if decision.trim().eq_ignore_ascii_case("yes") {
        engine_instance.resolve_cascade().await;
        scoring::evaluate_profile(&mut engine_instance.final_profile);
        crate::visualizer::generate_html_report(&engine_instance.final_profile, "report.html");
    }

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
