use crate::evidence::{build_evidence_observation, EvidenceInput};
use crate::models::{EntityNode, EntityType, EvidenceRecord, ObservationRecord, SensitivityClass, SourceClass};
use crate::noise_rules::{adjusted_confidence, evaluate_noise, NoiseAction, NoiseDecisionInput};
use crate::runtime_profile;
use crate::sanitize::{sanitize_text, SanitizeOptions};
use reqwest::{Client, redirect::Policy};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscoveryTaskKind {
    ProfileProbe,
    PageProbe,
    SearchQuery,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryTask {
    pub task_id: String,
    pub kind: DiscoveryTaskKind,
    pub seed_type: EntityType,
    pub seed_value: String,
    pub url: Option<String>,
    pub query: Option<String>,
    pub source_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryFinding {
    pub task_id: String,
    pub source_id: String,
    pub url: Option<String>,
    pub entity_type: EntityType,
    pub value: String,
    pub confidence: u8,
    pub note: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiscoveryStats {
    pub tasks_planned: usize,
    pub tasks_fetched: usize,
    pub fetch_errors: usize,
    pub findings_count: usize,
    pub blocked_by_noise_rules: usize,
    pub downranked_by_noise_rules: usize,
    pub evidences_count: usize,
    pub observations_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiscoveryReport {
    pub generated_at: u64,
    pub stats: DiscoveryStats,
    pub tasks: Vec<DiscoveryTask>,
    pub findings: Vec<DiscoveryFinding>,
    pub evidences: Vec<EvidenceRecord>,
    pub observations: Vec<ObservationRecord>,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub async fn run_public_discovery_for_seeds(seeds: &[EntityNode]) -> DiscoveryReport {
    let mut tasks = Vec::new();
    let mut seen_tasks = HashSet::new();
    for seed in seeds {
        for task in build_tasks_from_seed(seed) {
            let key = format!("{:?}:{:?}:{:?}", task.kind, task.url, task.query);
            if seen_tasks.insert(key) {
                tasks.push(task);
            }
        }
    }

    let max_tasks = runtime_profile::discovery_max_tasks();
    let fetch_enabled = runtime_profile::discovery_fetch();

    let client = build_discovery_client();
    let mut report = DiscoveryReport {
        generated_at: now_unix(),
        stats: DiscoveryStats { tasks_planned: tasks.len(), ..DiscoveryStats::default() },
        tasks: tasks.clone(),
        findings: Vec::new(),
        evidences: Vec::new(),
        observations: Vec::new(),
    };

    if !fetch_enabled || max_tasks == 0 {
        return report;
    }

    let mut seen_observations: HashSet<(EntityType, String)> = HashSet::new();
    for task in tasks.into_iter().take(max_tasks) {
        if !matches!(task.kind, DiscoveryTaskKind::ProfileProbe | DiscoveryTaskKind::PageProbe) {
            continue;
        }
        let Some(url) = task.url.clone() else {
            continue;
        };
        if !is_safe_public_url(&url) {
            continue;
        }

        match fetch_public_page(&client, &url).await {
            Ok(body) => {
                report.stats.tasks_fetched += 1;
                if looks_like_not_found(&body) {
                    continue;
                }

                if task.kind == DiscoveryTaskKind::ProfileProbe && !profile_page_matches_seed(&body, &task.seed_value) {
                    continue;
                }

                let findings = extract_findings_from_text(&body, &task, &url);
                for finding in findings {
                    let Some(finding) = apply_noise_rules_to_finding(finding, &mut report.stats) else {
                        continue;
                    };
                    let normalized = normalize_item_value(&finding.value, &finding.entity_type);
                    if normalized.is_empty() || !seen_observations.insert((finding.entity_type.clone(), normalized)) {
                        continue;
                    }

                    let sensitivity = sensitivity_for(&finding.entity_type);
                    let evidence_context = format!(
                        "discovery source={} url={} note={} snippet={}",
                        finding.source_id,
                        url,
                        finding.note,
                        sanitize_text(&body, &SanitizeOptions { max_chars: 600, ..SanitizeOptions::default() }).value
                    );
                    let pair = build_evidence_observation(EvidenceInput {
                        source_id: finding.source_id.clone(),
                        source_class: SourceClass::PublicOSINT,
                        entity_type: finding.entity_type.clone(),
                        raw_value: finding.value.clone(),
                        raw_context: evidence_context,
                        confidence: finding.confidence,
                        sensitivity,
                        tags: vec!["public_discovery".to_string(), format!("task:{:?}", task.kind)],
                    });
                    report.findings.push(finding);
                    report.evidences.push(pair.evidence);
                    report.observations.push(pair.observation);
                }
            }
            Err(_) => report.stats.fetch_errors += 1,
        }
    }

    report.stats.findings_count = report.findings.len();
    report.stats.evidences_count = report.evidences.len();
    report.stats.observations_count = report.observations.len();
    report
}

fn apply_noise_rules_to_finding(mut finding: DiscoveryFinding, stats: &mut DiscoveryStats) -> Option<DiscoveryFinding> {
    let decision = evaluate_noise(&NoiseDecisionInput {
        source_id: finding.source_id.clone(),
        note: finding.note.clone(),
        entity_type: finding.entity_type.clone(),
        value: finding.value.clone(),
        url: finding.url.clone(),
        confidence: finding.confidence,
    });

    match decision.action {
        NoiseAction::Block => {
            stats.blocked_by_noise_rules += 1;
            None
        }
        NoiseAction::Downrank => {
            stats.downranked_by_noise_rules += 1;
            let adjusted = adjusted_confidence(finding.confidence, &decision)?;
            finding.confidence = adjusted;
            finding.note = format!("{} | noise_downrank: {}", finding.note, decision.reason);
            Some(finding)
        }
        NoiseAction::Allow => Some(finding),
    }
}

pub fn build_tasks_from_seed(seed: &EntityNode) -> Vec<DiscoveryTask> {
    let mut tasks = Vec::new();
    match &seed.entity_type {
        EntityType::Nickname | EntityType::Username => {
            if let Some(username) = normalize_username(&seed.value) {
                tasks.extend(profile_tasks_for_username(&username, seed));
                tasks.extend(search_tasks_for_terms(&[username], seed));
            }
        }
        EntityType::Email => {
            if let Some((local, domain)) = split_email(&seed.value) {
                let candidates = username_candidates_from_email_local(&local);
                for candidate in &candidates {
                    tasks.extend(profile_tasks_for_username(candidate, seed));
                }
                tasks.extend(search_tasks_for_terms(&[seed.value.clone(), local, domain], seed));
            }
        }
        EntityType::FullName => {
            let value = seed.value.trim().to_string();
            if !value.is_empty() {
                tasks.extend(search_tasks_for_terms(&[
                    value.clone(),
                    format!("\"{}\" Telegram", value),
                    format!("\"{}\" GitHub", value),
                    format!("\"{}\" VK", value),
                ], seed));
            }
        }
        EntityType::Url => {
            let url = normalize_url(&seed.value);
            tasks.push(make_url_task(seed, DiscoveryTaskKind::PageProbe, &url, "direct_url_probe"));
        }
        EntityType::Domain => {
            let domain = seed.value.trim().trim_start_matches("http://").trim_start_matches("https://").trim_matches('/');
            if !domain.is_empty() {
                tasks.push(make_url_task(seed, DiscoveryTaskKind::PageProbe, &format!("https://{}", domain), "domain_home_probe"));
            }
        }
        EntityType::Phone | EntityType::Country | EntityType::DateOfBirth => {
            let value = seed.value.trim().to_string();
            if !value.is_empty() {
                tasks.extend(search_tasks_for_terms(&[value], seed));
            }
        }
        _ => {}
    }
    tasks
}

pub fn save_discovery_report(report: &DiscoveryReport, path: &str) -> Result<(), String> {
    let json = serde_json::to_string_pretty(report)
        .map_err(|err| format!("serialize discovery report: {}", err))?;
    std::fs::write(path, json).map_err(|err| format!("write {}: {}", path, err))
}

pub fn observations_as_entity_nodes(report: &DiscoveryReport, limit: usize) -> Vec<EntityNode> {
    let mut nodes = Vec::new();
    let mut seen = HashSet::new();
    for obs in &report.observations {
        if nodes.len() >= limit {
            break;
        }
        if !matches!(obs.entity_type, EntityType::Email | EntityType::Phone | EntityType::Username | EntityType::Url | EntityType::Domain) {
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
            nodes.push(EntityNode {
                value,
                entity_type: obs.entity_type.clone(),
                first_seen: obs.seen_at,
            });
        }
    }
    nodes
}

fn build_discovery_client() -> Client {
    Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(Policy::limited(3))
        .build()
        .expect("build discovery client")
}

async fn fetch_public_page(client: &Client, url: &str) -> Result<String, reqwest::Error> {
    let max_bytes = runtime_profile::discovery_max_bytes();
    let resp = client
        .get(url)
        .header("User-Agent", "XGEN-PublicDiscovery/1.0 (+local research)")
        .send()
        .await?;
    if !resp.status().is_success() {
        return Ok(String::new());
    }
    let text = resp.text().await?;
    Ok(text.chars().take(max_bytes).collect())
}

fn profile_tasks_for_username(username: &str, seed: &EntityNode) -> Vec<DiscoveryTask> {
    let templates = vec![
        ("telegram_public_profile", format!("https://t.me/{}", username)),
        ("github_profile", format!("https://github.com/{}", username)),
        ("gitlab_profile", format!("https://gitlab.com/{}", username)),
        ("vk_profile", format!("https://vk.com/{}", username)),
        ("youtube_handle", format!("https://www.youtube.com/@{}", username)),
        ("tiktok_profile", format!("https://www.tiktok.com/@{}", username)),
        ("instagram_profile", format!("https://www.instagram.com/{}/", username)),
        ("reddit_profile", format!("https://www.reddit.com/user/{}/", username)),
    ];

    templates
        .into_iter()
        .map(|(source_id, url)| make_url_task(seed, DiscoveryTaskKind::ProfileProbe, &url, source_id))
        .collect()
}

fn search_tasks_for_terms(terms: &[String], seed: &EntityNode) -> Vec<DiscoveryTask> {
    terms
        .iter()
        .filter(|term| !term.trim().is_empty())
        .map(|term| DiscoveryTask {
            task_id: format!("search_{}", crate::hashing::sha256_hex(&format!("{:?}:{}", seed.entity_type, term))[..16].to_string()),
            kind: DiscoveryTaskKind::SearchQuery,
            seed_type: seed.entity_type.clone(),
            seed_value: seed.value.clone(),
            url: None,
            query: Some(term.clone()),
            source_id: "planned_public_search_query".to_string(),
        })
        .collect()
}

fn make_url_task(seed: &EntityNode, kind: DiscoveryTaskKind, url: &str, source_id: &str) -> DiscoveryTask {
    DiscoveryTask {
        task_id: format!("disc_{}", &crate::hashing::sha256_hex(&format!("{:?}:{}:{}", seed.entity_type, seed.value, url))[..16]),
        kind,
        seed_type: seed.entity_type.clone(),
        seed_value: seed.value.clone(),
        url: Some(url.to_string()),
        query: None,
        source_id: source_id.to_string(),
    }
}

fn extract_findings_from_text(body: &str, task: &DiscoveryTask, url: &str) -> Vec<DiscoveryFinding> {
    let mut findings = Vec::new();
    let mut seen = HashSet::new();
    let clean = strip_html_noise(body);

    if task.kind == DiscoveryTaskKind::ProfileProbe {
        if let Some(username) = username_from_profile_url(url) {
            push_finding(&mut findings, &mut seen, task, url, EntityType::Username, username, 65, "profile_url_username".to_string());
        }
    }

    for raw in clean.split_whitespace() {
        let token = clean_token(raw);
        if token.is_empty() {
            continue;
        }
        if is_email(&token) {
            push_finding(&mut findings, &mut seen, task, url, EntityType::Email, token.to_lowercase(), 70, "email_in_public_page".to_string());
            continue;
        }
        if is_url(&token) {
            let normalized_url = normalize_url(&token);
            push_finding(&mut findings, &mut seen, task, url, EntityType::Url, normalized_url.clone(), 55, "url_in_public_page".to_string());
            if let Some(username) = username_from_url(&normalized_url) {
                push_finding(&mut findings, &mut seen, task, url, EntityType::Username, username, 55, "username_from_url".to_string());
            }
            continue;
        }
        if let Some(username) = username_from_at_token(&token) {
            push_finding(&mut findings, &mut seen, task, url, EntityType::Username, username, 50, "at_username_in_public_page".to_string());
            continue;
        }
        if let Some(phone) = phone_from_token(&token) {
            push_finding(&mut findings, &mut seen, task, url, EntityType::Phone, phone, 55, "phone_in_public_page".to_string());
        }
    }

    findings
}

fn push_finding(
    findings: &mut Vec<DiscoveryFinding>,
    seen: &mut HashSet<(EntityType, String)>,
    task: &DiscoveryTask,
    url: &str,
    entity_type: EntityType,
    value: String,
    confidence: u8,
    note: String,
) {
    let normalized = normalize_item_value(&value, &entity_type);
    if normalized.is_empty() {
        return;
    }
    if seen.insert((entity_type.clone(), normalized)) {
        findings.push(DiscoveryFinding {
            task_id: task.task_id.clone(),
            source_id: task.source_id.clone(),
            url: Some(url.to_string()),
            entity_type,
            value,
            confidence,
            note,
        });
    }
}

fn is_safe_public_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://"))
        && !lower.contains("localhost")
        && !lower.contains("127.0.0.1")
        && !lower.contains("0.0.0.0")
        && !lower.contains("169.254.")
        && !lower.contains(".onion")
}

fn looks_like_not_found(body: &str) -> bool {
    let lower = body.to_lowercase();
    lower.contains("page not found")
        || lower.contains("not found")
        || lower.contains("this account doesn't exist")
        || lower.contains("this account doesn’t exist")
        || lower.contains("sorry, this page isn't available")
        || lower.contains("profile not found")
}

fn profile_page_matches_seed(body: &str, seed_value: &str) -> bool {
    let Some(username) = normalize_username(seed_value) else {
        return true;
    };
    let lower = body.to_lowercase();
    let needle = username.to_lowercase();
    lower.contains(&format!("@{}", needle))
        || lower.contains(&format!("/{}", needle))
        || lower.contains(&format!("\"{}\"", needle))
        || lower.contains(&format!("'{}'", needle))
        || lower.contains(&needle)
}

fn strip_html_noise(body: &str) -> String {
    body.replace('<', " ")
        .replace('>', " ")
        .replace('"', " ")
        .replace('\'', " ")
        .replace('=', " ")
        .replace(',', " ")
        .replace(';', " ")
}

fn clean_token(raw: &str) -> String {
    raw.trim_matches(|c: char| matches!(c, ',' | ';' | ':' | ')' | '(' | '[' | ']' | '{' | '}' | '"' | '\'' | '<' | '>' | '!' | '?' | '…'))
        .to_string()
}

fn split_email(email: &str) -> Option<(String, String)> {
    let email = email.trim().to_lowercase();
    if !is_email(&email) {
        return None;
    }
    let (local, domain) = email.split_once('@')?;
    Some((local.to_string(), domain.to_string()))
}

fn username_candidates_from_email_local(local: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let base = local.trim().to_lowercase();
    if let Some(username) = normalize_username(&base) {
        candidates.push(username);
    }
    let compact = base.replace(['.', '-', '_'], "");
    if compact != base {
        if let Some(username) = normalize_username(&compact) {
            candidates.push(username);
        }
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

fn normalize_username(raw: &str) -> Option<String> {
    let username = raw.trim().trim_start_matches('@').trim();
    let lowered = username.to_lowercase();
    if username.is_empty()
        || username.len() > 64
        || lowered.starts_with("seed_")
        || username.contains(':')
        || username.contains('/')
        || username.contains('\\')
        || username.contains(' ')
    {
        return None;
    }
    if username.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-') {
        Some(username.to_string())
    } else {
        None
    }
}

fn is_email(token: &str) -> bool {
    let token = token.trim();
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

fn is_url(token: &str) -> bool {
    token.starts_with("http://")
        || token.starts_with("https://")
        || token.starts_with("t.me/")
        || token.starts_with("telegram.me/")
}

fn normalize_url(token: &str) -> String {
    if token.starts_with("t.me/") || token.starts_with("telegram.me/") {
        format!("https://{}", token)
    } else {
        token.to_string()
    }
}

fn username_from_profile_url(url: &str) -> Option<String> {
    username_from_url(url).or_else(|| {
        let trimmed = url.trim_end_matches('/');
        let last = trimmed.rsplit('/').next()?;
        normalize_username(last)
    })
}

fn username_from_url(token: &str) -> Option<String> {
    let lower = token.to_lowercase();
    for marker in ["t.me/", "telegram.me/", "github.com/", "gitlab.com/", "vk.com/", "instagram.com/", "tiktok.com/@", "youtube.com/@", "reddit.com/user/"] {
        if let Some(start) = lower.find(marker) {
            let rest = &token[start + marker.len()..];
            let username = rest
                .split(|c| matches!(c, '/' | '?' | '&' | '#'))
                .next()
                .unwrap_or_default();
            return normalize_username(username);
        }
    }
    None
}

fn username_from_at_token(token: &str) -> Option<String> {
    if !token.starts_with('@') || token.matches('@').count() > 1 {
        return None;
    }
    normalize_username(token.trim_start_matches('@'))
}

fn phone_from_token(token: &str) -> Option<String> {
    let has_phone_hint = token.starts_with('+') || token.chars().any(|c| matches!(c, '-' | '(' | ')'));
    let digits: String = token.chars().filter(|c| c.is_ascii_digit()).collect();
    if (7..=15).contains(&digits.len()) && (has_phone_hint || digits.starts_with("375") || digits.starts_with("80")) {
        Some(digits)
    } else {
        None
    }
}

fn normalize_item_value(value: &str, entity_type: &EntityType) -> String {
    match entity_type {
        EntityType::Phone => value.chars().filter(|c| c.is_ascii_digit()).collect(),
        EntityType::Email => value.trim().to_lowercase(),
        EntityType::Username | EntityType::Nickname => value.trim().trim_start_matches('@').to_lowercase(),
        EntityType::Url | EntityType::Domain => value.trim().to_lowercase(),
        _ => value.trim().to_string(),
    }
}

fn sensitivity_for(entity_type: &EntityType) -> SensitivityClass {
    match entity_type {
        EntityType::Email | EntityType::Phone => SensitivityClass::Personal,
        _ => SensitivityClass::PublicLow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_profile_tasks_from_username() {
        let seed = EntityNode { value: "@Fro_ZzZ".to_string(), entity_type: EntityType::Nickname, first_seen: 0 };
        let tasks = build_tasks_from_seed(&seed);
        assert!(tasks.iter().any(|t| t.url.as_deref() == Some("https://t.me/Fro_ZzZ")));
        assert!(tasks.iter().any(|t| t.url.as_deref() == Some("https://github.com/Fro_ZzZ")));
    }

    #[test]
    fn email_creates_username_candidates_and_search_tasks() {
        let seed = EntityNode { value: "Example.User@gmail.com".to_string(), entity_type: EntityType::Email, first_seen: 0 };
        let tasks = build_tasks_from_seed(&seed);
        assert!(tasks.iter().any(|t| t.url.as_deref() == Some("https://github.com/example.user")));
        assert!(tasks.iter().any(|t| t.url.as_deref() == Some("https://github.com/exampleuser")));
        assert!(tasks.iter().any(|t| t.kind == DiscoveryTaskKind::SearchQuery));
    }

    #[test]
    fn rejects_unsafe_url() {
        assert!(!is_safe_public_url("http://127.0.0.1:8080/admin"));
        assert!(!is_safe_public_url("http://localhost:8080/admin"));
        assert!(is_safe_public_url("https://github.com/test"));
    }

    #[test]
    fn extracts_findings_from_text() {
        let seed = EntityNode { value: "tester".to_string(), entity_type: EntityType::Username, first_seen: 0 };
        let task = make_url_task(&seed, DiscoveryTaskKind::PageProbe, "https://example.com/tester", "test_source");
        let findings = extract_findings_from_text("Contact test@example.com @other https://t.me/channel +375291234567", &task, "https://example.com/tester");
        assert!(findings.iter().any(|f| f.entity_type == EntityType::Email));
        assert!(findings.iter().any(|f| f.entity_type == EntityType::Username && f.value == "other"));
        assert!(findings.iter().any(|f| f.entity_type == EntityType::Phone));
    }
}
