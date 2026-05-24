use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterReport {
    pub generated_at: u64,
    pub verdict: MasterVerdict,
    pub summary: MasterSummary,
    pub reports: BTreeMap<String, ReportSlot>,
    pub missing_reports: Vec<String>,
    pub recommended_review_order: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterVerdict {
    pub status: String,
    pub confidence_adjusted: Option<u64>,
    pub high_risk: bool,
    pub needs_human_review: bool,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MasterSummary {
    pub run_profile: Option<String>,
    pub phone_checked: Option<u64>,
    pub phone_valid_shape: Option<u64>,
    pub phone_carrier_guesses: Option<u64>,
    pub phone_search_terms_generated: Option<u64>,
    pub phone_search_tasks_executed: Option<u64>,
    pub phone_api_errors: Option<u64>,
    pub phone_public_mentions: Option<u64>,
    pub phone_linked_entities: Option<u64>,
    pub autopilot_cycles: Option<u64>,
    pub autopilot_new_nodes: Option<u64>,
    pub autopilot_phone_new_nodes: Option<u64>,
    pub autopilot_email_domain_new_nodes: Option<u64>,
    pub autopilot_discovery_new_nodes: Option<u64>,
    pub autopilot_public_search_new_nodes: Option<u64>,
    pub discovery_findings: Option<u64>,
    pub discovery_blocked: Option<u64>,
    pub discovery_downranked: Option<u64>,
    pub public_search_findings: Option<u64>,
    pub public_search_blocked: Option<u64>,
    pub public_search_downranked: Option<u64>,
    pub email_valid_count: Option<u64>,
    pub email_domain_count: Option<u64>,
    pub email_username_candidates: Option<u64>,
    pub email_free_mail_domains: Option<u64>,
    pub email_corporate_domains: Option<u64>,
    pub email_suspicious_domains: Option<u64>,
    pub conflict_count: Option<u64>,
    pub active_links: Option<u64>,
    pub associated_nodes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSlot {
    pub path: String,
    pub loaded: bool,
    pub data: Option<Value>,
    pub error: Option<String>,
}

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

pub fn build_master_report() -> MasterReport {
    let report_paths = vec![
        ("run_profile", "run_profile_report.json"),
        ("preflight", "preflight_report.json"),
        ("phone_intel", "phone_intel_report.json"),
        ("autopilot", "autopilot_report.json"),
        ("discovery", "discovery_report.json"),
        ("public_search", "public_search_report.json"),
        ("email_domain", "email_domain_report.json"),
        ("confidence", "confidence_report.json"),
        ("conflicts", "conflict_report.json"),
        ("resolution", "resolution_report.json"),
        ("analysis", "analysis_report.json"),
        ("stix", "stix_report.json"),
    ];

    let mut reports = BTreeMap::new();
    let mut missing_reports = Vec::new();
    for (key, path) in report_paths {
        let slot = load_report_slot(path);
        if !slot.loaded { missing_reports.push(path.to_string()); }
        reports.insert(key.to_string(), slot);
    }

    let summary = build_summary(&reports);
    let verdict = build_verdict(&summary, &reports, &missing_reports);
    MasterReport {
        generated_at: now_unix(),
        verdict,
        summary,
        reports,
        missing_reports,
        recommended_review_order: vec![
            "master_report.json".to_string(),
            "phone_intel_report.json".to_string(),
            "confidence_report.json".to_string(),
            "conflict_report.json".to_string(),
            "email_domain_report.json".to_string(),
            "discovery_report.json".to_string(),
            "public_search_report.json".to_string(),
            "autopilot_report.json".to_string(),
            "analysis_report.json".to_string(),
            "report.html".to_string(),
        ],
    }
}

pub fn save_master_report(report: &MasterReport, path: &str) -> Result<(), String> {
    let json = serde_json::to_string_pretty(report).map_err(|err| format!("serialize master report: {}", err))?;
    std::fs::write(path, json).map_err(|err| format!("write {}: {}", path, err))
}

pub fn build_and_save_master_report(path: &str) -> Result<MasterReport, String> {
    let report = build_master_report();
    save_master_report(&report, path)?;
    Ok(report)
}

fn load_report_slot(path: &str) -> ReportSlot {
    match std::fs::read_to_string(path) {
        Ok(raw) => match serde_json::from_str::<Value>(&raw) {
            Ok(value) => ReportSlot { path: path.to_string(), loaded: true, data: Some(value), error: None },
            Err(err) => ReportSlot { path: path.to_string(), loaded: false, data: None, error: Some(format!("parse error: {}", err)) },
        },
        Err(err) => ReportSlot { path: path.to_string(), loaded: false, data: None, error: Some(format!("read error: {}", err)) },
    }
}

fn report_stat(reports: &BTreeMap<String, ReportSlot>, report: &str, name: &str) -> Option<u64> {
    get_report(reports, report)
        .and_then(|v| v.pointer(&format!("/stats/{}", name)))
        .and_then(Value::as_u64)
}

fn build_summary(reports: &BTreeMap<String, ReportSlot>) -> MasterSummary {
    let run_profile = get_report(reports, "run_profile").and_then(|v| v.get("label")).and_then(Value::as_str).map(|s| s.to_string());
    MasterSummary {
        run_profile,
        phone_checked: report_stat(reports, "phone_intel", "phones_checked"),
        phone_valid_shape: report_stat(reports, "phone_intel", "valid_shape"),
        phone_carrier_guesses: report_stat(reports, "phone_intel", "carrier_guesses"),
        phone_search_terms_generated: report_stat(reports, "phone_intel", "search_terms_generated"),
        phone_search_tasks_executed: report_stat(reports, "phone_intel", "search_tasks_executed"),
        phone_api_errors: report_stat(reports, "phone_intel", "api_errors"),
        phone_public_mentions: report_stat(reports, "phone_intel", "public_mentions"),
        phone_linked_entities: report_stat(reports, "phone_intel", "linked_entities"),
        autopilot_cycles: get_report(reports, "autopilot").and_then(|v| v.get("cycles")).and_then(Value::as_array).map(|arr| arr.len() as u64),
        autopilot_new_nodes: get_report(reports, "autopilot").and_then(|v| v.get("total_new_nodes")).and_then(Value::as_u64),
        autopilot_phone_new_nodes: sum_cycle_field(reports, "new_phone_intel_nodes"),
        autopilot_email_domain_new_nodes: sum_cycle_field(reports, "new_email_domain_nodes"),
        autopilot_discovery_new_nodes: sum_cycle_field(reports, "new_discovery_nodes"),
        autopilot_public_search_new_nodes: sum_cycle_field(reports, "new_public_search_nodes"),
        discovery_findings: report_stat(reports, "discovery", "findings_count"),
        discovery_blocked: report_stat(reports, "discovery", "blocked_by_noise_rules"),
        discovery_downranked: report_stat(reports, "discovery", "downranked_by_noise_rules"),
        public_search_findings: report_stat(reports, "public_search", "findings_count"),
        public_search_blocked: report_stat(reports, "public_search", "blocked_by_noise_rules"),
        public_search_downranked: report_stat(reports, "public_search", "downranked_by_noise_rules"),
        email_valid_count: report_stat(reports, "email_domain", "valid_emails"),
        email_domain_count: report_stat(reports, "email_domain", "domains_checked"),
        email_username_candidates: report_stat(reports, "email_domain", "username_candidates"),
        email_free_mail_domains: report_stat(reports, "email_domain", "free_mail_domains"),
        email_corporate_domains: report_stat(reports, "email_domain", "corporate_domains"),
        email_suspicious_domains: report_stat(reports, "email_domain", "suspicious_domains"),
        conflict_count: get_report(reports, "conflicts").and_then(|v| v.get("findings")).and_then(Value::as_array).map(|arr| arr.len() as u64),
        active_links: get_report(reports, "analysis").and_then(|v| v.pointer("/profile_summary/active_links")).and_then(Value::as_u64).or_else(|| get_report(reports, "resolution").and_then(|v| v.pointer("/active_links")).and_then(Value::as_u64)),
        associated_nodes: get_report(reports, "analysis").and_then(|v| v.pointer("/profile_summary/associated_nodes")).and_then(Value::as_u64).or_else(|| get_report(reports, "resolution").and_then(|v| v.pointer("/associated_nodes")).and_then(Value::as_u64)),
    }
}

fn build_verdict(summary: &MasterSummary, reports: &BTreeMap<String, ReportSlot>, missing_reports: &[String]) -> MasterVerdict {
    let confidence_adjusted = get_report(reports, "confidence").and_then(|v| v.get("adjusted_score")).and_then(Value::as_u64);
    let conflict_count = summary.conflict_count.unwrap_or(0);
    let high_risk = get_report(reports, "conflicts").and_then(|v| v.get("high_risk")).and_then(Value::as_bool).unwrap_or(false) || conflict_count > 0 || summary.email_suspicious_domains.unwrap_or(0) > 0;
    let mut reasons = Vec::new();
    if !missing_reports.is_empty() { reasons.push(format!("{} runtime reports are missing", missing_reports.len())); }
    if high_risk { reasons.push("conflict/risk engine reported review-worthy signals".to_string()); }
    if let Some(score) = confidence_adjusted {
        if score < 50 { reasons.push("adjusted confidence is below 50".to_string()); }
        else if score < 75 { reasons.push("adjusted confidence is moderate, manual review recommended".to_string()); }
    } else { reasons.push("confidence report is unavailable".to_string()); }
    if summary.public_search_blocked.unwrap_or(0) > 0 || summary.public_search_downranked.unwrap_or(0) > 0 || summary.discovery_blocked.unwrap_or(0) > 0 || summary.discovery_downranked.unwrap_or(0) > 0 { reasons.push("noise rules blocked or downranked discovery/search findings".to_string()); }
    if summary.autopilot_new_nodes.unwrap_or(0) == 0 { reasons.push("autopilot did not discover new nodes".to_string()); }
    if summary.autopilot_phone_new_nodes.unwrap_or(0) > 0 { reasons.push("autopilot expanded through phone-intel-derived nodes".to_string()); }
    if summary.autopilot_email_domain_new_nodes.unwrap_or(0) > 0 { reasons.push("autopilot expanded through email/domain-derived nodes".to_string()); }
    if summary.phone_carrier_guesses.unwrap_or(0) > 0 { reasons.push("phone intel produced carrier prefix guesses, not ownership confirmation".to_string()); }
    if summary.phone_search_tasks_executed.unwrap_or(0) > 0 && summary.phone_public_mentions.unwrap_or(0) == 0 { reasons.push("phone search adapter executed but found no public mentions".to_string()); }
    if summary.phone_api_errors.unwrap_or(0) > 0 { reasons.push("phone search adapter had API/rate-limit errors".to_string()); }
    if summary.email_suspicious_domains.unwrap_or(0) > 0 { reasons.push("email/domain checker found suspicious domain signals".to_string()); }
    let status = if high_risk { "review_required" } else if confidence_adjusted.unwrap_or(0) >= 75 && missing_reports.is_empty() { "usable_with_review" } else if confidence_adjusted.unwrap_or(0) >= 50 { "partial_review_required" } else { "low_confidence" }.to_string();
    MasterVerdict { status, confidence_adjusted, high_risk, needs_human_review: true, reasons }
}

fn get_report<'a>(reports: &'a BTreeMap<String, ReportSlot>, key: &str) -> Option<&'a Value> { reports.get(key)?.data.as_ref() }

fn sum_cycle_field(reports: &BTreeMap<String, ReportSlot>, field: &str) -> Option<u64> {
    let cycles = get_report(reports, "autopilot")?.get("cycles")?.as_array()?;
    Some(cycles.iter().filter_map(|cycle| cycle.get(field).and_then(Value::as_u64)).sum())
}

pub fn compact_master_summary(report: &MasterReport) -> Value {
    json!({
        "status": report.verdict.status,
        "confidence_adjusted": report.verdict.confidence_adjusted,
        "high_risk": report.verdict.high_risk,
        "run_profile": report.summary.run_profile,
        "phone_checked": report.summary.phone_checked,
        "phone_valid_shape": report.summary.phone_valid_shape,
        "phone_carrier_guesses": report.summary.phone_carrier_guesses,
        "phone_search_terms_generated": report.summary.phone_search_terms_generated,
        "phone_search_tasks_executed": report.summary.phone_search_tasks_executed,
        "phone_api_errors": report.summary.phone_api_errors,
        "phone_public_mentions": report.summary.phone_public_mentions,
        "phone_linked_entities": report.summary.phone_linked_entities,
        "autopilot_cycles": report.summary.autopilot_cycles,
        "autopilot_new_nodes": report.summary.autopilot_new_nodes,
        "autopilot_phone_new_nodes": report.summary.autopilot_phone_new_nodes,
        "autopilot_email_domain_new_nodes": report.summary.autopilot_email_domain_new_nodes,
        "autopilot_discovery_new_nodes": report.summary.autopilot_discovery_new_nodes,
        "autopilot_public_search_new_nodes": report.summary.autopilot_public_search_new_nodes,
        "discovery_findings": report.summary.discovery_findings,
        "discovery_blocked": report.summary.discovery_blocked,
        "public_search_findings": report.summary.public_search_findings,
        "public_search_blocked": report.summary.public_search_blocked,
        "email_username_candidates": report.summary.email_username_candidates,
        "email_suspicious_domains": report.summary.email_suspicious_domains,
        "conflicts": report.summary.conflict_count,
        "missing_reports": report.missing_reports,
        "review_order": report.recommended_review_order,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_slot_is_not_loaded() {
        let slot = load_report_slot("definitely_missing_report_for_test.json");
        assert!(!slot.loaded);
        assert!(slot.error.is_some());
    }
    #[test]
    fn compact_summary_contains_status() {
        let report = MasterReport { generated_at: 0, verdict: MasterVerdict { status: "low_confidence".to_string(), confidence_adjusted: Some(20), high_risk: false, needs_human_review: true, reasons: vec![] }, summary: MasterSummary::default(), reports: BTreeMap::new(), missing_reports: vec![], recommended_review_order: vec![] };
        assert_eq!(compact_master_summary(&report).get("status").and_then(Value::as_str), Some("low_confidence"));
    }
}
