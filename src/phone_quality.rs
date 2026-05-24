use crate::phone_intel::{PhoneCorrelationSummary, PhoneExtractedSignal, PhonePublicMention, PhoneSourceYear};
use crate::phone_search::{PhoneProviderStatus, PhoneSearchProviderSummary, PhoneSearchProviderTrace};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhoneSourceQualityLevel {
    Low,
    Medium,
    High,
    VeryHigh,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhoneSourceQualityScore {
    pub phone_value: String,
    pub source_id: String,
    pub url: Option<String>,
    pub quality_level: PhoneSourceQualityLevel,
    pub quality_score: u8,
    pub exact_phone_match: bool,
    pub context_chars: usize,
    pub linked_signal_count: usize,
    pub correlation_count: usize,
    pub year_hints: Vec<u32>,
    pub provider_status: Option<String>,
    pub provider_hits: usize,
    pub provider_errors: usize,
    pub provider_rate_limited: usize,
    pub risk_flags: Vec<String>,
    pub strengths: Vec<String>,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PhoneSourceQualityReport {
    pub scores: Vec<PhoneSourceQualityScore>,
    pub high_or_better: usize,
    pub medium_or_better: usize,
    pub low_quality: usize,
    pub risky_sources: usize,
}

pub fn score_phone_sources(
    mentions: &[PhonePublicMention],
    signals: &[PhoneExtractedSignal],
    correlations: &[PhoneCorrelationSummary],
    years: &[PhoneSourceYear],
    provider_summaries: &[PhoneSearchProviderSummary],
    traces: &[PhoneSearchProviderTrace],
) -> PhoneSourceQualityReport {
    let provider_index = provider_summaries
        .iter()
        .map(|p| (p.provider_id.clone(), p))
        .collect::<HashMap<_, _>>();
    let mut scores = Vec::new();
    let mut seen = HashSet::new();

    for mention in mentions {
        let key = format!("{}::{:?}::{}", mention.source_id, mention.url, mention.value);
        if !seen.insert(key) { continue; }

        let linked_signal_count = signals
            .iter()
            .filter(|s| s.phone_value == mention.value && s.source_id == mention.source_id && s.url == mention.url)
            .count();
        let correlation_count = correlations
            .iter()
            .filter(|c| c.phone_value == mention.value && c.source_ids.iter().any(|sid| sid == &mention.source_id))
            .count();
        let year_hints = years_for_mention(years, mention);
        let provider = provider_index.get(&mention.source_id).copied();
        let provider_trace = traces.iter().find(|t| t.provider_id == mention.source_id && t.hits > 0 && t.url == mention.url);
        let exact_phone_match = mention.context_snippet.contains(&mention.value) || mention.url.as_deref().unwrap_or_default().contains(&mention.value);
        let context_chars = mention.context_snippet.chars().count();
        let mut risk_flags = source_risk_flags(mention, provider, provider_trace, exact_phone_match, context_chars, linked_signal_count);
        let strengths = source_strengths(mention, provider, exact_phone_match, linked_signal_count, correlation_count, &year_hints);
        let quality_score = calculate_quality_score(mention, provider, exact_phone_match, context_chars, linked_signal_count, correlation_count, year_hints.len(), &risk_flags);
        let quality_level = quality_level(quality_score);
        let reasons = quality_reasons(&quality_level, quality_score, &strengths, &risk_flags);

        risk_flags.sort();
        risk_flags.dedup();
        scores.push(PhoneSourceQualityScore {
            phone_value: mention.value.clone(),
            source_id: mention.source_id.clone(),
            url: mention.url.clone(),
            quality_level,
            quality_score,
            exact_phone_match,
            context_chars,
            linked_signal_count,
            correlation_count,
            year_hints,
            provider_status: provider.map(|p| format!("{:?}", p.status)),
            provider_hits: provider.map(|p| p.hits).unwrap_or_default(),
            provider_errors: provider.map(|p| p.errors).unwrap_or_default(),
            provider_rate_limited: provider.map(|p| p.rate_limited).unwrap_or_default(),
            risk_flags,
            strengths,
            reasons,
        });
    }

    scores.sort_by(|a, b| b.quality_score.cmp(&a.quality_score));
    PhoneSourceQualityReport {
        high_or_better: scores.iter().filter(|s| matches!(s.quality_level, PhoneSourceQualityLevel::High | PhoneSourceQualityLevel::VeryHigh)).count(),
        medium_or_better: scores.iter().filter(|s| !matches!(s.quality_level, PhoneSourceQualityLevel::Low)).count(),
        low_quality: scores.iter().filter(|s| matches!(s.quality_level, PhoneSourceQualityLevel::Low)).count(),
        risky_sources: scores.iter().filter(|s| !s.risk_flags.is_empty()).count(),
        scores,
    }
}

fn years_for_mention(years: &[PhoneSourceYear], mention: &PhonePublicMention) -> Vec<u32> {
    let mut out = years
        .iter()
        .filter(|y| y.phone_value == mention.value && y.source_id == mention.source_id && y.url == mention.url)
        .map(|y| y.year)
        .collect::<Vec<_>>();
    out.sort();
    out.dedup();
    out
}

fn source_risk_flags(
    mention: &PhonePublicMention,
    provider: Option<&PhoneSearchProviderSummary>,
    trace: Option<&PhoneSearchProviderTrace>,
    exact_phone_match: bool,
    context_chars: usize,
    linked_signal_count: usize,
) -> Vec<String> {
    let mut flags = Vec::new();
    if !exact_phone_match {
        flags.push("no_exact_phone_in_context_or_url".to_string());
    }
    if context_chars < 40 {
        flags.push("thin_context".to_string());
    }
    if linked_signal_count == 0 {
        flags.push("no_linked_context_signals".to_string());
    }
    if mention.url.is_none() {
        flags.push("missing_public_url".to_string());
    }
    if let Some(provider) = provider {
        if provider.rate_limited > 0 { flags.push("provider_rate_limited".to_string()); }
        if provider.errors > 0 { flags.push("provider_errors_present".to_string()); }
        if provider.hits == 0 { flags.push("provider_summary_has_no_hits".to_string()); }
        if matches!(provider.status, PhoneProviderStatus::Error | PhoneProviderStatus::RateLimited) {
            flags.push("provider_unstable_status".to_string());
        }
    }
    if let Some(trace) = trace {
        if matches!(trace.status, PhoneProviderStatus::Error | PhoneProviderStatus::RateLimited) {
            flags.push("trace_unstable_status".to_string());
        }
    }
    flags
}

fn source_strengths(
    mention: &PhonePublicMention,
    provider: Option<&PhoneSearchProviderSummary>,
    exact_phone_match: bool,
    linked_signal_count: usize,
    correlation_count: usize,
    year_hints: &[u32],
) -> Vec<String> {
    let mut strengths = Vec::new();
    if exact_phone_match { strengths.push("exact_phone_match".to_string()); }
    if mention.url.is_some() { strengths.push("public_url_present".to_string()); }
    if mention.context_snippet.chars().count() >= 120 { strengths.push("rich_context".to_string()); }
    if linked_signal_count > 0 { strengths.push(format!("linked_signals={}", linked_signal_count)); }
    if correlation_count > 0 { strengths.push(format!("correlations={}", correlation_count)); }
    if !year_hints.is_empty() { strengths.push(format!("year_hints={:?}", year_hints)); }
    if let Some(provider) = provider {
        if provider.hits > 0 { strengths.push(format!("provider_hits={}", provider.hits)); }
        if provider.errors == 0 && provider.rate_limited == 0 { strengths.push("provider_stable".to_string()); }
    }
    strengths
}

fn calculate_quality_score(
    mention: &PhonePublicMention,
    provider: Option<&PhoneSearchProviderSummary>,
    exact_phone_match: bool,
    context_chars: usize,
    linked_signal_count: usize,
    correlation_count: usize,
    year_count: usize,
    risk_flags: &[String],
) -> u8 {
    let mut score = mention.confidence as isize;
    if exact_phone_match { score += 10; } else { score -= 18; }
    if mention.url.is_some() { score += 6; } else { score -= 8; }
    if context_chars >= 120 { score += 8; } else if context_chars < 40 { score -= 10; }
    score += (linked_signal_count.min(4) * 5) as isize;
    score += (correlation_count.min(4) * 6) as isize;
    score += (year_count.min(3) * 2) as isize;
    if let Some(provider) = provider {
        if provider.hits > 0 { score += 5; }
        if provider.errors > 0 { score -= 6; }
        if provider.rate_limited > 0 { score -= 8; }
        if matches!(provider.status, PhoneProviderStatus::Matched) { score += 5; }
    }
    score -= (risk_flags.len().min(6) * 3) as isize;
    score.clamp(0, 98) as u8
}

fn quality_level(score: u8) -> PhoneSourceQualityLevel {
    if score >= 86 { PhoneSourceQualityLevel::VeryHigh }
    else if score >= 72 { PhoneSourceQualityLevel::High }
    else if score >= 50 { PhoneSourceQualityLevel::Medium }
    else { PhoneSourceQualityLevel::Low }
}

fn quality_reasons(level: &PhoneSourceQualityLevel, score: u8, strengths: &[String], risk_flags: &[String]) -> Vec<String> {
    let mut reasons = Vec::new();
    reasons.push(format!("source quality {:?} score={}", level, score));
    if !strengths.is_empty() { reasons.push(format!("strengths: {}", strengths.join(", "))); }
    if !risk_flags.is_empty() { reasons.push(format!("risk_flags: {}", risk_flags.join(", "))); }
    if matches!(level, PhoneSourceQualityLevel::Low | PhoneSourceQualityLevel::Medium) {
        reasons.push("treat this as a lead, not a confirmed identity link".to_string());
    }
    reasons
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_score_rewards_rich_exact_context() {
        let mention = PhonePublicMention {
            source_id: "phone_public_url_probe".to_string(),
            url: Some("https://example.com/profile".to_string()),
            value: "+375257997676".to_string(),
            context_snippet: "public contact card +375257997676 email test@example.com username @tester year 2021 with additional profile context".to_string(),
            confidence: 70,
            note: "configured_public_url_probe_exact_match".to_string(),
        };
        let signals = vec![PhoneExtractedSignal { phone_value: "+375257997676".to_string(), entity_type: crate::models::EntityType::Email, value: "test@example.com".to_string(), source_id: "phone_public_url_probe".to_string(), url: Some("https://example.com/profile".to_string()), confidence: 62, reason: "email".to_string(), context_snippet: mention.context_snippet.clone() }];
        let report = score_phone_sources(&[mention], &signals, &[], &[], &[], &[]);
        assert_eq!(report.scores.len(), 1);
        assert!(report.scores[0].quality_score >= 70);
    }
}
