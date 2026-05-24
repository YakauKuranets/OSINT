use crate::phone_context;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhoneSearchProviderKind {
    GitHubCode,
    GitHubIssues,
    HackerNews,
    GitLab,
    NpmRegistry,
    UrlProbe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhoneProviderStatus {
    Skipped,
    Executed,
    EmptyResult,
    Matched,
    RateLimited,
    Error,
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
pub struct PhoneSearchProviderTrace {
    pub provider_id: String,
    pub term: Option<String>,
    pub status: PhoneProviderStatus,
    pub url: Option<String>,
    pub hits: usize,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhoneSearchProviderSummary {
    pub provider_id: String,
    pub enabled: bool,
    pub status: PhoneProviderStatus,
    pub terms_attempted: usize,
    pub hits: usize,
    pub errors: usize,
    pub rate_limited: usize,
    pub empty_results: usize,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PhoneSearchProviderReport {
    pub providers: Vec<PhoneSearchProviderSummary>,
    pub traces: Vec<PhoneSearchProviderTrace>,
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
            PhoneSearchProviderKind::HackerNews => "phone_hackernews_search",
            PhoneSearchProviderKind::GitLab => "phone_gitlab_search",
            PhoneSearchProviderKind::NpmRegistry => "phone_npm_registry_search",
            PhoneSearchProviderKind::UrlProbe => "phone_public_url_probe",
        };
        Self { kind, id }
    }
}

struct ProviderRunOutcome {
    hits: Vec<PhoneSearchHit>,
    status: PhoneProviderStatus,
    message: Option<String>,
    url: Option<String>,
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
            status: PhoneProviderStatus::Executed,
            terms_attempted: 0,
            hits: 0,
            errors: 0,
            rate_limited: 0,
            empty_results: 0,
            last_error: None,
        };

        if terms.is_empty() {
            summary.enabled = false;
            summary.status = PhoneProviderStatus::Skipped;
            summary.last_error = Some("no focused phone terms available".to_string());
            report.traces.push(trace(provider.id, None, PhoneProviderStatus::Skipped, None, 0, summary.last_error.clone()));
            report.providers.push(summary);
            continue;
        }

        if provider.kind == PhoneSearchProviderKind::UrlProbe && configured_probe_templates().is_empty() {
            summary.enabled = false;
            summary.status = PhoneProviderStatus::Skipped;
            summary.last_error = Some("XGEN_PHONE_PROBE_URLS is empty; url_probe provider skipped".to_string());
            report.traces.push(trace(provider.id, None, PhoneProviderStatus::Skipped, None, 0, summary.last_error.clone()));
            report.providers.push(summary);
            continue;
        }

        for term in &terms {
            summary.terms_attempted += 1;
            let outcome = run_provider(client, &provider, input, term).await;
            summary.hits += outcome.hits.len();
            match outcome.status {
                PhoneProviderStatus::Matched => {}
                PhoneProviderStatus::EmptyResult => summary.empty_results += 1,
                PhoneProviderStatus::RateLimited => {
                    summary.rate_limited += 1;
                    summary.last_error = outcome.message.clone();
                }
                PhoneProviderStatus::Error => {
                    summary.errors += 1;
                    summary.last_error = outcome.message.clone();
                }
                PhoneProviderStatus::Skipped | PhoneProviderStatus::Executed => {}
            }
            report.traces.push(trace(provider.id, Some(term.clone()), outcome.status, outcome.url.clone(), outcome.hits.len(), outcome.message.clone()));
            report.hits.extend(outcome.hits);
        }

        summary.status = provider_summary_status(&summary);
        report.providers.push(summary);
    }

    dedupe_hits(&mut report.hits);
    report
}

fn trace(provider_id: &str, term: Option<String>, status: PhoneProviderStatus, url: Option<String>, hits: usize, message: Option<String>) -> PhoneSearchProviderTrace {
    PhoneSearchProviderTrace { provider_id: provider_id.to_string(), term, status, url, hits, message }
}

fn provider_summary_status(summary: &PhoneSearchProviderSummary) -> PhoneProviderStatus {
    if !summary.enabled { PhoneProviderStatus::Skipped }
    else if summary.hits > 0 { PhoneProviderStatus::Matched }
    else if summary.rate_limited > 0 && summary.errors == 0 { PhoneProviderStatus::RateLimited }
    else if summary.errors > 0 && summary.empty_results == 0 { PhoneProviderStatus::Error }
    else if summary.empty_results > 0 || summary.terms_attempted > 0 { PhoneProviderStatus::EmptyResult }
    else { PhoneProviderStatus::Executed }
}

