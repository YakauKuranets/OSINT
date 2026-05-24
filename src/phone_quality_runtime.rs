use crate::phone_intel::{PhoneCorrelationSummary, PhoneExtractedSignal, PhonePublicMention, PhoneSourceYear};
use crate::phone_quality;
use crate::phone_search::{PhoneSearchProviderSummary, PhoneSearchProviderTrace};
use serde_json::Value;

pub fn enrich_phone_intel_report_file(path: &str) -> Result<(), String> {
    let raw = std::fs::read_to_string(path).map_err(|err| format!("read {}: {}", path, err))?;
    let mut json: Value = serde_json::from_str(&raw).map_err(|err| format!("parse {}: {}", path, err))?;

    let mentions: Vec<PhonePublicMention> = read_vec(&json, "public_mentions")?;
    let signals: Vec<PhoneExtractedSignal> = read_vec(&json, "extracted_signals")?;
    let correlations: Vec<PhoneCorrelationSummary> = read_vec(&json, "correlation_summaries")?;
    let years: Vec<PhoneSourceYear> = read_vec(&json, "source_years")?;
    let provider_summaries: Vec<PhoneSearchProviderSummary> = read_vec(&json, "provider_summaries")?;
    let traces: Vec<PhoneSearchProviderTrace> = read_vec(&json, "traces")?;

    let quality = phone_quality::score_phone_sources(
        &mentions,
        &signals,
        &correlations,
        &years,
        &provider_summaries,
        &traces,
    );

    let quality_json = serde_json::to_value(&quality).map_err(|err| format!("serialize source_quality: {}", err))?;
    json["source_quality"] = quality_json.clone();
    ensure_stats_object(&mut json);
    json["stats"]["source_quality_scores"] = serde_json::json!(quality.scores.len());
    json["stats"]["high_quality_phone_sources"] = serde_json::json!(quality.high_or_better);
    json["stats"]["medium_quality_phone_sources"] = serde_json::json!(quality.medium_or_better);
    json["stats"]["low_quality_phone_sources"] = serde_json::json!(quality.low_quality);
    json["stats"]["risky_phone_sources"] = serde_json::json!(quality.risky_sources);

    let enriched = serde_json::to_string_pretty(&json).map_err(|err| format!("serialize enriched {}: {}", path, err))?;
    std::fs::write(path, enriched).map_err(|err| format!("write enriched {}: {}", path, err))?;
    std::fs::write("phone_source_quality_report.json", serde_json::to_string_pretty(&quality).map_err(|err| format!("serialize phone_source_quality_report.json: {}", err))?)
        .map_err(|err| format!("write phone_source_quality_report.json: {}", err))?;
    Ok(())
}

fn read_vec<T: serde::de::DeserializeOwned>(json: &Value, field: &str) -> Result<Vec<T>, String> {
    match json.get(field) {
        Some(value) => serde_json::from_value(value.clone()).map_err(|err| format!("deserialize {}: {}", field, err)),
        None => Ok(Vec::new()),
    }
}

fn ensure_stats_object(json: &mut Value) {
    if !json.get("stats").map(|v| v.is_object()).unwrap_or(false) {
        json["stats"] = serde_json::json!({});
    }
}
