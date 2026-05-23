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
mod evidence;
mod source_registry;
mod intake;
mod sanitize;
mod hashing;
mod conflicts;
mod analysis_report;
mod telegram_export;
mod discovery;
mod public_search;
mod autopilot;
mod checkers;
mod confidence;
mod noise_rules;
mod runtime_profile;
mod master_report;

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
    telegram_export_path: Option<String>,
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
        telegram_export_path: ask_optional("  Путь к Telegram export/result.json (Enter = пропустить): "),
    }
}

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

fn print_conflict_report(report: &conflicts::ConflictReport) {
    println!("\n[*] Conflict Engine:");
    println!(
        "  findings={} | severity_score={} | high_risk={}",
        report.findings.len(),
        report.severity_score(),
        report.has_high_risk()
    );

    if report.findings.is_empty() {
        println!("  - Конфликты не обнаружены.");
        return;
    }

    for finding in report.findings.iter().take(8) {
        println!(
            "  - [{:?}/{:?}] {} | sources={:?}",
            finding.severity,
            finding.kind,
            finding.entity_value,
            finding.source_ids
        );
        println!("    {}", finding.message);
        println!("    next: {}", finding.recommended_action);
    }

    if report.findings.len() > 8 {
        println!("  ... и еще {} конфликтов", report.findings.len() - 8);
    }
}

fn save_conflict_report(report: &conflicts::ConflictReport, path: &str) {
    match serde_json::to_string_pretty(report) {
        Ok(json) => {
            if let Err(err) = std::fs::write(path, json) {
                eprintln!("[!] Не удалось сохранить {}: {}", path, err);
            }
        }
        Err(err) => eprintln!("[!] Не удалось сериализовать conflict report: {}", err),
    }
}

fn apply_and_save_confidence_guardrails(profile: &mut models::IdentityProfile) {
    let before = profile.calculated_confidence;
    let report = confidence::apply_confidence_guardrails(profile);
    println!(
        "\n[*] Confidence Guardrails: original={} adjusted={} capped={}",
        before,
        report.adjusted_score,
        report.was_capped()
    );
    if report.was_capped() {
        for rule in &report.applied_guardrails {
            println!("  - {:?} cap={} | {}", rule.kind, rule.cap, rule.reason);
        }
    }
    if let Err(err) = confidence::save_confidence_report(&report, "confidence_report.json") {
        eprintln!("[!] Не удалось сохранить confidence_report.json: {}", err);
    }
}

fn save_full_analysis_report(
    profile: &models::IdentityProfile,
    resolution_report: models::ResolutionReport,
    conflict_report: conflicts::ConflictReport,
    source_health: Vec<scoring::SourceHealth>,
    next_steps: Vec<String>,
) {
    let report = analysis_report::build_analysis_report(
        profile,
        resolution_report,
        conflict_report,
        source_health,
        next_steps,
    );

    if let Err(err) = analysis_report::save_analysis_report(&report, "analysis_report.json") {
        eprintln!("[!] Не удалось сохранить analysis_report.json: {}", err);
    }
}

fn build_and_save_master_report() {
    match master_report::build_and_save_master_report("master_report.json") {
        Ok(report) => {
            let compact = master_report::compact_master_summary(&report);
            println!("\n[*] Master Report: master_report.json");
            println!("  status: {}", compact.get("status").and_then(|v| v.as_str()).unwrap_or("unknown"));
            println!("  confidence_adjusted: {}", compact.get("confidence_adjusted").map(|v| v.to_string()).unwrap_or_else(|| "null".to_string()));
            println!("  high_risk: {}", compact.get("high_risk").and_then(|v| v.as_bool()).unwrap_or(false));
            println!("  missing_reports: {}", report.missing_reports.len());
        }
        Err(err) => eprintln!("[!] Не удалось сохранить master_report.json: {}", err),
    }
}