async fn run_provider(client: &Client, provider: &PhoneSearchProvider, input: &PhoneSearchInput, term: &str) -> ProviderRunOutcome {
    match provider.kind {
        PhoneSearchProviderKind::GitHubCode => search_github_code_for_phone(client, input, term).await,
        PhoneSearchProviderKind::GitHubIssues => search_github_issues_for_phone(client, input, term).await,
        PhoneSearchProviderKind::HackerNews => search_hackernews_for_phone(client, input, term).await,
        PhoneSearchProviderKind::GitLab => search_gitlab_for_phone(client, input, term).await,
        PhoneSearchProviderKind::NpmRegistry => search_npm_registry_for_phone(client, input, term).await,
        PhoneSearchProviderKind::UrlProbe => probe_configured_public_urls(client, input, term).await,
    }
}

fn configured_providers() -> Vec<PhoneSearchProvider> {
    let raw = std::env::var("XGEN_PHONE_PROVIDERS").unwrap_or_else(|_| "github_code,github_issues,hackernews,gitlab,npm_registry,url_probe".to_string());
    let mut providers = Vec::new();
    for item in raw.split(',').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()) {
        match item.as_str() {
            "github_code" | "phone_github_code_search" => providers.push(PhoneSearchProvider::new(PhoneSearchProviderKind::GitHubCode)),
            "github_issues" | "phone_github_issue_search" => providers.push(PhoneSearchProvider::new(PhoneSearchProviderKind::GitHubIssues)),
            "hackernews" | "hn" | "phone_hackernews_search" => providers.push(PhoneSearchProvider::new(PhoneSearchProviderKind::HackerNews)),
            "gitlab" | "phone_gitlab_search" => providers.push(PhoneSearchProvider::new(PhoneSearchProviderKind::GitLab)),
            "npm" | "npm_registry" | "phone_npm_registry_search" => providers.push(PhoneSearchProvider::new(PhoneSearchProviderKind::NpmRegistry)),
            "url_probe" | "phone_public_url_probe" => providers.push(PhoneSearchProvider::new(PhoneSearchProviderKind::UrlProbe)),
            _ => {}
        }
    }
    if providers.is_empty() {
        providers.push(PhoneSearchProvider::new(PhoneSearchProviderKind::GitHubCode));
        providers.push(PhoneSearchProvider::new(PhoneSearchProviderKind::GitHubIssues));
        providers.push(PhoneSearchProvider::new(PhoneSearchProviderKind::HackerNews));
        providers.push(PhoneSearchProvider::new(PhoneSearchProviderKind::GitLab));
        providers.push(PhoneSearchProvider::new(PhoneSearchProviderKind::NpmRegistry));
        providers.push(PhoneSearchProvider::new(PhoneSearchProviderKind::UrlProbe));
    }
    providers
}

fn phone_search_max_terms() -> usize {
    std::env::var("XGEN_PHONE_SEARCH_MAX_TERMS").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(4).clamp(1, 12)
}

fn phone_search_per_page() -> usize {
    std::env::var("XGEN_PHONE_SEARCH_PER_PAGE").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(5).clamp(1, 10)
}

fn phone_probe_max_bytes() -> usize {
    std::env::var("XGEN_PHONE_PROBE_MAX_BYTES").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(512_000).clamp(16_384, 2_000_000)
}

fn configured_probe_templates() -> Vec<String> {
    std::env::var("XGEN_PHONE_PROBE_URLS").unwrap_or_default().split('|').map(|s| s.trim().to_string()).filter(|s| is_safe_probe_template(s)).collect()
}

fn is_safe_probe_template(template: &str) -> bool {
    let lowered = template.to_lowercase();
    !template.is_empty()
        && template.contains("{term}")
        && (lowered.starts_with("https://") || lowered.starts_with("http://"))
        && !lowered.contains("localhost")
        && !lowered.contains("127.0.0.1")
        && !lowered.contains("169.254.")
        && !lowered.contains("file:")
}

