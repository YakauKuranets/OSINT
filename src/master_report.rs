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
    pub autopilot_cycles: Option<u64>,
    pub autopilot_new_nodes: Option<u64>,
    pub discovery_findings: Option<u64>,
    pub public_search_findings: Option<u64>,
    pub public_search_blocked: Option<u64>,
    pub public_search_downranked: Option<u64>,
    pub email_valid_count: Option<u64>,
    pub email_domain_count: Option<u64>,
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
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn build_master_report() -> MasterReport {
    let report_paths = vec![
        ("run_profile", "run_profile_report.json"),
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
        if !slot.loaded {
            missing_reports.push(path.to_string());
        }
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
            "confidence_report.json".to_string(),
            "conflict_report.json".to_string(),
            "public_search_report.json".to_string(),
            "discovery_report.json".to_string(),
            "autopilot_report.json".to_string(),
            "email_domain_report.json".to_string(),
            "analysis_report.json".to_string(),
            "report.html".to_string(),
        ],
    }
}

pub fn save_master_report(report: &MasterReport, path: &str) -> Result<(), String> {
    let json = serde_json::to_string_pretty(report)
        .map_err(|err| format!("serialize master report: {}", err))?;
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
            Ok(value) => ReportSlot {
                path: path.to_string(),
                loaded: true,
                data: Some(value),
                error: None,
            },
            Err(err) => ReportSlot {
                path: path.to_string(),
                loaded: false,
                data: None,
                error: Some(format!("parse error: {}", err)),
            },
        },
        Err(err) => ReportSlot {
            path: path.to_string(),
            loaded: false,
            data: None,
            error: Some(format!("read error: {}", err)),
        },
    }
}

fn build_summary(reports: &BTreeMap<String, ReportSlot>) -> MasterSummary {
    let run_profile = get_report(reports, "run_profile")
        .and_then(|v| v.get("label"))
        .and_then(Value::as_str)
        .map(|s| s.to_string());

    MasterSummary {
        run_profile,
        autopilot_cycles: get_report(reports, "autopilot")
            .and_then(|v| v.get("cycles"))
            .and_then(Value::as_array)
            .map(|arr| arr.len() as u64),
        autopilot_new_nodes: get_report(reports, "autopilot")
            .and_then(|v| v.get("total_new_nodes"))
            .and_then(Value::as_u64),
        discovery_findings: get_report(reports, "discovery")
            .and_then(|v| v.pointer("/stats/findings_count"))
            .and_then(Value::as_u64),
        public_search_findings: get_report(reports, "public_search")
            .and_then(|v| v.pointer("/stats/findings_count"))
            .and_then(Value::as_u64),
        public_search_blocked: get_report(reports, "public_search")
            .and_then(|v| v.pointer("/stats/blocked_by_noise_rules"))
            .and_then(Value::as_u64),
        public_search_downranked: get_report(reports, "public_search")
            .and_then(|v| v.pointer("/stats/downranked_by_noise_rules"))
            .and_then(Value::as_u64),
        email_valid_count: get_report(reports, "email_domain")
            .and_then(|v| v.pointer("/stats/valid_emails"))
            .and_then(Value::as_u64),
        email_domain_count: get_report(reports, "email_domain")
            .and_then(|v| v.pointer("/stats/domains_checked"))
            .and_then(Value::as_u64),
        conflict_count: get_report(reports, "conflicts")
            .and_then(|v| v.get("findings"))
            .and_then(Value::as_array)
            .map(|arr| arr.len() as u64),
        active_links: get_report(reports, "analysis")
            .and_then(|v| v.pointer("/profile_summary/active_links"))
            .and_then(Value::as_u64)
            .or_else(|| get_report(reports, "resolution").and_then(|v| v.pointer("/active_links")).and_then(Value::as_u64)),
        associated_nodes: get_report(reports, "analysis")
            .and_then(|v| v.pointer("/profile_summary/associated_nodes"))
            .and_then(Value::as_u64)
            .or_else(|| get_report(reports, "resolution").and_then(|v| v.pointer("/associated_nodes")).and_then(Value::as_u64)),
    }
}

fn build_verdict(
    summary: &MasterSummary,
    reports: &BTreeMap<String, ReportSlot>,
    missing_reports: &[String],
) -> MasterVerdict {
    let confidence_adjusted = get_report(reports, "confidence")
        .and_then(|v| v.get("adjusted_score"))
        .and_then(Value::as_u64);
    let conflict_count = summary.conflict_count.unwrap_or(0);
    let high_risk = get_report(reports, "conflicts")
        .and_then(|v| v.get("high_risk"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || conflict_count > 0;

    let mut reasons = Vec::new();
    if !missing_reports.is_empty() {
        reasons.push(format!("{} runtime reports are missing", missing_reports.len()));
    }
    if high_risk {
        reasons.push("conflict engine reported risk or conflicts".to_string());
    }
    if let Some(score) = confidence_adjusted {
        if score < 50 {
            reasons.push("adjusted confidence is below 50".to_string());
        } else if score < 75 {
            reasons.push("adjusted confidence is moderate, manual review recommended".to_string());
        }
    } else {
        reasons.push("confidence report is unavailable".to_string());
    }
    if summary.public_search_blocked.unwrap_or(0) > 0 || summary.public_search_downranked.unwrap_or(0) > 0 {
        reasons.push("noise rules blocked or downranked public-search findings".to_string());
    }
    if summary.autopilot_new_nodes.unwrap_or(0) == 0 {
        reasons.push("autopilot did not discover new nodes".to_string());
    }

    let status = if high_risk {
        "review_required"
    } else if confidence_adjusted.unwrap_or(0) >= 75 && missing_reports.is_empty() {
        "usable_with_review"
    } else if confidence_adjusted.unwrap_or(0) >= 50 {
        "partial_review_required"
    } else {
        "low_confidence"
    }
    .to_string();

    MasterVerdict {
        status,
        confidence_adjusted,
        high_risk,
        needs_human_review: true,
        reasons,
    }
}

fn get_report<'a>(reports: &'a BTreeMap<String, ReportSlot>, key: &str) -> Option<&'a Value> {
    reports.get(key)?.data.as_ref()
}

pub fn compact_master_summary(report: &MasterReport) -> Value {
    json!({
        "status": report.verdict.status,
        "confidence_adjusted": report.verdict.confidence_adjusted,
        "high_risk": report.verdict.high_risk,
        "run_profile": report.summary.run_profile,
        "autopilot_cycles": report.summary.autopilot_cycles,
        "autopilot_new_nodes": report.summary.autopilot_new_nodes,
        "discovery_findings": report.summary.discovery_findings,
        "public_search_findings": report.summary.public_search_findings,
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
        let report = MasterReport {
            generated_at: 0,
            verdict: MasterVerdict {
                status: "low_confidence".to_string(),
                confidence_adjusted: Some(20),
                high_risk: false,
                needs_human_review: true,
                reasons: vec![],
            },
            summary: MasterSummary::default(),
            reports: BTreeMap::new(),
            missing_reports: vec![],
            recommended_review_order: vec![],
        };
        assert_eq!(compact_master_summary(&report).get("status").and_then(Value::as_str), Some("low_confidence"));
    }
}