fn add_telegram_export_seeds(seeds: &mut Vec<models::EntityNode>, path: &str) {
    println!("\n[*] Telegram Export Parser: {}", path);
    match telegram_export::analyze_telegram_export(path) {
        Ok(report) => {
            println!(
                "  chats={} | messages={} | emails={} | phones={} | usernames={} | urls={}",
                report.chats_analyzed,
                report.messages_analyzed,
                report.extracted_counts.emails,
                report.extracted_counts.phones,
                report.extracted_counts.usernames,
                report.extracted_counts.urls
            );
            if let Err(err) = telegram_export::save_telegram_export_report(&report, "telegram_export_report.json") {
                eprintln!("  [!] Не удалось сохранить telegram_export_report.json: {}", err);
            }

            let nodes = telegram_export::observations_as_entity_nodes(&report, 100);
            println!("  [+] Добавлено Telegram-derived селекторов: {}", nodes.len());
            seeds.extend(nodes);
        }
        Err(err) => eprintln!("  [!] Telegram export не обработан: {}", err),
    }
}

async fn add_email_domain_checker_seeds(seeds: &mut Vec<models::EntityNode>) {
    println!("\n[*] Email/Domain Checker MVP");
    let report = checkers::run_email_domain_checkers(seeds).await;
    println!(
        "  emails={} valid={} invalid={} domains={} username_candidates={} dns_errors={} findings={}",
        report.stats.emails_checked,
        report.stats.valid_emails,
        report.stats.invalid_emails,
        report.stats.domains_checked,
        report.stats.username_candidates,
        report.stats.dns_errors,
        report.stats.findings_count
    );

    if let Err(err) = checkers::save_email_domain_report(&report, "email_domain_report.json") {
        eprintln!("  [!] Не удалось сохранить email_domain_report.json: {}", err);
    }

    let nodes = checkers::observations_as_entity_nodes(&report, 100);
    println!("  [+] Добавлено email/domain селекторов: {}", nodes.len());
    seeds.extend(nodes);
}

async fn run_autopilot_seeds(seeds: &mut Vec<models::EntityNode>) {
    println!("\n[*] Autonomous OSINT Autopilot");
    let report = autopilot::run_autonomous_osint(seeds).await;
    println!(
        "  cycles={} | initial_seeds={} | final_seeds={} | new_nodes={}",
        report.cycles.len(),
        report.initial_seed_count,
        report.final_seed_count,
        report.total_new_nodes
    );

    for cycle in &report.cycles {
        println!(
            "  - cycle {}: input={} discovery_new={} search_new={} total={}",
            cycle.cycle,
            cycle.input_seed_count,
            cycle.new_discovery_nodes,
            cycle.new_public_search_nodes,
            cycle.total_seed_count_after_cycle
        );
    }

    if let Err(err) = autopilot::save_autopilot_report(&report, "autopilot_report.json") {
        eprintln!("  [!] Не удалось сохранить autopilot_report.json: {}", err);
    }

    if let Some(last_cycle) = report.cycles.last() {
        let _ = discovery::save_discovery_report(&last_cycle.discovery_report, "discovery_report.json");
        let _ = public_search::save_public_search_report(&last_cycle.public_search_report, "public_search_report.json");
    }
}