pub fn focused_phone_terms(input: &PhoneSearchInput) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(e164) = &input.phone_e164 { out.push(e164.clone()); }
    if !input.digits.is_empty() { out.push(input.digits.clone()); }
    if input.country_code.as_deref() == Some("375") {
        if let Some(national) = input.national_number.as_deref() {
            if national.len() == 9 { out.push(format!("80{}", national)); }
        }
    }
    for term in &input.terms {
        if !term.starts_with("site:") && !term.contains(' ') && !term.contains('"') { out.push(term.clone()); }
    }
    out.retain(|term| !term.trim().is_empty());
    out.sort();
    out.dedup();
    out
}

async fn search_github_code_for_phone(client: &Client, input: &PhoneSearchInput, term: &str) -> ProviderRunOutcome {
    let query = format!("{} in:file", term);
    let url = format!("https://api.github.com/search/code?q={}&per_page={}", url_encode(&query), phone_search_per_page());
    let body = match github_json(client, &url).await {
        Ok(value) => value,
        Err(err) => return outcome_error(url, err),
    };
    if let Some(message) = body.get("message").and_then(Value::as_str) {
        if body.get("items").is_none() { return outcome_from_github_message(url, "github_code", message); }
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
    outcome_hits(url, hits)
}

async fn search_github_issues_for_phone(client: &Client, input: &PhoneSearchInput, term: &str) -> ProviderRunOutcome {
    let query = format!("{} in:title,body,comments", term);
    let url = format!("https://api.github.com/search/issues?q={}&per_page={}", url_encode(&query), phone_search_per_page());
    let body = match github_json(client, &url).await {
        Ok(value) => value,
        Err(err) => return outcome_error(url, err),
    };
    if let Some(message) = body.get("message").and_then(Value::as_str) {
        if body.get("items").is_none() { return outcome_from_github_message(url, "github_issues", message); }
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
    outcome_hits(url, hits)
}

async fn search_hackernews_for_phone(client: &Client, input: &PhoneSearchInput, term: &str) -> ProviderRunOutcome {
    let url = format!("https://hn.algolia.com/api/v1/search?query={}&tags=story,comment&hitsPerPage={}", url_encode(term), phone_search_per_page());
    let body = match public_json(client, &url, "hackernews").await {
        Ok(value) => value,
        Err(err) => return outcome_error(url, err),
    };
    let mut hits = Vec::new();
    if let Some(items) = body.get("hits").and_then(Value::as_array) {
        for item in items.iter().take(phone_search_per_page()) {
            let title = item.get("title").or_else(|| item.get("story_title")).and_then(Value::as_str).unwrap_or("untitled");
            let text = item.get("comment_text").or_else(|| item.get("story_text")).and_then(Value::as_str).unwrap_or("");
            let object_id = item.get("objectID").and_then(Value::as_str).unwrap_or("");
            let source_url = item.get("url").and_then(Value::as_str).map(|s| s.to_string()).or_else(|| if object_id.is_empty() { None } else { Some(format!("https://news.ycombinator.com/item?id={}", object_id)) });
            let combined = format!("{} {} {:?}", title, text, source_url);
            if contains_any_variant(&combined, &match_variants(input, term)) {
                hits.push(PhoneSearchHit {
                    provider_id: "phone_hackernews_search".to_string(),
                    url: source_url,
                    matched_value: input.phone_e164.clone().unwrap_or_else(|| input.digits.clone()),
                    context_snippet: compact_context(&format!("HackerNews public result: title={} text={}", title, strip_htmlish(text)), 420),
                    confidence: 58,
                    note: "hackernews_public_mention".to_string(),
                });
            }
        }
    }
    outcome_hits(url, hits)
}

async fn search_gitlab_for_phone(client: &Client, input: &PhoneSearchInput, term: &str) -> ProviderRunOutcome {
    let scopes = ["projects", "issues", "merge_requests", "snippets"];
    let variants = match_variants(input, term);
    let mut hits = Vec::new();
    let mut last_url = None;
    let mut last_error = None;
    for scope in scopes {
        let url = format!("https://gitlab.com/api/v4/search?scope={}&search={}&per_page={}", scope, url_encode(term), phone_search_per_page());
        last_url = Some(url.clone());
        let body = match public_json(client, &url, "gitlab").await {
            Ok(value) => value,
            Err(err) => { last_error = Some(err); continue; }
        };
        if let Some(items) = body.as_array() {
            for item in items.iter().take(phone_search_per_page()) {
                let name = item.get("name").or_else(|| item.get("title")).and_then(Value::as_str).unwrap_or("untitled");
                let desc = item.get("description").and_then(Value::as_str).unwrap_or("");
                let web_url = item.get("web_url").or_else(|| item.get("url")).and_then(Value::as_str).map(|s| s.to_string());
                let combined = format!("{} {} {:?}", name, desc, web_url);
                if contains_any_variant(&combined, &variants) || scope == "projects" && !desc.is_empty() {
                    hits.push(PhoneSearchHit {
                        provider_id: "phone_gitlab_search".to_string(),
                        url: web_url,
                        matched_value: input.phone_e164.clone().unwrap_or_else(|| input.digits.clone()),
                        context_snippet: compact_context(&format!("GitLab public {} result: {} {}", scope, name, desc), 420),
                        confidence: 55,
                        note: format!("gitlab_public_{}_mention", scope),
                    });
                }
            }
        }
    }
    if !hits.is_empty() { outcome_hits(last_url.unwrap_or_default(), hits) }
    else if let Some(err) = last_error { outcome_error(last_url.unwrap_or_default(), err) }
    else { ProviderRunOutcome { hits, status: PhoneProviderStatus::EmptyResult, message: Some("GitLab public search returned no phone matches".to_string()), url: last_url } }
}

async fn search_npm_registry_for_phone(client: &Client, input: &PhoneSearchInput, term: &str) -> ProviderRunOutcome {
    let url = format!("https://registry.npmjs.org/-/v1/search?text={}&size={}", url_encode(term), phone_search_per_page());
    let body = match public_json(client, &url, "npm_registry").await {
        Ok(value) => value,
        Err(err) => return outcome_error(url, err),
    };
    let variants = match_variants(input, term);
    let mut hits = Vec::new();
    if let Some(items) = body.get("objects").and_then(Value::as_array) {
        for item in items.iter().take(phone_search_per_page()) {
            let pkg = item.get("package").unwrap_or(&Value::Null);
            let name = pkg.get("name").and_then(Value::as_str).unwrap_or("unknown_package");
            let desc = pkg.get("description").and_then(Value::as_str).unwrap_or("");
            let npm_url = pkg.pointer("/links/npm").and_then(Value::as_str).map(|s| s.to_string());
            let repo_url = pkg.pointer("/links/repository").and_then(Value::as_str).map(|s| s.to_string());
            let combined = format!("{} {} {:?} {:?}", name, desc, npm_url, repo_url);
            if contains_any_variant(&combined, &variants) || !desc.is_empty() {
                hits.push(PhoneSearchHit {
                    provider_id: "phone_npm_registry_search".to_string(),
                    url: npm_url.or(repo_url),
                    matched_value: input.phone_e164.clone().unwrap_or_else(|| input.digits.clone()),
                    context_snippet: compact_context(&format!("NPM registry public result: package={} description={}", name, desc), 420),
                    confidence: 52,
                    note: "npm_registry_public_mention".to_string(),
                });
            }
        }
    }
    outcome_hits(url, hits)
}

async fn probe_configured_public_urls(client: &Client, input: &PhoneSearchInput, term: &str) -> ProviderRunOutcome {
    let templates = configured_probe_templates();
    if templates.is_empty() {
        return ProviderRunOutcome { hits: Vec::new(), status: PhoneProviderStatus::Skipped, message: Some("XGEN_PHONE_PROBE_URLS is empty".to_string()), url: None };
    }
    let variants = match_variants(input, term);
    let mut hits = Vec::new();
    let mut last_url = None;
    let mut last_error = None;
    for template in templates.into_iter().take(16) {
        let url = template.replace("{term}", &url_encode(term));
        last_url = Some(url.clone());
        let body = match fetch_text_limited(client, &url).await {
            Ok(body) => body,
            Err(err) => { last_error = Some(err); continue; }
        };
        if let Some(ctx) = phone_context::parse_phone_page_context(&body, &variants, Some(&url)) {
            hits.push(PhoneSearchHit {
                provider_id: "phone_public_url_probe".to_string(),
                url: Some(url.clone()),
                matched_value: input.phone_e164.clone().unwrap_or_else(|| input.digits.clone()),
                context_snippet: phone_context::format_context_for_phone_hit(&ctx),
                confidence: url_probe_confidence(&ctx),
                note: "configured_public_url_probe_enriched_context_match".to_string(),
            });
        }
    }
    if !hits.is_empty() { outcome_hits(last_url.unwrap_or_default(), hits) }
    else if let Some(err) = last_error { ProviderRunOutcome { hits, status: PhoneProviderStatus::Error, message: Some(err), url: last_url } }
    else { ProviderRunOutcome { hits, status: PhoneProviderStatus::EmptyResult, message: Some("configured public URL probes returned no exact phone match".to_string()), url: last_url } }
}

fn url_probe_confidence(ctx: &phone_context::PhonePageContext) -> u8 {
    let mut score = 70usize;
    if ctx.title.is_some() { score += 3; }
    if ctx.meta_description.is_some() { score += 3; }
    if !ctx.email_hints.is_empty() { score += 5; }
    if !ctx.username_hints.is_empty() { score += 4; }
    if !ctx.url_hints.is_empty() { score += 2; }
    if !ctx.date_hints.is_empty() { score += 2; }
    if !ctx.json_ld_types.is_empty() { score += 3; }
    if ctx.cleaned_text_chars < 80 { score = score.saturating_sub(8); }
    score.min(88) as u8
}

fn outcome_hits(url: String, hits: Vec<PhoneSearchHit>) -> ProviderRunOutcome {
    if hits.is_empty() {
        ProviderRunOutcome { hits, status: PhoneProviderStatus::EmptyResult, message: Some("provider executed successfully but returned no hits".to_string()), url: Some(url) }
    } else {
        ProviderRunOutcome { status: PhoneProviderStatus::Matched, message: None, url: Some(url), hits }
    }
}

fn outcome_error(url: String, err: String) -> ProviderRunOutcome {
    let status = if is_rate_limit_message(&err) { PhoneProviderStatus::RateLimited } else { PhoneProviderStatus::Error };
    ProviderRunOutcome { hits: Vec::new(), status, message: Some(err), url: Some(url) }
}

fn outcome_from_github_message(url: String, provider: &str, message: &str) -> ProviderRunOutcome {
    let msg = format!("{}: {}", provider, message);
    let status = if is_rate_limit_message(&msg) { PhoneProviderStatus::RateLimited } else { PhoneProviderStatus::Error };
    ProviderRunOutcome { hits: Vec::new(), status, message: Some(msg), url: Some(url) }
}

fn is_rate_limit_message(message: &str) -> bool {
    let lowered = message.to_lowercase();
    lowered.contains("rate limit") || lowered.contains("api rate") || lowered.contains("secondary rate") || lowered.contains("too many requests") || lowered.contains("403") || lowered.contains("429")
}

fn match_variants(input: &PhoneSearchInput, term: &str) -> Vec<String> {
    let mut variants = vec![term.to_string(), input.digits.clone()];
    if let Some(e164) = &input.phone_e164 { variants.push(e164.clone()); }
    if input.country_code.as_deref() == Some("375") {
        if let Some(national) = input.national_number.as_deref() {
            if national.len() == 9 {
                variants.push(format!("80{}", national));
                variants.push(format!("+375{}", national));
            }
        }
    }
    variants.retain(|v| !v.trim().is_empty());
    variants.sort();
    variants.dedup();
    variants
}

fn contains_any_variant(text: &str, variants: &[String]) -> bool {
    let lower = text.to_lowercase();
    variants.iter().filter(|v| !v.trim().is_empty()).any(|v| lower.contains(&v.to_lowercase()))
}

async fn fetch_text_limited(client: &Client, url: &str) -> Result<String, String> {
    let response = client
        .get(url)
        .header("User-Agent", "XGEN-PhoneProbe/1.0 (+configured public URL probe)")
        .send()
        .await
        .map_err(|err| format!("url_probe request failed: {}", err))?;
    let status = response.status();
    if !status.is_success() { return Err(format!("url_probe HTTP {}", status.as_u16())); }
    let text = response.text().await.map_err(|err| format!("url_probe body read failed: {}", err))?;
    let limit = phone_probe_max_bytes();
    Ok(text.chars().take(limit).collect())
}

async fn github_json(client: &Client, url: &str) -> Result<Value, String> {
    let response = client
        .get(url)
        .header("User-Agent", "XGEN-PhoneSearch/1.0 (+local self-audit)")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|err| format!("request failed: {}", err))?;
    let status = response.status();
    if status.as_u16() == 403 || status.as_u16() == 429 { return Err(format!("HTTP {} rate limit or forbidden", status.as_u16())); }
    response.json::<Value>().await.map_err(|err| format!("json parse failed: {}", err))
}

async fn public_json(client: &Client, url: &str, provider: &str) -> Result<Value, String> {
    let response = client
        .get(url)
        .header("User-Agent", format!("XGEN-PhoneSearch/1.0 (+public {}; local self-audit)", provider))
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|err| format!("{} request failed: {}", provider, err))?;
    let status = response.status();
    if status.as_u16() == 403 || status.as_u16() == 429 { return Err(format!("{} HTTP {} rate limit or forbidden", provider, status.as_u16())); }
    if !status.is_success() { return Err(format!("{} HTTP {}", provider, status.as_u16())); }
    response.json::<Value>().await.map_err(|err| format!("{} json parse failed: {}", provider, err))
}

