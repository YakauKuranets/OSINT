use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhoneSearchProviderKind {
    GitHubCode,
    GitHubIssues,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhoneSearchInput {
    pub phone_e164: Option<String>,
    pub digits: String,
    pub country_code: Option<String>,
    pub national_number: Option<String>,
    pub terms: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhoneSearchHit {
    pub provider_id: String,
    pub url: Option<String>,
    pub matched_value: String,
    pub context_snippet: String,
    pub confidence: u8,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhoneSearchProviderSummary {
    pub provider_id: String,
    pub enabled: bool,
    pub terms_attempted: usize,
    pub hits: usize,
    pub errors: usize,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PhoneSearchProviderReport {
    pub providers: Vec<PhoneSearchProviderSummary>,
    pub hits: Vec<PhoneSearchHit>,
}

#[derive(Debug, Clone)]
struct PhoneSearchProvider {
    kind: PhoneSearchProviderKind,
    id: &'static str,
}

impl PhoneSearchProvider {
    fn new(kind: PhoneSearchProviderKind) -> Self {
        let id = match kind {
            PhoneSearchProviderKind::GitHubCode => "phone_github_code_search",
            PhoneSearchProviderKind::GitHubIssues => "phone_github_issue_search",
        };
        Self { kind, id }
    }
}

pub async fn run_phone_search_providers(client: &Client, input: &PhoneSearchInput) -> PhoneSearchProviderReport {
    let providers = configured_providers();
    let terms = focused_phone_terms(input)
        .into_iter()
        .take(phone_search_max_terms())
        .collect::<Vec<_>>();

    let mut report = PhoneSearchProviderReport::default();
    for provider in providers {
        let mut summary = PhoneSearchProviderSummary {
            provider_id: provider.id.to_string(),
            enabled: true,
            terms_attempted: 0,
            hits: 0,
            errors: 0,
            last_error: None,
        };

        for term in &terms {
            summary.terms_attempted += 1;
            match run_provider(client, &provider, input, term).await {
                Ok(mut hits) => {
                    summary.hits += hits.len();
                    report.hits.append(&mut hits);
                }
                Err(err) => {
                    summary.errors += 1;
                    summary.last_error = Some(err);
                }
            }
        }

        report.providers.push(summary);
    }

    dedupe_hits(&mut report.hits);
    report
}

async fn run_provider(client: &Client, provider: &PhoneSearchProvider, input: &PhoneSearchInput, term: &str) -> Result<Vec<PhoneSearchHit>, String> {
    match provider.kind {
        PhoneSearchProviderKind::GitHubCode => search_github_code_for_phone(client, input, term).await,
        PhoneSearchProviderKind::GitHubIssues => search_github_issues_for_phone(client, input, term).await,
    }
}

fn configured_providers() -> Vec<PhoneSearchProvider> {
    let raw = std::env::var("XGEN_PHONE_PROVIDERS").unwrap_or_else(|_| "github_code,github_issues".to_string());
    let mut providers = Vec::new();
    for item in raw.split(',').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()) {
        match item.as_str() {
            "github_code" | "phone_github_code_search" => providers.push(PhoneSearchProvider::new(PhoneSearchProviderKind::GitHubCode)),
            "github_issues" | "phone_github_issue_search" => providers.push(PhoneSearchProvider::new(PhoneSearchProviderKind::GitHubIssues)),
            _ => {}
        }
    }
    if providers.is_empty() {
        providers.push(PhoneSearchProvider::new(PhoneSearchProviderKind::GitHubCode));
        providers.push(PhoneSearchProvider::new(PhoneSearchProviderKind::GitHubIssues));
    }
    providers
}

fn phone_search_max_terms() -> usize {
    std::env::var("XGEN_PHONE_SEARCH_MAX_TERMS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(4)
        .clamp(1, 12)
}

fn phone_search_per_page() -> usize {
    std::env::var("XGEN_PHONE_SEARCH_PER_PAGE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(5)
        .clamp(1, 10)
}

pub fn focused_phone_terms(input: &PhoneSearchInput) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(e164) = &input.phone_e164 { out.push(e164.clone()); }
    if !input.digits.is_empty() { out.push(input.digits.clone()); }
    if input.country_code.as_deref() == Some("375") {
        if let Some(national) = input.national_number.as_deref() {
            if national.len() == 9 {
                out.push(format!("80{}", national));
            }
        }
    }
    for term in &input.terms {
        if !term.starts_with("site:") && !term.contains(' ') && !term.contains('"') {
            out.push(term.clone());
        }
    }
    out.retain(|term| !term.trim().is_empty());
    out.sort();
    out.dedup();
    out
}

async fn search_github_code_for_phone(client: &Client, input: &PhoneSearchInput, term: &str) -> Result<Vec<PhoneSearchHit>, String> {
    let query = format!("{} in:file", term);
    let url = format!("https://api.github.com/search/code?q={}&per_page={}", url_encode(&query), phone_search_per_page());
    let body = github_json(client, &url).await?;
    if let Some(message) = body.get("message").and_then(Value::as_str) {
        if body.get("items").is_none() {
            return Err(format!("github_code: {}", message));
        }
    }

    let mut hits = Vec::new();
    if let Some(items) = body.get("items").and_then(Value::as_array) {
        for item in items.iter().take(phone_search_per_page()) {
            let html_url = item.get("html_url").and_then(Value::as_str).map(|s| s.to_string());
            let repo = item.pointer("/repository/full_name").and_then(Value::as_str).unwrap_or("unknown_repo");
            let path = item.get("path").and_then(Value::as_str).unwrap_or("unknown_path");
            hits.push(PhoneSearchHit {
                provider_id: "phone_github_code_search".to_string(),
                url: html_url,
                matched_value: input.phone_e164.clone().unwrap_or_else(|| input.digits.clone()),
                context_snippet: format!("GitHub code search result: {}/{} matched term {}; exact line text requires opening public source", repo, path, term),
                confidence: 65,
                note: "github_code_public_mention".to_string(),
            });
        }
    }
    Ok(hits)
}

async fn search_github_issues_for_phone(client: &Client, input: &PhoneSearchInput, term: &str) -> Result<Vec<PhoneSearchHit>, String> {
    let query = format!("{} in:title,body,comments", term);
    let url = format!("https://api.github.com/search/issues?q={}&per_page={}", url_encode(&query), phone_search_per_page());
    let body = github_json(client, &url).await?;
    if let Some(message) = body.get("message").and_then(Value::as_str) {
        if body.get("items").is_none() {
            return Err(format!("github_issues: {}", message));
        }
    }

    let mut hits = Vec::new();
    if let Some(items) = body.get("items").and_then(Value::as_array) {
        for item in items.iter().take(phone_search_per_page()) {
            let html_url = item.get("html_url").and_then(Value::as_str).map(|s| s.to_string());
            let title = item.get("title").and_then(Value::as_str).unwrap_or("untitled");
            hits.push(PhoneSearchHit {
                provider_id: "phone_github_issue_search".to_string(),
                url: html_url,
                matched_value: input.phone_e164.clone().unwrap_or_else(|| input.digits.clone()),
                context_snippet: format!("GitHub issue/discussion search result: {} matched term {}", title, term),
                confidence: 60,
                note: "github_issue_public_mention".to_string(),
            });
        }
    }
    Ok(hits)
}

async fn github_json(client: &Client, url: &str) -> Result<Value, String> {
    client
        .get(url)
        .header("User-Agent", "XGEN-PhoneSearch/1.0 (+local self-audit)")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|err| format!("request failed: {}", err))?
        .json::<Value>()
        .await
        .map_err(|err| format!("json parse failed: {}", err))
}

fn dedupe_hits(hits: &mut Vec<PhoneSearchHit>) {
    let mut seen = std::collections::HashSet::new();
    hits.retain(|hit| seen.insert(format!("{}:{:?}:{}", hit.provider_id, hit.url, hit.matched_value)));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focused_terms_include_e164_and_trunk_variant() {
        let input = PhoneSearchInput {
            phone_e164: Some("+375257997676".to_string()),
            digits: "375257997676".to_string(),
            country_code: Some("375".to_string()),
            national_number: Some("257997676".to_string()),
            terms: vec!["site:t.me \"+375257997676\"".to_string(), "80257997676".to_string()],
        };
        let terms = focused_phone_terms(&input);
        assert!(terms.contains(&"+375257997676".to_string()));
        assert!(terms.contains(&"375257997676".to_string()));
        assert!(terms.contains(&"80257997676".to_string()));
        assert!(!terms.iter().any(|term| term.starts_with("site:")));
    }
}