#[tokio::main]
async fn main() {
    println!("==================================================");
    println!("     📊 X-GEN OSINT PLATFORM v3.0 [NEURO]         ");
    println!("==================================================");

    let profile = runtime_profile::init_runtime_profile();
    println!(
        "[*] Run profile: {} | discovery_tasks={} | public_search_tasks={} | cycles={} | dns={} | github={} | fetch={}",
        profile.label,
        profile.discovery_max_tasks,
        profile.public_search_max_tasks,
        profile.autopilot_cycles,
        profile.dns_check,
        profile.github_search,
        profile.discovery_fetch
    );
    if let Err(err) = runtime_profile::save_run_profile_report("run_profile_report.json") {
        eprintln!("[!] Не удалось сохранить run_profile_report.json: {}", err);
    }

    let selectors = collect_selectors();

    let mut seeds: Vec<models::EntityNode> = Vec::new();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    if let Some(v) = selectors.phone.clone() { seeds.push(models::EntityNode { value: v, entity_type: models::EntityType::Phone, first_seen: now }); }
    if let Some(v) = selectors.email.clone() { seeds.push(models::EntityNode { value: v, entity_type: models::EntityType::Email, first_seen: now }); }
    if let Some(v) = selectors.nickname.clone() { seeds.push(models::EntityNode { value: v, entity_type: models::EntityType::Nickname, first_seen: now }); }
    if let Some(v) = selectors.full_name.clone() { seeds.push(models::EntityNode { value: v, entity_type: models::EntityType::FullName, first_seen: now }); }
    if let Some(v) = selectors.dob.clone() { seeds.push(models::EntityNode { value: v, entity_type: models::EntityType::DateOfBirth, first_seen: now }); }
    if let Some(v) = selectors.country.clone() { seeds.push(models::EntityNode { value: v, entity_type: models::EntityType::Country, first_seen: now }); }
    if let Some(path) = selectors.telegram_export_path.as_deref() {
        add_telegram_export_seeds(&mut seeds, path);
    }

    add_email_domain_checker_seeds(&mut seeds).await;
    run_autopilot_seeds(&mut seeds).await;

    let mut registry = connectors::ConnectorRegistry::new();
    let mut connector_seeds = Vec::new();
    let observations = registry.collect_seed_observations(&seeds, now);
    for obs in observations {
        connector_seeds.push(obs.to_entity_node());
    }
    seeds.extend(connector_seeds);

    if seeds.is_empty() {
        println!("[!] Ошибка: не введено ни одного валидного селектора. Завершение работы.");
        build_and_save_master_report();
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

    engine_instance.resolve_cascade().await;
    scoring::evaluate_profile(&mut engine_instance.final_profile);
    apply_and_save_confidence_guardrails(&mut engine_instance.final_profile);
    let conflict_report = conflicts::ConflictEngine::analyze(&engine_instance.final_profile);
    print_conflict_report(&conflict_report);
    save_conflict_report(&conflict_report, "conflict_report.json");

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

    save_full_analysis_report(
        &engine_instance.final_profile,
        resolution_report,
        conflict_report,
        source_health,
        next_steps,
    );

    build_and_save_master_report();

    println!("\n[?] Найдено связей: {} | confidence: {}", engine_instance.final_profile.active_links.len(), engine_instance.final_profile.calculated_confidence);
    print!("[?] Продолжить поиск по найденным корреляциям? (yes/no): ");
    io::stdout().flush().unwrap();
    let mut decision = String::new();
    io::stdin().read_line(&mut decision).ok();
    if decision.trim().eq_ignore_ascii_case("yes") {
        engine_instance.resolve_cascade().await;
        scoring::evaluate_profile(&mut engine_instance.final_profile);
        apply_and_save_confidence_guardrails(&mut engine_instance.final_profile);
        let conflict_report = conflicts::ConflictEngine::analyze(&engine_instance.final_profile);
        print_conflict_report(&conflict_report);
        save_conflict_report(&conflict_report, "conflict_report.json");
        crate::visualizer::generate_html_report(&engine_instance.final_profile, "report.html");

        let resolution_report = scoring::build_resolution_report(&engine_instance.final_profile);
        let next_steps = scoring::suggest_next_steps(&engine_instance.final_profile);
        let source_health = scoring::source_health_summary(&engine_instance.final_profile);
        save_full_analysis_report(
            &engine_instance.final_profile,
            resolution_report,
            conflict_report,
            source_health,
            next_steps,
        );
        build_and_save_master_report();
    }

    let shared_state = Arc::new(AppState {
        engine: Mutex::new(engine_instance)
    });

    let app = Router::new()
        .route("/expand", post(expand_handler))
        .route("/", get(report_handler))
        .with_state(shared_state);

    println!("\n==================================================");
    println!("[+] Платформа успешно переведена в режим сервера!");
    println!("[*] Открой браузер: http://127.0.0.1:3000");
    println!("==================================================");

    let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn report_handler() -> impl IntoResponse {
    let html = std::fs::read_to_string("report.html").unwrap_or_else(|_| {
        "<html><body><h1>report.html не найден</h1><p>Сначала запусти анализ.</p></body></html>".to_string()
    });
    axum::response::Html(html)
}

#[axum::debug_handler]
async fn expand_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ExpandRequest>
) -> impl IntoResponse {
    let mut machine = state.engine.lock().await;

    println!("[Web] Запуск дополнительного поиска для узла: {}", payload.target);

    let _node = models::EntityNode::new(&payload.target, models::EntityType::Nickname);

    machine.resolve_cascade().await;

    crate::visualizer::generate_html_report(&machine.final_profile, "report.html");
    build_and_save_master_report();

    Json(serde_json::json!({
        "status": "success",
        "message": "Граф обновлен. Обновите страницу в браузере!"
    }))
}
