use crate::evidence::{build_evidence_observation, EvidenceInput};
use crate::models::{EntityNode, EntityType, EvidenceRecord, ObservationRecord, SensitivityClass, SourceClass};
use crate::sanitize::{sanitize_text, SanitizeOptions};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PublicSearchTaskKind {
    GitHubUserSearch,
    GitHubRepoSearch,
    WebSearchQuery,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicSearchTask {
    pub task_id: String,
    pub kind: PublicSearchTaskKind,
    pub seed_type: EntityType,
    pub seed_value: String,
    pub query: String,
    pub source_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicSearchFinding {
    pub task_id: String,
    pub source_id: String,
    pub entity_type: EntityType,
    pub value: String,
    pub confidence: u8,
    pub url: Option<String>,
    pub note: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PublicSearchStats {
    pub tasks_planned: usize,
    pub tasks_executed: usize,
    pub api_errors: usize,
    pub findings_count: usize,
    pub evidences_count: usize,
    pub observations_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PublicSearchReport {
    pub generated_at: u64,
    pub stats: PublicSearchStats,
    pub tasks: Vec<PublicSearchTask>,
    pub findings: Vec<PublicSearchFinding>,
    pub evidences: Vec<EvidenceRecord>,
    pub observations: Vec<ObservationRecord>,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub async fn run_public_search_for_seeds(seeds: &[EntityNode]) -> PublicSearchReport {
    let tasks = build_public_search_tasks(seeds);
    let max_tasks = std::env::var("OSINT_PUBLIC_SEARCH_MAX_TASKS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(20);
    let github_enabled = std::env::var("OSINT_GITHUB_SEARCH")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes"))
        .unwrap_or(true);

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .expect("build public search client");

    let mut report = PublicSearchReport {
        generated_at: now_unix(),
        stats: PublicSearchStats { tasks_planned: tasks.len(), ..PublicSearchStats::default() },
        tasks: tasks.clone(),
        findings: Vec::new(),
        evidences: Vec::new(),
        observations: Vec::new(),
    };

    let mut seen_observations: HashSet<(EntityType, String)> = HashSet::new();

    for task in tasks.into_iter().take(max_tasks) {
        let findings = match task.kind {
            PublicSearchTaskKind::GitHubUserSearch if github_enabled => {
                report.stats.tasks_executed += 1;
                match search_github_users(&client, &task).await {
                    Ok(findings) => findings,
                    Err(_) => {
                        report.stats.api_errors += 1;
                        Vec::new()
                    }
                }
            }
            PublicSearchTaskKind::GitHubRepoSearch if github_enabled => {
                report.stats.tasks_executed += 1;
                match search_github_repos(&client, &task).await {
                    Ok(findings) => findings,
                    Err(_) => {
                        report.stats.api_errors += 1;
                        Vec::new()
                    }
                }
            }
            PublicSearchTaskKind::WebSearchQuery => Vec::new(),
            _ => Vec::new(),
        };

        for finding in findings {
            let normalized = normalize_item_value(&finding.value, &finding.entity_type);
            if normalized.is_empty() || !seen_observations.insert((finding.entity_type.clone(), normalized)) {
                continue;
            }

            let sensitivity = sensitivity_for(&finding.entity_type);
            let context = format!(
                "public_search source={} query={} url={:?} note={} value={}",
                finding.source_id,
                task.query,
                finding.url,
                finding.note,
                sanitize_text(&finding.value, &SanitizeOptions::default()).value
            );
            let pair = build_evidence_observation(EvidenceInput {
                source_id: finding.source_id.clone(),
                source_class: SourceClass::PublicOSINT,
                entity_type: finding.entity_type.clone(),
                raw_value: finding.value.clone(),
                raw_context: context,
                confidence: finding.confidence,
                sensitivity,
                tags: vec!["public_search".to_string(), format!("task:{:?}", task.kind)],
            });

            report.findings.push(finding);
            report.evidences.push(pair.evidence);
            report.observations.push(pair.observation);
        }
    }

    report.stats.findings_count = report.findings.len();
    report.stats.evidences_count = report.evidences.len();
    report.stats.observations_count = report.observations.len();
    report
}

pub fn build_public_search_tasks(seeds: &[EntityNode]) -> Vec<PublicSearchTask> {
    let mut tasks = Vec::new();
    let mut seen = HashSet::new();

    for seed in seeds {
        for term in search_terms_from_seed(seed) {
            if term.trim().is_empty() {
                continue;
            }
            for kind in [PublicSearchTaskKind::GitHubUserSearch, PublicSearchTaskKind::GitHubRepoSearch, PublicSearchTaskKind::WebSearchQuery] {
                let source_id = match kind {
                    PublicSearchTaskKind::GitHubUserSearch => "github_public_user_search",
                    PublicSearchTaskKind::GitHubRepoSearch => "github_public_repo_search",
                    PublicSearchTaskKind::WebSearchQuery => "planned_web_search_query",
                };
                let key = format!("{:?}:{}", kind, term.to_lowercase());
                if seen.insert(key) {
                    tasks.push(PublicSearchTask {
                        task_id: format!("ps_{}", &crate::hashing::sha256_hex(&format!("{:?}:{}", kind, term))[..16]),
                        kind,
                        seed_type: seed.entity_type.clone(),
                        seed_value: seed.value.clone(),
                        query: term.clone(),
                        source_id: source_id.to_string(),
                    });
                }
            }
        }
    }

    tasks
}

pub fn save_public_search_report(report: &PublicSearchReport, path: &str) -> Result<(), String> {
    let json = serde_json::to_string_pretty(report)
        .map_err(|err| format!("serialize public search report: {}", err))?;
    std::fs::write(path, json).map_err(|err| format!("write {}: {}", path, err))
}

pub fn observations_as_entity_nodes(report: &PublicSearchReport, limit: usize) -> Vec<EntityNode> {
    let mut nodes = Vec::new();
    let mut seen = HashSet::new();

    for obs in &report.observations {
        if nodes.len() >= limit {
            break;
        }
        if !matches!(obs.entity_type, EntityType::Username | EntityType::Url | EntityType::Domain | EntityType::Email) {
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

async fn search_github_users(client: &Client, task: &PublicSearchTask) -> Result<Vec<PublicSearchFinding>, reqwest::Error> {
    let query = url_encode(&task.query);
    let url = format!("https://api.github.com/search/users?q={}&per_page=5", query);
    let body = fetch_json(client, &url).await?;
    let mut findings = Vec::new();

    if let Some(items) = body.get("items").and_then(|v| v.as_array()) {
        for item in items.iter().take(5) {
            if let Some(login) = item.get("login").and_then(|v| v.as_str()) {
                findings.push(PublicSearchFinding {
                    task_id: task.task_id.clone(),
                    source_id: task.source_id.clone(),
                    entity_type: EntityType::Username,
                    value: login.to_string(),
                    confidence: confidence_for_github_match(login, &task.query),
                    url: item.get("html_url").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    note: "github_user_search_login".to_string(),
                });
                if let Some(html_url) = item.get("html_url").and_then(|v| v.as_str()) {
                    findings.push(PublicSearchFinding {
                        task_id: task.task_id.clone(),
                        source_id: task.source_id.clone(),
                        entity_type: EntityType::Url,
                        value: html_url.to_string(),
                        confidence: 55,
                        url: Some(html_url.to_string()),
                        note: "github_user_profile_url".to_string(),
                    });
                }
            }
        }
    }

    Ok(findings)
}

async fn search_github_repos(client: &Client, task: &PublicSearchTask) -> Result<Vec<PublicSearchFinding>, reqwest::Error> {
    let query = url_encode(&task.query);
    let url = format!("https://api.github.com/search/repositories?q={}&per_page=5", query);
    let body = fetch_json(client, &url).await?;
    let mut findings = Vec::new();

    if let Some(items) = body.get("items").and_then(|v| v.as_array()) {
        for item in items.iter().take(5) {
            if let Some(full_name) = item.get("full_name").and_then(|v| v.as_str()) {
                findings.push(PublicSearchFinding {
                    task_id: task.task_id.clone(),
                    source_id: task.source_id.clone(),
                    entity_type: EntityType::Url,
                    value: format!("https://github.com/{}", full_name),
                    confidence: confidence_for_github_match(full_name, &task.query).saturating_sub(10),
                    url: Some(format!("https://github.com/{}", full_name)),
                    note: "github_repo_search_result".to_string(),
                });
                if let Some(owner) = item.get("owner").and_then(|v| v.get("login")).and_then(|v| v.as_str()) {
                    findings.push(PublicSearchFinding {
                        task_id: task.task_id.clone(),
                        source_id: task.source_id.clone(),
                        entity_type: EntityType::Username,
                        value: owner.to_string(),
                        confidence: 45,
                        url: item.get("html_url").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        note: "github_repo_owner".to_string(),
                    });
                }
            }
        }
    }

    Ok(findings)
}

async fn fetch_json(client: &Client, url: &str) -> Result<Value, reqwest::Error> {
    client
        .get(url)
        .header("User-Agent", "XGEN-PublicSearch/1.0 (+local research)")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .json::<Value>()
        .await
}

fn search_terms_from_seed(seed: &EntityNode) -> Vec<String> {
    let mut terms = Vec::new();
    match &seed.entity_type {
        EntityType::Nickname | EntityType::Username => {
            if let Some(username) = normalize_username(&seed.value) {
                terms.push(username);
            }
        }
        EntityType::Email => {
            let email = seed.value.trim().to_lowercase();
            if is_email(&email) {
                terms.push(email.clone());
                if let Some((local, _domain)) = email.split_once('@') {
                    terms.extend(username_candidates_from_email_local(local));
                }
            }
        }
        EntityType::FullName => {
            let name = seed.value.trim();
            if !name.is_empty() {
                terms.push(name.to_string());
                terms.push(transliterate_basic(name));
            }
        }
        EntityType::Url | EntityType::Domain => {
            terms.push(seed.value.trim().to_string());
        }
        _ => {}
    }

    terms.retain(|t| !t.trim().is_empty());
    terms.sort();
    terms.dedup();
    terms
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

fn confidence_for_github_match(value: &str, query: &str) -> u8 {
    let value = value.to_lowercase();
    let query = query.to_lowercase();
    if value == query {
        75
    } else if value.contains(&query) || query.contains(&value) {
        60
    } else {
        40
    }
}

fn normalize_item_value(value: &str, entity_type: &EntityType) -> String {
    match entity_type {
        EntityType::Email => value.trim().to_lowercase(),
        EntityType::Username | EntityType::Nickname => value.trim().trim_start_matches('@').to_lowercase(),
        EntityType::Url | EntityType::Domain => value.trim().to_lowercase(),
        EntityType::Phone => value.chars().filter(|c| c.is_ascii_digit()).collect(),
        _ => value.trim().to_string(),
    }
}

fn sensitivity_for(entity_type: &EntityType) -> SensitivityClass {
    match entity_type {
        EntityType::Email | EntityType::Phone => SensitivityClass::Personal,
        _ => SensitivityClass::PublicLow,
    }
}

fn url_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}

fn transliterate_basic(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            'а' | 'А' => "a", 'б' | 'Б' => "b", 'в' | 'В' => "v", 'г' | 'Г' => "g",
            'д' | 'Д' => "d", 'е' | 'Е' => "e", 'ё' | 'Ё' => "e", 'ж' | 'Ж' => "zh",
            'з' | 'З' => "z", 'и' | 'И' => "i", 'й' | 'Й' => "y", 'к' | 'К' => "k",
            'л' | 'Л' => "l", 'м' | 'М' => "m", 'н' | 'Н' => "n", 'о' | 'О' => "o",
            'п' | 'П' => "p", 'р' | 'Р' => "r", 'с' | 'С' => "s", 'т' | 'Т' => "t",
            'у' | 'У' => "u", 'ф' | 'Ф' => "f", 'х' | 'Х' => "h", 'ц' | 'Ц' => "ts",
            'ч' | 'Ч' => "ch", 'ш' | 'Ш' => "sh", 'щ' | 'Щ' => "sch", 'ы' | 'Ы' => "y",
            'э' | 'Э' => "e", 'ю' | 'Ю' => "yu", 'я' | 'Я' => "ya", 'ь' | 'Ь' | 'ъ' | 'Ъ' => "",
            _ => "",
        })
        .collect::<Vec<_>>()
        .join("")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_tasks_from_email() {
        let seed = EntityNode { value: "Example.User@gmail.com".to_string(), entity_type: EntityType::Email, first_seen: 0 };
        let tasks = build_public_search_tasks(&[seed]);
        assert!(tasks.iter().any(|t| t.query == "example.user"));
        assert!(tasks.iter().any(|t| t.query == "exampleuser"));
        assert!(tasks.iter().any(|t| t.kind == PublicSearchTaskKind::GitHubUserSearch));
    }

    #[test]
    fn username_rejects_seed_noise() {
        assert!(normalize_username("seed_nickname:@test").is_none());
        assert!(normalize_username("bad:user").is_none());
        assert_eq!(normalize_username("@Fro_ZzZ"), Some("Fro_ZzZ".to_string()));
    }

    #[test]
    fn url_encoding_encodes_cyrillic() {
        let encoded = url_encode("Куранец Яков");
        assert!(encoded.contains("%"));
        assert!(encoded.contains("+"));
    }
}