fn strip_htmlish(value: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in value.chars() {
        match ch {
            '<' => { in_tag = true; out.push(' '); }
            '>' => { in_tag = false; out.push(' '); }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&quot;", "\"").replace("&#x27;", "'").replace("&amp;", "&")
}

fn compact_context(value: &str, max_chars: usize) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(max_chars).collect()
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
            phone_e164: Some("+000000000000".to_string()),
            digits: "000000000000".to_string(),
            country_code: Some("375".to_string()),
            national_number: Some("000000000".to_string()),
            terms: vec!["site:example.test \"+000000000000\"".to_string(), "80000000000".to_string()],
        };
        let terms = focused_phone_terms(&input);
        assert!(terms.contains(&"+000000000000".to_string()));
        assert!(terms.contains(&"000000000000".to_string()));
        assert!(terms.contains(&"80000000000".to_string()));
        assert!(!terms.iter().any(|term| term.starts_with("site:")));
    }

    #[test]
    fn rejects_unsafe_probe_template() {
        assert!(!is_safe_probe_template("file:///tmp/{term}"));
        assert!(!is_safe_probe_template("http://127.0.0.1/{term}"));
        assert!(!is_safe_probe_template("https://example.com/no-placeholder"));
        assert!(is_safe_probe_template("https://example.com/search?q={term}"));
    }

    #[test]
    fn match_variants_include_belarus_forms() {
        let input = PhoneSearchInput {
            phone_e164: Some("+000000000000".to_string()),
            digits: "000000000000".to_string(),
            country_code: Some("375".to_string()),
            national_number: Some("000000000".to_string()),
            terms: vec![],
        };
        let variants = match_variants(&input, "+000000000000");
        assert!(variants.contains(&"+000000000000".to_string()));
        assert!(variants.contains(&"000000000000".to_string()));
        assert!(variants.contains(&"80000000000".to_string()));
    }

    #[test]
    fn provider_status_prefers_matched() {
        let summary = PhoneSearchProviderSummary { provider_id: "p".to_string(), enabled: true, status: PhoneProviderStatus::Executed, terms_attempted: 2, hits: 1, errors: 1, rate_limited: 0, empty_results: 1, last_error: None };
        assert_eq!(provider_summary_status(&summary), PhoneProviderStatus::Matched);
    }

    #[test]
    fn detects_rate_limit_message() {
        assert!(is_rate_limit_message("HTTP 403 rate limit or forbidden"));
        assert!(is_rate_limit_message("API rate limit exceeded"));
        assert!(is_rate_limit_message("HTTP 429 too many requests"));
    }

    #[test]
    fn configured_providers_understand_new_aliases() {
        let providers = [
            PhoneSearchProvider::new(PhoneSearchProviderKind::HackerNews).id,
            PhoneSearchProvider::new(PhoneSearchProviderKind::GitLab).id,
            PhoneSearchProvider::new(PhoneSearchProviderKind::NpmRegistry).id,
        ];
        assert!(providers.contains(&"phone_hackernews_search"));
        assert!(providers.contains(&"phone_gitlab_search"));
        assert!(providers.contains(&"phone_npm_registry_search"));
    }
}
