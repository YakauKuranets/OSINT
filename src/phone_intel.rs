use crate::evidence::{build_evidence_observation, EvidenceInput};
use crate::models::{EntityNode, EntityType, EvidenceRecord, ObservationRecord, SensitivityClass, SourceClass};
use crate::phone_search::{self, PhoneSearchInput, PhoneSearchProviderSummary, PhoneSearchProviderTrace};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
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
pub struct PhoneExtractedSignal {
    pub phone_value: String,
    pub entity_type: EntityType,
    pub value: String,
    pub source_id: String,
    pub url: Option<String>,
    pub confidence: u8,
    pub reason: String,
    pub context_snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhoneSourceYear {
    pub phone_value: String,
    pub source_id: String,
    pub url: Option<String>,
    pub year: u32,
    pub confidence: u8,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhoneCorrelationLevel {
    Weak,
    Possible,
    Probable,
    Strong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhoneCorrelationSummary {
    pub phone_value: String,
    pub entity_type: EntityType,
    pub value: String,
    pub level: PhoneCorrelationLevel,
    pub confidence: u8,
    pub mentions: usize,
    pub independent_sources: usize,
    pub urls_count: usize,
    pub year_hints: Vec<u32>,
    pub source_ids: Vec<String>,
    pub urls: Vec<String>,
    pub reasons: Vec<String>,
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
    pub extracted_signals: usize,
    pub correlation_summaries: usize,
    pub strong_correlations: usize,
    pub probable_correlations: usize,
    pub source_years: usize,
    pub providers_enabled: usize,
    pub providers_with_hits: usize,
    pub provider_traces: usize,
    pub evidences_count: usize,
    pub observations_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PhoneIntelReport {
    pub generated_at: u64,
    pub input_count: usize,
    pub phones: Vec<PhoneNormalized>,
    pub carrier_guesses: Vec<PhoneCarrierGuess>,
    pub provider_summaries: Vec<PhoneSearchProviderSummary>,
    pub traces: Vec<PhoneSearchProviderTrace>,
    pub public_mentions: Vec<PhonePublicMention>,
    pub extracted_signals: Vec<PhoneExtractedSignal>,
    pub correlation_summaries: Vec<PhoneCorrelationSummary>,
    pub source_years: Vec<PhoneSourceYear>,
    pub linked_entities: Vec<PhoneLinkedEntity>,
    pub search_terms: Vec<String>,
    pub findings: Vec<PhoneIntelFinding>,
    pub evidences: Vec<EvidenceRecord>,
    pub observations: Vec<ObservationRecord>,
    pub stats: PhoneIntelStats,
}

#[derive(Debug, Default)]
struct CorrelationBucket {
    phone_value: String,
    entity_type: Option<EntityType>,
    value: String,
    max_signal_confidence: u8,
    mentions: usize,
    source_ids: HashSet<String>,
    urls: HashSet<String>,
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
    let mut seen_signals = HashSet::new();
    let mut seen_years = HashSet::new();
    let mut seen_provider_summary = HashSet::new();
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

        let provider_report = run_phone_public_search(&client, &normalized, &phone_terms).await;
        report.traces.extend(provider_report.traces);
        for summary in provider_report.providers {
            report.stats.search_tasks_executed += summary.terms_attempted;
            report.stats.api_errors += summary.errors;
            if seen_provider_summary.insert(summary.provider_id.clone()) {
                report.provider_summaries.push(summary);
            } else if let Some(existing) = report.provider_summaries.iter_mut().find(|item| item.provider_id == summary.provider_id) {
                existing.terms_attempted += summary.terms_attempted;
                existing.hits += summary.hits;
                existing.errors += summary.errors;
                existing.rate_limited += summary.rate_limited;
                existing.empty_results += summary.empty_results;
                if summary.hits > 0 { existing.status = summary.status; }
                existing.last_error = summary.last_error.or_else(|| existing.last_error.clone());
            }
        }
        for hit in provider_report.hits {
            let mention = PhonePublicMention {
                source_id: hit.provider_id,
                url: hit.url,
                value: hit.matched_value,
                context_snippet: hit.context_snippet,
                confidence: hit.confidence,
                note: hit.note,
            };
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
                let signals = extract_signals_from_mention(&mention);
                for signal in signals {
                    let signal_key = format!("{:?}:{}:{}:{:?}", signal.entity_type, signal.value, signal.source_id, signal.url);
                    if seen_signals.insert(signal_key) {
                        report.linked_entities.push(PhoneLinkedEntity {
                            entity_type: signal.entity_type.clone(),
                            value: signal.value.clone(),
                            confidence: signal.confidence,
                            source_id: signal.source_id.clone(),
                            reason: signal.reason.clone(),
                        });
                        push_finding(&mut report.findings, &mut seen_findings, PhoneIntelFinding {
                            source_id: signal.source_id.clone(),
                            entity_type: signal.entity_type.clone(),
                            value: signal.value.clone(),
                            confidence: signal.confidence,
                            note: "phone_context_extracted_signal".to_string(),
                            reason: signal.reason.clone(),
                        });
                        report.extracted_signals.push(signal);
                    }
                }
                for source_year in extract_years_from_mention(&mention) {
                    let year_key = format!("{}:{}:{:?}", source_year.source_id, source_year.year, source_year.url);
                    if seen_years.insert(year_key) {
                        push_finding(&mut report.findings, &mut seen_findings, PhoneIntelFinding {
                            source_id: source_year.source_id.clone(),
                            entity_type: EntityType::DataSource,
                            value: format!("phone_seen_year:{}", source_year.year),
                            confidence: source_year.confidence,
                            note: "phone_source_year_hint".to_string(),
                            reason: source_year.reason.clone(),
                        });
                        report.source_years.push(source_year);
                    }
                }
                report.public_mentions.push(mention);
            }
        }

        report.phones.push(normalized);
    }

    let correlation_summaries = aggregate_phone_correlations(&report.extracted_signals, &report.source_years);
    for summary in &correlation_summaries {
        report.linked_entities.push(PhoneLinkedEntity {
            entity_type: summary.entity_type.clone(),
            value: summary.value.clone(),
            confidence: summary.confidence,
            source_id: "phone_correlation_aggregator".to_string(),
            reason: summary.reasons.join("; "),
        });
        push_finding(&mut report.findings, &mut seen_findings, PhoneIntelFinding {
            source_id: "phone_correlation_aggregator".to_string(),
            entity_type: summary.entity_type.clone(),
            value: summary.value.clone(),
            confidence: summary.confidence,
            note: format!("phone_correlation_{:?}", summary.level).to_lowercase(),
            reason: summary.reasons.join("; "),
        });
    }
    report.correlation_summaries = correlation_summaries;

    report.stats.search_terms_generated = report.search_terms.len();
    report.stats.public_mentions = report.public_mentions.len();
    report.stats.extracted_signals = report.extracted_signals.len();
    report.stats.correlation_summaries = report.correlation_summaries.len();
    report.stats.strong_correlations = report.correlation_summaries.iter().filter(|s| s.level == PhoneCorrelationLevel::Strong).count();
    report.stats.probable_correlations = report.correlation_summaries.iter().filter(|s| s.level == PhoneCorrelationLevel::Probable).count();
    report.stats.source_years = report.source_years.len();
    report.stats.linked_entities = report.linked_entities.len();
    report.stats.providers_enabled = report.provider_summaries.iter().filter(|p| p.enabled).count();
    report.stats.providers_with_hits = report.provider_summaries.iter().filter(|p| p.hits > 0).count();
    report.stats.provider_traces = report.traces.len();
    materialize_findings(&mut report);
    report.stats.evidences_count = report.evidences.len();
    report.stats.observations_count = report.observations.len();
    report
}

async fn run_phone_public_search(client: &Client, phone: &PhoneNormalized, terms: &[String]) -> phone_search::PhoneSearchProviderReport {
    if !phone.valid_shape {
        return phone_search::PhoneSearchProviderReport::default();
    }
    let input = PhoneSearchInput {
        phone_e164: phone.e164.clone(),
        digits: phone.digits.clone(),
        country_code: phone.country_code.clone(),
        national_number: phone.national_number.clone(),
        terms: terms.to_vec(),
    };
    phone_search::run_phone_search_providers(client, &input).await
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
        if !matches!(obs.entity_type, EntityType::Phone | EntityType::Country | EntityType::DataSource | EntityType::Url | EntityType::Email | EntityType::Username | EntityType::Nickname | EntityType::SocialProfile) { continue; }
        let value = if obs.normalized_value.is_empty() { obs.value_masked.clone() } else { obs.normalized_value.clone() };
        if value.is_empty() || value.contains("[redacted]") { continue; }
        let key = format!("{:?}:{}", obs.entity_type, value);
        if seen.insert(key) { nodes.push(EntityNode { value, entity_type: obs.entity_type.clone(), first_seen: obs.seen_at }); }
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

fn aggregate_phone_correlations(signals: &[PhoneExtractedSignal], years: &[PhoneSourceYear]) -> Vec<PhoneCorrelationSummary> {
    let mut buckets: HashMap<String, CorrelationBucket> = HashMap::new();
    for signal in signals {
        let key = format!("{}::{:?}::{}", signal.phone_value, signal.entity_type, signal.value.to_lowercase());
        let bucket = buckets.entry(key).or_insert_with(|| CorrelationBucket {
            phone_value: signal.phone_value.clone(),
            entity_type: Some(signal.entity_type.clone()),
            value: signal.value.clone(),
            ..CorrelationBucket::default()
        });
        bucket.mentions += 1;
        bucket.max_signal_confidence = bucket.max_signal_confidence.max(signal.confidence);
        bucket.source_ids.insert(signal.source_id.clone());
        if let Some(url) = &signal.url { bucket.urls.insert(url.clone()); }
    }

    let mut summaries = Vec::new();
    for bucket in buckets.into_values() {
        let entity_type = bucket.entity_type.unwrap_or(EntityType::DataSource);
        let year_hints = years_for_phone(years, &bucket.phone_value);
        let independent_sources = bucket.source_ids.len();
        let urls_count = bucket.urls.len();
        let confidence = correlation_confidence(bucket.max_signal_confidence, bucket.mentions, independent_sources, urls_count, year_hints.len());
        let level = correlation_level(confidence, bucket.mentions, independent_sources);
        let mut source_ids = bucket.source_ids.into_iter().collect::<Vec<_>>();
        let mut urls = bucket.urls.into_iter().collect::<Vec<_>>();
        source_ids.sort();
        urls.sort();
        let reasons = correlation_reasons(&level, confidence, bucket.mentions, independent_sources, urls_count, &year_hints);
        summaries.push(PhoneCorrelationSummary {
            phone_value: bucket.phone_value,
            entity_type,
            value: bucket.value,
            level,
            confidence,
            mentions: bucket.mentions,
            independent_sources,
            urls_count,
            year_hints,
            source_ids,
            urls,
            reasons,
        });
    }
    summaries.sort_by(|a, b| b.confidence.cmp(&a.confidence).then_with(|| b.mentions.cmp(&a.mentions)));
    summaries
}

fn years_for_phone(years: &[PhoneSourceYear], phone_value: &str) -> Vec<u32> {
    let mut out = years.iter().filter(|year| year.phone_value == phone_value).map(|year| year.year).collect::<Vec<_>>();
    out.sort();
    out.dedup();
    out
}

fn correlation_confidence(base: u8, mentions: usize, independent_sources: usize, urls_count: usize, year_count: usize) -> u8 {
    let mut score = base as usize;
    score += mentions.saturating_sub(1).min(6) * 4;
    score += independent_sources.saturating_sub(1).min(4) * 9;
    score += urls_count.min(4) * 3;
    score += year_count.min(3) * 2;
    score.min(92) as u8
}

fn correlation_level(confidence: u8, mentions: usize, independent_sources: usize) -> PhoneCorrelationLevel {
    if confidence >= 82 && independent_sources >= 2 && mentions >= 2 { PhoneCorrelationLevel::Strong }
    else if confidence >= 68 && (independent_sources >= 2 || mentions >= 2) { PhoneCorrelationLevel::Probable }
    else if confidence >= 50 { PhoneCorrelationLevel::Possible }
    else { PhoneCorrelationLevel::Weak }
}

fn correlation_reasons(level: &PhoneCorrelationLevel, confidence: u8, mentions: usize, independent_sources: usize, urls_count: usize, year_hints: &[u32]) -> Vec<String> {
    let mut reasons = Vec::new();
    reasons.push(format!("aggregated phone-context signal level={:?} confidence={}", level, confidence));
    reasons.push(format!("mentions={} independent_sources={} urls_count={}", mentions, independent_sources, urls_count));
    if !year_hints.is_empty() {
        reasons.push(format!("year hints present: {:?}; these are source/date hints, not guaranteed first_seen", year_hints));
    }
    if independent_sources < 2 {
        reasons.push("single-source correlation; do not treat as identity confirmation".to_string());
    }
    reasons
}

fn extract_signals_from_mention(mention: &PhonePublicMention) -> Vec<PhoneExtractedSignal> {
    let mut signals = Vec::new();
    let mut seen = HashSet::new();
    let context = format!("{} {}", mention.url.clone().unwrap_or_default(), mention.context_snippet);
    for email in extract_emails(&context) {
        push_signal(&mut signals, &mut seen, mention, EntityType::Email, email, 62, "email found in same public phone mention context");
    }
    for url in extract_urls(&context) {
        push_signal(&mut signals, &mut seen, mention, EntityType::Url, url, 58, "url found in same public phone mention context");
    }
    for username in extract_usernames(&context) {
        push_signal(&mut signals, &mut seen, mention, EntityType::Username, username, 52, "username-like token found in same public phone mention context");
    }
    signals
}

fn push_signal(signals: &mut Vec<PhoneExtractedSignal>, seen: &mut HashSet<String>, mention: &PhonePublicMention, entity_type: EntityType, value: String, confidence: u8, reason: &str) {
    let key = format!("{:?}:{}", entity_type, value.to_lowercase());
    if value.trim().is_empty() || !seen.insert(key) { return; }
    signals.push(PhoneExtractedSignal {
        phone_value: mention.value.clone(),
        entity_type,
        value,
        source_id: mention.source_id.clone(),
        url: mention.url.clone(),
        confidence,
        reason: reason.to_string(),
        context_snippet: mention.context_snippet.clone(),
    });
}

fn extract_years_from_mention(mention: &PhonePublicMention) -> Vec<PhoneSourceYear> {
    let context = format!("{} {}", mention.url.clone().unwrap_or_default(), mention.context_snippet);
    extract_years(&context).into_iter().map(|year| PhoneSourceYear {
        phone_value: mention.value.clone(),
        source_id: mention.source_id.clone(),
        url: mention.url.clone(),
        year,
        confidence: 45,
        reason: format!("year-like value {} found in public mention context; treat as source/date hint, not guaranteed first seen", year),
    }).collect()
}

fn extract_emails(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in text.split_whitespace() {
        let clean = clean_token(token);
        if clean.contains('@') && clean.contains('.') && clean.len() <= 254 && !clean.starts_with('@') && !clean.ends_with('@') {
            out.push(clean.to_lowercase());
        }
    }
    out.sort();
    out.dedup();
    out
}

fn extract_urls(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in text.split_whitespace() {
        let clean = clean_token(token);
        let lower = clean.to_lowercase();
        if (lower.starts_with("http://") || lower.starts_with("https://")) && clean.len() <= 512 {
            out.push(clean);
        }
    }
    out.sort();
    out.dedup();
    out
}

fn extract_usernames(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in text.split_whitespace() {
        let clean = clean_token(token);
        if clean.starts_with('@') && clean.len() >= 4 && clean.len() <= 33 {
            let body = &clean[1..];
            if body.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && body.chars().any(|c| c.is_ascii_alphabetic()) {
                out.push(clean.to_lowercase());
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn extract_years(text: &str) -> Vec<u32> {
    let mut years = Vec::new();
    let mut buf = String::new();
    for ch in text.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_digit() {
            buf.push(ch);
        } else {
            if buf.len() == 4 {
                if let Ok(year) = buf.parse::<u32>() {
                    if (1990..=2035).contains(&year) { years.push(year); }
                }
            }
            buf.clear();
        }
    }
    years.sort();
    years.dedup();
    years
}

fn clean_token(token: &str) -> String {
    token.trim_matches(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | ':' | ')' | '(' | '[' | ']' | '{' | '}' | '<' | '>' | '"' | '\'' | '`')).to_string()
}

fn push_finding(findings: &mut Vec<PhoneIntelFinding>, seen: &mut HashSet<String>, finding: PhoneIntelFinding) {
    let key = format!("{:?}:{}:{}", finding.entity_type, finding.value, finding.note);
    if seen.insert(key) { findings.push(finding); }
}

fn materialize_findings(report: &mut PhoneIntelReport) {
    let findings = report.findings.clone();
    for finding in findings {
        let sensitivity = match finding.entity_type { EntityType::Phone | EntityType::Email | EntityType::Username | EntityType::Nickname => SensitivityClass::Personal, _ => SensitivityClass::PublicLow };
        let context = format!("phone_intel source={} note={} reason={} value={}", finding.source_id, finding.note, finding.reason, finding.value);
        let pair = build_evidence_observation(EvidenceInput { source_id: finding.source_id, source_class: SourceClass::PublicOSINT, entity_type: finding.entity_type, raw_value: finding.value, raw_context: context, confidence: finding.confidence, sensitivity, tags: vec!["phone_intel".to_string(), finding.note] });
        report.evidences.push(pair.evidence);
        report.observations.push(pair.observation);
    }
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
    fn extracts_context_signals() {
        let mention = PhonePublicMention { source_id: "test".to_string(), url: Some("https://example.com/u/@tester".to_string()), value: "+375257997676".to_string(), context_snippet: "contact test@example.com @tester year 2021".to_string(), confidence: 70, note: "test".to_string() };
        let signals = extract_signals_from_mention(&mention);
        assert!(signals.iter().any(|s| s.entity_type == EntityType::Email && s.value == "test@example.com"));
        assert!(signals.iter().any(|s| s.entity_type == EntityType::Username && s.value == "@tester"));
        assert_eq!(extract_years_from_mention(&mention)[0].year, 2021);
    }
    #[test]
    fn aggregates_repeated_phone_signals() {
        let signals = vec![
            PhoneExtractedSignal { phone_value: "+375257997676".to_string(), entity_type: EntityType::Email, value: "test@example.com".to_string(), source_id: "s1".to_string(), url: Some("https://a.example".to_string()), confidence: 62, reason: "r".to_string(), context_snippet: "c".to_string() },
            PhoneExtractedSignal { phone_value: "+375257997676".to_string(), entity_type: EntityType::Email, value: "test@example.com".to_string(), source_id: "s2".to_string(), url: Some("https://b.example".to_string()), confidence: 62, reason: "r".to_string(), context_snippet: "c".to_string() },
        ];
        let years = vec![PhoneSourceYear { phone_value: "+375257997676".to_string(), source_id: "s1".to_string(), url: None, year: 2021, confidence: 45, reason: "year".to_string() }];
        let summaries = aggregate_phone_correlations(&signals, &years);
        assert_eq!(summaries.len(), 1);
        assert!(summaries[0].confidence >= 68);
        assert_eq!(summaries[0].independent_sources, 2);
    }
}
