use crate::evidence::{build_evidence_observation, EvidenceInput};
use crate::models::{EntityNode, EntityType, EvidenceRecord, ObservationRecord, SensitivityClass, SourceClass};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailDomainFinding {
    pub source_id: String,
    pub entity_type: EntityType,
    pub value: String,
    pub confidence: u8,
    pub note: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DomainDnsSummary {
    pub domain: String,
    pub has_mx: bool,
    pub has_txt: bool,
    pub mx_hosts: Vec<String>,
    pub txt_records: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EmailDomainStats {
    pub emails_checked: usize,
    pub domains_checked: usize,
    pub valid_emails: usize,
    pub invalid_emails: usize,
    pub username_candidates: usize,
    pub dns_errors: usize,
    pub findings_count: usize,
    pub evidences_count: usize,
    pub observations_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EmailDomainReport {
    pub generated_at: u64,
    pub stats: EmailDomainStats,
    pub dns_summaries: Vec<DomainDnsSummary>,
    pub findings: Vec<EmailDomainFinding>,
    pub evidences: Vec<EvidenceRecord>,
    pub observations: Vec<ObservationRecord>,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub async fn run_email_domain_checkers(seeds: &[EntityNode]) -> EmailDomainReport {
    let dns_enabled = std::env::var("OSINT_DNS_CHECK")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes"))
        .unwrap_or(true);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .expect("build checker http client");

    let mut report = EmailDomainReport {
        generated_at: now_unix(),
        ..EmailDomainReport::default()
    };
    let mut domains_to_check = HashSet::new();
    let mut seen_findings = HashSet::new();

    for seed in seeds {
        match &seed.entity_type {
            EntityType::Email => {
                report.stats.emails_checked += 1;
                match split_email(&seed.value) {
                    Some((local, domain)) => {
                        report.stats.valid_emails += 1;
                        domains_to_check.insert(domain.clone());
                        push_finding(
                            &mut report.findings,
                            &mut seen_findings,
                            EmailDomainFinding {
                                source_id: "email_basic_checker".to_string(),
                                entity_type: EntityType::Email,
                                value: format!("{}@{}", local, domain),
                                confidence: 85,
                                note: "valid_email_format".to_string(),
                            },
                        );
                        push_finding(
                            &mut report.findings,
                            &mut seen_findings,
                            EmailDomainFinding {
                                source_id: "email_domain_checker".to_string(),
                                entity_type: EntityType::Domain,
                                value: domain.clone(),
                                confidence: 80,
                                note: "domain_from_email".to_string(),
                            },
                        );

                        let candidates = username_candidates_from_email_local(&local);
                        report.stats.username_candidates += candidates.len();
                        for candidate in candidates {
                            push_finding(
                                &mut report.findings,
                                &mut seen_findings,
                                EmailDomainFinding {
                                    source_id: "email_username_candidate_checker".to_string(),
                                    entity_type: EntityType::Username,
                                    value: candidate,
                                    confidence: 45,
                                    note: "username_candidate_from_email_local_part".to_string(),
                                },
                            );
                        }
                    }
                    None => report.stats.invalid_emails += 1,
                }
            }
            EntityType::Domain => {
                if let Some(domain) = normalize_domain(&seed.value) {
                    domains_to_check.insert(domain);
                }
            }
            EntityType::Url => {
                if let Some(domain) = domain_from_url(&seed.value) {
                    domains_to_check.insert(domain);
                }
            }
            _ => {}
        }
    }

    for domain in sorted_domains(domains_to_check) {
        report.stats.domains_checked += 1;
        push_finding(
            &mut report.findings,
            &mut seen_findings,
            EmailDomainFinding {
                source_id: "domain_basic_checker".to_string(),
                entity_type: EntityType::Domain,
                value: domain.clone(),
                confidence: 70,
                note: "domain_candidate".to_string(),
            },
        );

        if dns_enabled {
            match check_domain_dns(&client, &domain).await {
                Ok(summary) => {
                    if summary.has_mx {
                        push_finding(
                            &mut report.findings,
                            &mut seen_findings,
                            EmailDomainFinding {
                                source_id: "domain_dns_mx_checker".to_string(),
                                entity_type: EntityType::DataSource,
                                value: format!("mx:{}:{}", summary.domain, summary.mx_hosts.join(",")),
                                confidence: 70,
                                note: "domain_has_mx".to_string(),
                            },
                        );
                    }
                    if summary.has_txt {
                        push_finding(
                            &mut report.findings,
                            &mut seen_findings,
                            EmailDomainFinding {
                                source_id: "domain_dns_txt_checker".to_string(),
                                entity_type: EntityType::DataSource,
                                value: format!("txt:{}:{}", summary.domain, classify_txt_records(&summary.txt_records).join(",")),
                                confidence: 50,
                                note: "domain_has_txt".to_string(),
                            },
                        );
                    }
                    report.dns_summaries.push(summary);
                }
                Err(_) => report.stats.dns_errors += 1,
            }
        }
    }

    materialize_findings(&mut report);
    report.stats.findings_count = report.findings.len();
    report.stats.evidences_count = report.evidences.len();
    report.stats.observations_count = report.observations.len();
    report
}

pub fn save_email_domain_report(report: &EmailDomainReport, path: &str) -> Result<(), String> {
    let json = serde_json::to_string_pretty(report)
        .map_err(|err| format!("serialize email/domain report: {}", err))?;
    std::fs::write(path, json).map_err(|err| format!("write {}: {}", path, err))
}

pub fn observations_as_entity_nodes(report: &EmailDomainReport, limit: usize) -> Vec<EntityNode> {
    let mut nodes = Vec::new();
    let mut seen = HashSet::new();
    for obs in &report.observations {
        if nodes.len() >= limit {
            break;
        }
        if !matches!(obs.entity_type, EntityType::Email | EntityType::Username | EntityType::Domain | EntityType::Url) {
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
        let key = format!("{:?}:{}", obs.entity_type, normalize_item_value(&value, &obs.entity_type));
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

fn materialize_findings(report: &mut EmailDomainReport) {
    let findings = report.findings.clone();
    for finding in findings {
        let sensitivity = sensitivity_for(&finding.entity_type);
        let context = format!(
            "email_domain_checker source={} note={} value={}",
            finding.source_id,
            finding.note,
            finding.value
        );
        let pair = build_evidence_observation(EvidenceInput {
            source_id: finding.source_id,
            source_class: SourceClass::PublicOSINT,
            entity_type: finding.entity_type,
            raw_value: finding.value,
            raw_context: context,
            confidence: finding.confidence,
            sensitivity,
            tags: vec!["email_domain_checker".to_string(), finding.note],
        });
        report.evidences.push(pair.evidence);
        report.observations.push(pair.observation);
    }
}

async fn check_domain_dns(client: &Client, domain: &str) -> Result<DomainDnsSummary, reqwest::Error> {
    let mx = query_doh(client, domain, "MX").await?;
    let txt = query_doh(client, domain, "TXT").await?;

    let mx_hosts = dns_answer_data(&mx)
        .into_iter()
        .map(|data| strip_mx_priority(&data))
        .filter(|host| !host.is_empty())
        .collect::<Vec<_>>();
    let txt_records = dns_answer_data(&txt)
        .into_iter()
        .map(|s| s.trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();

    Ok(DomainDnsSummary {
        domain: domain.to_string(),
        has_mx: !mx_hosts.is_empty(),
        has_txt: !txt_records.is_empty(),
        mx_hosts,
        txt_records,
    })
}

async fn query_doh(client: &Client, domain: &str, record_type: &str) -> Result<Value, reqwest::Error> {
    let url = format!(
        "https://cloudflare-dns.com/dns-query?name={}&type={}",
        url_encode(domain),
        record_type
    );
    client
        .get(url)
        .header("Accept", "application/dns-json")
        .header("User-Agent", "XGEN-EmailDomainChecker/1.0 (+local research)")
        .send()
        .await?
        .json::<Value>()
        .await
}

fn dns_answer_data(value: &Value) -> Vec<String> {
    value
        .get("Answer")
        .and_then(|v| v.as_array())
        .map(|answers| {
            answers
                .iter()
                .filter_map(|answer| answer.get("data").and_then(|v| v.as_str()).map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn strip_mx_priority(value: &str) -> String {
    value
        .split_whitespace()
        .last()
        .unwrap_or_default()
        .trim_end_matches('.')
        .to_lowercase()
}

fn classify_txt_records(records: &[String]) -> Vec<String> {
    let mut labels = Vec::new();
    for record in records {
        let lower = record.to_lowercase();
        if lower.contains("v=spf1") {
            labels.push("spf".to_string());
        }
        if lower.contains("dmarc") || lower.contains("v=dmarc1") {
            labels.push("dmarc".to_string());
        }
        if lower.contains("google-site-verification") {
            labels.push("google-site-verification".to_string());
        }
        if lower.contains("ms=") || lower.contains("mscid") {
            labels.push("microsoft-verification".to_string());
        }
    }
    labels.sort();
    labels.dedup();
    if labels.is_empty() {
        labels.push("txt".to_string());
    }
    labels
}

fn push_finding(
    findings: &mut Vec<EmailDomainFinding>,
    seen: &mut HashSet<String>,
    finding: EmailDomainFinding,
) {
    let key = format!("{:?}:{}:{}", finding.entity_type, normalize_item_value(&finding.value, &finding.entity_type), finding.note);
    if seen.insert(key) {
        findings.push(finding);
    }
}

pub fn is_valid_email_format(email: &str) -> bool {
    split_email(email).is_some()
}

pub fn split_email(email: &str) -> Option<(String, String)> {
    let email = email.trim().to_lowercase();
    let parts: Vec<&str> = email.split('@').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return None;
    }
    let local = parts[0];
    let domain = parts[1];
    if local.len() > 64 || domain.len() > 253 {
        return None;
    }
    if local.starts_with('.') || local.ends_with('.') || local.contains("..") {
        return None;
    }
    if normalize_domain(domain).is_none() {
        return None;
    }
    if !local.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+')) {
        return None;
    }
    Some((local.to_string(), domain.to_string()))
}

pub fn username_candidates_from_email_local(local: &str) -> Vec<String> {
    let base = local.trim().to_lowercase();
    let before_plus = base.split('+').next().unwrap_or(&base).to_string();
    let mut raw = vec![base.clone(), before_plus.clone()];
    raw.push(before_plus.replace(['.', '-', '_'], ""));
    raw.extend(before_plus.split(['.', '-', '_']).filter(|part| part.len() >= 3).map(|s| s.to_string()));

    let mut candidates = raw
        .into_iter()
        .filter_map(|candidate| normalize_username(&candidate))
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.dedup();
    candidates.truncate(8);
    candidates
}

fn normalize_username(raw: &str) -> Option<String> {
    let username = raw.trim().trim_start_matches('@').trim();
    let lowered = username.to_lowercase();
    if username.len() < 3
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

fn normalize_domain(raw: &str) -> Option<String> {
    let domain = raw
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_matches('/')
        .to_lowercase();
    if domain.len() < 4 || domain.len() > 253 || !domain.contains('.') || domain.contains("..") {
        return None;
    }
    if domain
        .split('.')
        .all(|label| !label.is_empty() && !label.starts_with('-') && !label.ends_with('-') && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
    {
        Some(domain)
    } else {
        None
    }
}

fn domain_from_url(url: &str) -> Option<String> {
    let trimmed = url.trim();
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed);
    let host = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default();
    normalize_domain(host)
}

fn sorted_domains(domains: HashSet<String>) -> Vec<String> {
    let mut values = domains.into_iter().collect::<Vec<_>>();
    values.sort();
    values
}

fn normalize_item_value(value: &str, entity_type: &EntityType) -> String {
    match entity_type {
        EntityType::Email => value.trim().to_lowercase(),
        EntityType::Username | EntityType::Nickname => value.trim().trim_start_matches('@').to_lowercase(),
        EntityType::Domain | EntityType::Url => value.trim().to_lowercase(),
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

pub fn summarize_email_domains(report: &EmailDomainReport) -> HashMap<String, DomainDnsSummary> {
    report
        .dns_summaries
        .iter()
        .map(|summary| (summary.domain.clone(), summary.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_email_passes() {
        assert!(is_valid_email_format("test.user+tag@example.com"));
    }

    #[test]
    fn invalid_email_rejected() {
        assert!(!is_valid_email_format("bad@@example.com"));
        assert!(!is_valid_email_format("bad@example"));
        assert!(!is_valid_email_format(".bad@example.com"));
    }

    #[test]
    fn email_splits_into_local_and_domain() {
        let (local, domain) = split_email("Test.User@Example.COM").expect("valid email");
        assert_eq!(local, "test.user");
        assert_eq!(domain, "example.com");
    }

    #[test]
    fn username_candidates_are_generated() {
        let candidates = username_candidates_from_email_local("test.user-01+tag");
        assert!(candidates.contains(&"test.user-01".to_string()));
        assert!(candidates.contains(&"testuser01".to_string()));
    }

    #[test]
    fn domain_from_url_extracts_host() {
        assert_eq!(domain_from_url("https://sub.example.com/path?q=1"), Some("sub.example.com".to_string()));
    }

    #[tokio::test]
    async fn email_checker_builds_observations_without_dns() {
        std::env::set_var("OSINT_DNS_CHECK", "0");
        let seeds = vec![EntityNode {
            value: "test.user@example.com".to_string(),
            entity_type: EntityType::Email,
            first_seen: 0,
        }];
        let report = run_email_domain_checkers(&seeds).await;
        assert!(report.stats.valid_emails >= 1);
        assert!(report.observations.iter().any(|obs| obs.entity_type == EntityType::Email));
        assert!(report.observations.iter().any(|obs| obs.entity_type == EntityType::Domain));
        std::env::remove_var("OSINT_DNS_CHECK");
    }
}
