use crate::evidence::{build_evidence_observation, EvidenceInput};
use crate::models::{EntityNode, EntityType, EvidenceRecord, ObservationRecord, SensitivityClass, SourceClass};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PhoneNormalized {
    pub raw: String,
    pub digits: String,
    pub e164: Option<String>,
    pub country_guess: Option<String>,
    pub country_code: Option<String>,
    pub national_number: Option<String>,
    pub valid_shape: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhoneCarrierGuess {
    pub phone_e164: Option<String>,
    pub country: String,
    pub operator: Option<String>,
    pub number_type: String,
    pub confidence: u8,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhonePublicMention {
    pub source_id: String,
    pub url: Option<String>,
    pub value: String,
    pub context_snippet: String,
    pub confidence: u8,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhoneLinkedEntity {
    pub entity_type: EntityType,
    pub value: String,
    pub confidence: u8,
    pub source_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhoneIntelFinding {
    pub source_id: String,
    pub entity_type: EntityType,
    pub value: String,
    pub confidence: u8,
    pub note: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PhoneIntelStats {
    pub phones_checked: usize,
    pub valid_shape: usize,
    pub carrier_guesses: usize,
    pub search_terms_generated: usize,
    pub search_tasks_executed: usize,
    pub api_errors: usize,
    pub public_mentions: usize,
    pub linked_entities: usize,
    pub evidences_count: usize,
    pub observations_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PhoneIntelReport {
    pub generated_at: u64,
    pub input_count: usize,
    pub phones: Vec<PhoneNormalized>,
    pub carrier_guesses: Vec<PhoneCarrierGuess>,
    pub public_mentions: Vec<PhonePublicMention>,
    pub linked_entities: Vec<PhoneLinkedEntity>,
    pub search_terms: Vec<String>,
    pub findings: Vec<PhoneIntelFinding>,
    pub evidences: Vec<EvidenceRecord>,
    pub observations: Vec<ObservationRecord>,
    pub stats: PhoneIntelStats,
}

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

pub async fn run_phone_intel_for_seeds(seeds: &[EntityNode]) -> PhoneIntelReport {
    let mut report = PhoneIntelReport { generated_at: now_unix(), input_count: seeds.iter().filter(|seed| seed.entity_type == EntityType::Phone).count(), ..PhoneIntelReport::default() };
    let mut seen_phones = HashSet::new();
    let mut seen_terms = HashSet::new();
    let mut seen_findings = HashSet::new();
    let mut seen_mentions = HashSet::new();
    let client = Client::builder().timeout(std::time::Duration::from_secs(12)).build().expect("build phone search client");

    for seed in seeds.iter().filter(|seed| seed.entity_type == EntityType::Phone) {
        report.stats.phones_checked += 1;
        let normalized = normalize_phone(&seed.value);
        let phone_key = normalized.e164.clone().unwrap_or_else(|| normalized.digits.clone());
        if phone_key.is_empty() || !seen_phones.insert(phone_key.clone()) { continue; }
        if normalized.valid_shape { report.stats.valid_shape += 1; }

        push_finding(&mut report.findings, &mut seen_findings, PhoneIntelFinding {
            source_id: "phone_normalizer".to_string(),
            entity_type: EntityType::Phone,
            value: normalized.e164.clone().unwrap_or_else(|| normalized.digits.clone()),
            confidence: if normalized.valid_shape { 90 } else { 35 },
            note: "normalized_phone_shape".to_string(),
            reason: normalized.notes.join("; "),
        });

        if let Some(country) = normalized.country_guess.clone() {
            report.linked_entities.push(PhoneLinkedEntity { entity_type: EntityType::Country, value: country.clone(), confidence: if normalized.valid_shape { 80 } else { 45 }, source_id: "phone_country_prefix".to_string(), reason: format!("country inferred from phone country code {:?}", normalized.country_code) });
            push_finding(&mut report.findings, &mut seen_findings, PhoneIntelFinding { source_id: "phone_country_prefix".to_string(), entity_type: EntityType::Country, value: country, confidence: if normalized.valid_shape { 80 } else { 45 }, note: "country_guess_from_phone_prefix".to_string(), reason: "phone prefix maps to country guess".to_string() });
        }

        if let Some(guess) = guess_carrier(&normalized) {
            report.stats.carrier_guesses += 1;
            report.linked_entities.push(PhoneLinkedEntity {
                entity_type: EntityType::DataSource,
                value: format!("carrier_guess:{}:{}", normalized.e164.clone().unwrap_or_else(|| normalized.digits.clone()), guess.operator.clone().unwrap_or_else(|| "unknown".to_string())),
                confidence: guess.confidence,
                source_id: "phone_carrier_prefix_guess".to_string(),
                reason: guess.reason.clone(),
            });
            push_finding(&mut report.findings, &mut seen_findings, PhoneIntelFinding {
                source_id: "phone_carrier_prefix_guess".to_string(),
                entity_type: EntityType::DataSource,
                value: format!("carrier_guess:{}:{}:{}", guess.country, guess.operator.clone().unwrap_or_else(|| "unknown".to_string()), guess.number_type),
                confidence: guess.confidence,
                note: "operator_guess_not_owner_confirmation".to_string(),
                reason: guess.reason.clone(),
            });
            report.carrier_guesses.push(guess);
        }

        let phone_terms = build_phone_search_terms(&normalized);
        for term in &phone_terms {
            if seen_terms.insert(term.clone()) { report.search_terms.push(term.clone()); }
        }

        for mention in run_phone_public_search(&client, &normalized, &phone_terms, &mut report.stats).await {
            let key = format!("{}:{:?}:{}", mention.source_id, mention.url, mention.value);
            if seen_mentions.insert(key) {
                push_finding(&mut report.findings, &mut seen_findings, PhoneIntelFinding {
                    source_id: mention.source_id.clone(),
                    entity_type: EntityType::Phone,
                    value: mention.value.clone(),
                    confidence: mention.confidence,
                    note: mention.note.clone(),
                    reason: format!("public mention at {:?}: {}", mention.url, mention.context_snippet),
                });
                report.public_mentions.push(mention);
            }
        }

        report.phones.push(normalized);
    }

    report.stats.search_terms_generated = report.search_terms.len();
    report.stats.public_mentions = report.public_mentions.len();
    report.stats.linked_entities = report.linked_entities.len();
    materialize_findings(&mut report);
    report.stats.evidences_count = report.evidences.len();
    report.stats.observations_count = report.observations.len();
    report
}

async fn run_phone_public_search(client: &Client, phone: &PhoneNormalized, terms: &[String], stats: &mut PhoneIntelStats) -> Vec<PhonePublicMention> {
    let mut mentions = Vec::new();
    if !phone.valid_shape {
        return mentions;
    }
    let focused_terms = focused_phone_terms(phone, terms);
    for term in focused_terms.into_iter().take(4) {
        match search_github_code_for_phone(client, phone, &term).await {
            Ok(mut found) => { stats.search_tasks_executed += 1; mentions.append(&mut found); }
            Err(_) => { stats.search_tasks_executed += 1; stats.api_errors += 1; }
        }
        match search_github_issues_for_phone(client, phone, &term).await {
            Ok(mut found) => { stats.search_tasks_executed += 1; mentions.append(&mut found); }
            Err(_) => { stats.search_tasks_executed += 1; stats.api_errors += 1; }
        }
    }
    mentions
}

fn focused_phone_terms(phone: &PhoneNormalized, terms: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(e164) = &phone.e164 { out.push(e164.clone()); }
    out.push(phone.digits.clone());
    if phone.country_code.as_deref() == Some("375") {
        if let Some(national) = phone.national_number.as_deref() {
            if national.len() == 9 {
                out.push(format!("80{}", national));
            }
        }
    }
    for term in terms {
        if !term.starts_with("site:") && !term.contains(' ') && !term.contains('"') { out.push(term.clone()); }
    }
    out.retain(|term| !term.trim().is_empty());
    out.sort();
    out.dedup();
    out
}

async fn search_github_code_for_phone(client: &Client, phone: &PhoneNormalized, term: &str) -> Result<Vec<PhonePublicMention>, reqwest::Error> {
    let query = format!("{} in:file", term);
    let url = format!("https://api.github.com/search/code?q={}&per_page=5", url_encode(&query));
    let body = github_json(client, &url).await?;
    let mut mentions = Vec::new();
    if let Some(items) = body.get("items").and_then(|v| v.as_array()) {
        for item in items.iter().take(5) {
            let html_url = item.get("html_url").and_then(Value::as_str).map(|s| s.to_string());
            let repo = item.pointer("/repository/full_name").and_then(Value::as_str).unwrap_or("unknown_repo");
            let path = item.get("path").and_then(Value::as_str).unwrap_or("unknown_path");
            let snippet = format!("GitHub code search result: {}/{} matched term {}; exact line text requires opening source", repo, path, term);
            mentions.push(PhonePublicMention {
                source_id: "phone_github_code_search".to_string(),
                url: html_url,
                value: phone.e164.clone().unwrap_or_else(|| phone.digits.clone()),
                context_snippet: snippet,
                confidence: 65,
                note: "github_code_public_mention".to_string(),
            });
        }
    }
    Ok(mentions)
}

async fn search_github_issues_for_phone(client: &Client, phone: &PhoneNormalized, term: &str) -> Result<Vec<PhonePublicMention>, reqwest::Error> {
    let query = format!("{} in:title,body,comments", term);
    let url = format!("https://api.github.com/search/issues?q={}&per_page=5", url_encode(&query));
    let body = github_json(client, &url).await?;
    let mut mentions = Vec::new();
    if let Some(items) = body.get("items").and_then(|v| v.as_array()) {
        for item in items.iter().take(5) {
            let html_url = item.get("html_url").and_then(Value::as_str).map(|s| s.to_string());
            let title = item.get("title").and_then(Value::as_str).unwrap_or("untitled");
            let snippet = format!("GitHub issue/discussion search result: {} matched term {}", title, term);
            mentions.push(PhonePublicMention {
                source_id: "phone_github_issue_search".to_string(),
                url: html_url,
                value: phone.e164.clone().unwrap_or_else(|| phone.digits.clone()),
                context_snippet: snippet,
                confidence: 60,
                note: "github_issue_public_mention".to_string(),
            });
        }
    }
    Ok(mentions)
}

async fn github_json(client: &Client, url: &str) -> Result<Value, reqwest::Error> {
    client
        .get(url)
        .header("User-Agent", "XGEN-PhoneIntel/1.0 (+local self-audit)")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .json::<Value>()
        .await
}

pub fn save_phone_intel_report(report: &PhoneIntelReport, path: &str) -> Result<(), String> {
    let json = serde_json::to_string_pretty(report).map_err(|err| format!("serialize phone intel report: {}", err))?;
    std::fs::write(path, json).map_err(|err| format!("write {}: {}", path, err))
}

pub fn observations_as_entity_nodes(report: &PhoneIntelReport, limit: usize) -> Vec<EntityNode> {
    let mut nodes = Vec::new();
    let mut seen = HashSet::new();
    for obs in &report.observations {
        if nodes.len() >= limit { break; }
        if !matches!(obs.entity_type, EntityType::Phone | EntityType::Country | EntityType::DataSource | EntityType::Url) { continue; }
        let value = if obs.normalized_value.is_empty() { obs.value_masked.clone() } else { obs.normalized_value.clone() };
        if value.is_empty() || value.contains("[redacted]") { continue; }
        let key = format!("{:?}:{}", obs.entity_type, value);
        if seen.insert(key) {
            nodes.push(EntityNode { value, entity_type: obs.entity_type.clone(), first_seen: obs.seen_at });
        }
    }
    nodes
}

pub fn normalize_phone(raw: &str) -> PhoneNormalized {
    let digits: String = raw.trim().chars().filter(|c| c.is_ascii_digit()).collect();
    let mut result = PhoneNormalized { raw: raw.to_string(), digits: digits.clone(), ..PhoneNormalized::default() };

    if digits.is_empty() {
        result.notes.push("no digits found".to_string());
        return result;
    }

    if let Some(national) = digits.strip_prefix("375") {
        result.country_guess = Some("Беларусь".to_string());
        result.country_code = Some("375".to_string());
        result.national_number = Some(national.to_string());
        result.e164 = Some(format!("+375{}", national));
        result.valid_shape = national.len() == 9;
        result.notes.push("Belarus +375 shape".to_string());
        return result;
    }

    if digits.starts_with("80") && digits.len() == 11 {
        let national = digits[2..].to_string();
        result.country_guess = Some("Беларусь".to_string());
        result.country_code = Some("375".to_string());
        result.national_number = Some(national.clone());
        result.e164 = Some(format!("+375{}", national));
        result.valid_shape = national.len() == 9;
        result.notes.push("Belarus trunk 80 converted to +375".to_string());
        return result;
    }

    if let Some(national) = digits.strip_prefix("48") {
        result.country_guess = Some("Польша".to_string());
        result.country_code = Some("48".to_string());
        result.national_number = Some(national.to_string());
        result.e164 = Some(format!("+48{}", national));
        result.valid_shape = national.len() == 9;
        result.notes.push("Poland +48 shape".to_string());
        return result;
    }

    if let Some(national) = digits.strip_prefix("380") {
        result.country_guess = Some("Украина".to_string());
        result.country_code = Some("380".to_string());
        result.national_number = Some(national.to_string());
        result.e164 = Some(format!("+380{}", national));
        result.valid_shape = national.len() == 9;
        result.notes.push("Ukraine +380 shape".to_string());
        return result;
    }

    if let Some(national) = digits.strip_prefix('7') {
        result.country_guess = Some("Россия/Казахстан".to_string());
        result.country_code = Some("7".to_string());
        result.national_number = Some(national.to_string());
        result.e164 = Some(format!("+7{}", national));
        result.valid_shape = national.len() == 10;
        result.notes.push("+7 shape, country requires additional context".to_string());
        return result;
    }

    result.valid_shape = false;
    result.notes.push("unsupported or ambiguous country prefix".to_string());
    result
}

pub fn guess_carrier(phone: &PhoneNormalized) -> Option<PhoneCarrierGuess> {
    let country_code = phone.country_code.as_deref()?;
    let national = phone.national_number.as_deref().unwrap_or_default();
    match country_code {
        "375" => guess_belarus_carrier(phone, national),
        "48" => Some(PhoneCarrierGuess { phone_e164: phone.e164.clone(), country: "Польша".to_string(), operator: None, number_type: "mobile_or_fixed_requires_external_lookup".to_string(), confidence: 35, reason: "Poland operator cannot be reliably inferred from prefix without external numbering/MNP data".to_string() }),
        "7" => Some(PhoneCarrierGuess { phone_e164: phone.e164.clone(), country: "Россия/Казахстан".to_string(), operator: None, number_type: "requires_external_lookup".to_string(), confidence: 25, reason: "+7 numbers require external numbering plan and MNP data".to_string() }),
        "380" => Some(PhoneCarrierGuess { phone_e164: phone.e164.clone(), country: "Украина".to_string(), operator: None, number_type: "requires_external_lookup".to_string(), confidence: 25, reason: "Ukraine operator requires external numbering/MNP data".to_string() }),
        _ => None,
    }
}

fn guess_belarus_carrier(phone: &PhoneNormalized, national: &str) -> Option<PhoneCarrierGuess> {
    if national.len() < 2 { return None; }
    let prefix = &national[..2];
    let (operator, number_type, confidence, reason) = match prefix {
        "25" => (Some("life:)".to_string()), "mobile", 60, "prefix 25 historically belongs to life:); MNP may change current operator".to_string()),
        "29" => (Some("A1 / MTS".to_string()), "mobile", 45, "prefix 29 is shared historically; subprefix/MNP lookup required".to_string()),
        "33" => (Some("MTS".to_string()), "mobile", 60, "prefix 33 historically belongs to MTS; MNP may change current operator".to_string()),
        "44" => (Some("A1".to_string()), "mobile", 60, "prefix 44 historically belongs to A1; MNP may change current operator".to_string()),
        "17" => (Some("fixed Minsk area".to_string()), "fixed_line", 55, "prefix 17 suggests Minsk fixed-line/area numbering".to_string()),
        _ => (None, "unknown", 20, format!("Belarus prefix {} is not in the local static map", prefix)),
    };
    Some(PhoneCarrierGuess { phone_e164: phone.e164.clone(), country: "Беларусь".to_string(), operator, number_type: number_type.to_string(), confidence, reason })
}

pub fn build_phone_search_terms(phone: &PhoneNormalized) -> Vec<String> {
    let mut terms = Vec::new();
    let digits = phone.digits.trim();
    if digits.is_empty() { return terms; }
    if let Some(e164) = &phone.e164 {
        terms.push(e164.clone());
        terms.push(format!("\"{}\"", e164));
    }
    terms.push(digits.to_string());
    terms.push(format!("\"{}\"", digits));

    if phone.country_code.as_deref() == Some("375") {
        if let Some(national) = phone.national_number.as_deref() {
            if national.len() == 9 {
                let operator = &national[..2];
                let part1 = &national[2..5];
                let part2 = &national[5..7];
                let part3 = &national[7..9];
                terms.push(format!("80{}{}{}{}", operator, part1, part2, part3));
                terms.push(format!("{} {} {} {}", operator, part1, part2, part3));
                terms.push(format!("+375 {} {} {} {}", operator, part1, part2, part3));
            }
        }
    }

    if let Some(e164) = &phone.e164 {
        for site in ["t.me", "vk.com", "github.com", "pastebin.com", "steamcommunity.com", "worldoftanks.eu", "wargaming.net"] {
            terms.push(format!("site:{} \"{}\"", site, e164));
        }
    }
    terms.retain(|term| !term.trim().is_empty());
    terms.sort();
    terms.dedup();
    terms
}

fn push_finding(findings: &mut Vec<PhoneIntelFinding>, seen: &mut HashSet<String>, finding: PhoneIntelFinding) {
    let key = format!("{:?}:{}:{}", finding.entity_type, finding.value, finding.note);
    if seen.insert(key) { findings.push(finding); }
}

fn materialize_findings(report: &mut PhoneIntelReport) {
    let findings = report.findings.clone();
    for finding in findings {
        let sensitivity = match finding.entity_type { EntityType::Phone => SensitivityClass::Personal, _ => SensitivityClass::PublicLow };
        let context = format!("phone_intel source={} note={} reason={} value={}", finding.source_id, finding.note, finding.reason, finding.value);
        let pair = build_evidence_observation(EvidenceInput { source_id: finding.source_id, source_class: SourceClass::PublicOSINT, entity_type: finding.entity_type, raw_value: finding.value, raw_context: context, confidence: finding.confidence, sensitivity, tags: vec!["phone_intel".to_string(), finding.note] });
        report.evidences.push(pair.evidence);
        report.observations.push(pair.observation);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_belarus_e164_phone() {
        let phone = normalize_phone("+375 25 799 76 76");
        assert_eq!(phone.digits, "375257997676");
        assert_eq!(phone.e164.as_deref(), Some("+375257997676"));
        assert_eq!(phone.country_guess.as_deref(), Some("Беларусь"));
        assert!(phone.valid_shape);
    }

    #[test]
    fn normalizes_belarus_80_phone() {
        let phone = normalize_phone("80 25 799 76 76");
        assert_eq!(phone.e164.as_deref(), Some("+375257997676"));
        assert_eq!(phone.national_number.as_deref(), Some("257997676"));
        assert_eq!(phone.country_code.as_deref(), Some("375"));
        assert!(phone.valid_shape);
    }

    #[test]
    fn guesses_belarus_life_prefix_25() {
        let phone = normalize_phone("+375257997676");
        let guess = guess_carrier(&phone).expect("carrier guess");
        assert_eq!(guess.operator.as_deref(), Some("life:)"));
        assert!(guess.reason.contains("MNP"));
    }

    #[test]
    fn builds_phone_search_terms() {
        let phone = normalize_phone("+375257997676");
        let terms = build_phone_search_terms(&phone);
        assert!(terms.contains(&"+375257997676".to_string()));
        assert!(terms.contains(&"\"+375257997676\"".to_string()));
        assert!(terms.contains(&"80257997676".to_string()));
        assert!(terms.iter().any(|term| term.starts_with("site:t.me")));
    }

    #[test]
    fn phone_intel_report_materializes_evidence() {
        let seed = EntityNode { value: "+375257997676".to_string(), entity_type: EntityType::Phone, first_seen: 0 };
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let report = rt.block_on(run_phone_intel_for_seeds(&[seed]));
        assert_eq!(report.stats.phones_checked, 1);
        assert!(report.stats.evidences_count > 0);
        assert!(report.stats.observations_count > 0);
    }

    #[test]
    fn focused_terms_include_e164_and_80_variant() {
        let phone = normalize_phone("+375257997676");
        let terms = focused_phone_terms(&phone, &build_phone_search_terms(&phone));
        assert!(terms.contains(&"+375257997676".to_string()));
        assert!(terms.contains(&"80257997676".to_string()));
    }
}
