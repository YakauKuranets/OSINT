use crate::runtime_profile;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreflightSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightCheck {
    pub name: String,
    pub severity: PreflightSeverity,
    pub ok: bool,
    pub message: String,
    pub recommendation: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PreflightSummary {
    pub checks_total: usize,
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    pub can_continue: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightReport {
    pub generated_at: u64,
    pub profile: String,
    pub working_directory: String,
    pub summary: PreflightSummary,
    pub checks: Vec<PreflightCheck>,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn run_preflight_doctor() -> PreflightReport {
    let profile = runtime_profile::config().clone();
    let working_directory = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let mut checks = Vec::new();
    checks.push(check_working_directory());
    checks.push(check_or_create_directory("dumps"));
    checks.push(check_write_permission("preflight_write_test.tmp"));
    checks.push(check_profile_sanity(&profile));
    checks.push(check_network_profile_consistency(&profile));
    checks.push(check_report_targets_writable());

    let summary = summarize_checks(&checks);

    PreflightReport {
        generated_at: now_unix(),
        profile: profile.label,
        working_directory,
        summary,
        checks,
    }
}

pub fn save_preflight_report(report: &PreflightReport, path: &str) -> Result<(), String> {
    let json = serde_json::to_string_pretty(report)
        .map_err(|err| format!("serialize preflight report: {}", err))?;
    std::fs::write(path, json).map_err(|err| format!("write {}: {}", path, err))
}

pub fn run_and_save_preflight(path: &str) -> PreflightReport {
    let report = run_preflight_doctor();
    if let Err(err) = save_preflight_report(&report, path) {
        eprintln!("[!] Не удалось сохранить {}: {}", path, err);
    }
    report
}

fn check_working_directory() -> PreflightCheck {
    match std::env::current_dir() {
        Ok(dir) => PreflightCheck {
            name: "working_directory".to_string(),
            severity: PreflightSeverity::Info,
            ok: true,
            message: format!("Working directory: {}", dir.display()),
            recommendation: None,
        },
        Err(err) => PreflightCheck {
            name: "working_directory".to_string(),
            severity: PreflightSeverity::Error,
            ok: false,
            message: format!("Cannot read current directory: {}", err),
            recommendation: Some("Запусти программу из корня проекта OSINT".to_string()),
        },
    }
}

fn check_or_create_directory(path: &str) -> PreflightCheck {
    let dir = Path::new(path);
    if dir.exists() && dir.is_dir() {
        return PreflightCheck {
            name: format!("directory:{}", path),
            severity: PreflightSeverity::Info,
            ok: true,
            message: format!("Directory exists: {}", path),
            recommendation: None,
        };
    }

    match std::fs::create_dir_all(dir) {
        Ok(_) => PreflightCheck {
            name: format!("directory:{}", path),
            severity: PreflightSeverity::Warning,
            ok: true,
            message: format!("Directory was missing and has been created: {}", path),
            recommendation: Some("Проверь, что это ожидаемая рабочая папка проекта".to_string()),
        },
        Err(err) => PreflightCheck {
            name: format!("directory:{}", path),
            severity: PreflightSeverity::Error,
            ok: false,
            message: format!("Cannot create directory {}: {}", path, err),
            recommendation: Some("Проверь права записи и путь запуска".to_string()),
        },
    }
}

fn check_write_permission(test_path: &str) -> PreflightCheck {
    match std::fs::write(test_path, b"xgen-preflight") {
        Ok(_) => {
            let _ = std::fs::remove_file(test_path);
            PreflightCheck {
                name: "write_permission".to_string(),
                severity: PreflightSeverity::Info,
                ok: true,
                message: "Current directory is writable".to_string(),
                recommendation: None,
            }
        }
        Err(err) => PreflightCheck {
            name: "write_permission".to_string(),
            severity: PreflightSeverity::Error,
            ok: false,
            message: format!("Cannot write test file: {}", err),
            recommendation: Some("Запусти терминал с правами на запись или перейди в рабочую папку проекта".to_string()),
        },
    }
}

fn check_report_targets_writable() -> PreflightCheck {
    let targets = [
        "run_profile_report.json",
        "preflight_report.json",
        "autopilot_report.json",
        "discovery_report.json",
        "public_search_report.json",
        "email_domain_report.json",
        "confidence_report.json",
        "conflict_report.json",
        "analysis_report.json",
        "master_report.json",
        "report.html",
    ];

    let blocked = targets
        .iter()
        .filter(|path| Path::new(path).exists() && std::fs::OpenOptions::new().append(true).open(path).is_err())
        .map(|s| s.to_string())
        .collect::<Vec<_>>();

    if blocked.is_empty() {
        PreflightCheck {
            name: "report_targets_writable".to_string(),
            severity: PreflightSeverity::Info,
            ok: true,
            message: "Report targets are writable or not created yet".to_string(),
            recommendation: None,
        }
    } else {
        PreflightCheck {
            name: "report_targets_writable".to_string(),
            severity: PreflightSeverity::Error,
            ok: false,
            message: format!("Some report files are not writable: {}", blocked.join(", ")),
            recommendation: Some("Закрой файлы в редакторе/браузере, проверь права доступа и повтори запуск".to_string()),
        }
    }
}

fn check_profile_sanity(profile: &runtime_profile::RunProfileConfig) -> PreflightCheck {
    let mut problems = Vec::new();
    if profile.autopilot_cycles == 0 || profile.autopilot_cycles > 5 {
        problems.push("autopilot_cycles must be 1..=5".to_string());
    }
    if profile.discovery_fetch && profile.discovery_max_tasks == 0 {
        problems.push("discovery_fetch=true but discovery_max_tasks=0".to_string());
    }
    if profile.github_search && profile.public_search_max_tasks == 0 {
        problems.push("github_search=true but public_search_max_tasks=0".to_string());
    }
    if profile.discovery_max_bytes < 1024 {
        problems.push("discovery_max_bytes is too small".to_string());
    }

    if problems.is_empty() {
        PreflightCheck {
            name: "profile_sanity".to_string(),
            severity: PreflightSeverity::Info,
            ok: true,
            message: format!("Profile '{}' looks sane", profile.label),
            recommendation: None,
        }
    } else {
        PreflightCheck {
            name: "profile_sanity".to_string(),
            severity: PreflightSeverity::Warning,
            ok: false,
            message: problems.join("; "),
            recommendation: Some("Используй профиль quick/safe/deep/offline или проверь env overrides".to_string()),
        }
    }
}

fn check_network_profile_consistency(profile: &runtime_profile::RunProfileConfig) -> PreflightCheck {
    if matches!(profile.name, runtime_profile::RunProfileName::Offline)
        && (profile.discovery_fetch || profile.github_search || profile.dns_check)
    {
        return PreflightCheck {
            name: "network_profile_consistency".to_string(),
            severity: PreflightSeverity::Error,
            ok: false,
            message: "Offline profile still has network-enabled components".to_string(),
            recommendation: Some("Проверь runtime_profile::offline()".to_string()),
        };
    }

    let active_network = profile.discovery_fetch || profile.github_search || profile.dns_check;
    PreflightCheck {
        name: "network_profile_consistency".to_string(),
        severity: PreflightSeverity::Info,
        ok: true,
        message: if active_network {
            "Network-enabled profile: public fetch/API/DNS may be used".to_string()
        } else {
            "Network-disabled profile: offline/local-only execution".to_string()
        },
        recommendation: None,
    }
}

fn summarize_checks(checks: &[PreflightCheck]) -> PreflightSummary {
    let mut summary = PreflightSummary {
        checks_total: checks.len(),
        can_continue: true,
        ..PreflightSummary::default()
    };

    for check in checks {
        match check.severity {
            PreflightSeverity::Info => summary.infos += 1,
            PreflightSeverity::Warning => summary.warnings += 1,
            PreflightSeverity::Error => summary.errors += 1,
        }
    }

    summary.can_continue = checks.iter().all(|check| check.severity != PreflightSeverity::Error || check.ok);
    summary
}

pub fn print_preflight_report(report: &PreflightReport) {
    println!(
        "\n[*] Preflight Doctor: checks={} errors={} warnings={} can_continue={}",
        report.summary.checks_total,
        report.summary.errors,
        report.summary.warnings,
        report.summary.can_continue
    );
    for check in &report.checks {
        let status = if check.ok { "OK" } else { "FAIL" };
        println!("  - [{:?}/{}] {} — {}", check.severity, status, check.name, check.message);
        if let Some(recommendation) = &check.recommendation {
            println!("    next: {}", recommendation);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_allows_info_only() {
        let checks = vec![PreflightCheck {
            name: "x".to_string(),
            severity: PreflightSeverity::Info,
            ok: true,
            message: "ok".to_string(),
            recommendation: None,
        }];
        let summary = summarize_checks(&checks);
        assert!(summary.can_continue);
        assert_eq!(summary.infos, 1);
    }

    #[test]
    fn summary_blocks_hard_error() {
        let checks = vec![PreflightCheck {
            name: "x".to_string(),
            severity: PreflightSeverity::Error,
            ok: false,
            message: "bad".to_string(),
            recommendation: None,
        }];
        let summary = summarize_checks(&checks);
        assert!(!summary.can_continue);
        assert_eq!(summary.errors, 1);
    }
}
