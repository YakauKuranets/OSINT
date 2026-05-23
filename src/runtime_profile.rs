use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

static RUNTIME_PROFILE: OnceLock<RunProfileConfig> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunProfileName {
    Default,
    Quick,
    Deep,
    Offline,
    Safe,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunProfileConfig {
    pub name: RunProfileName,
    pub label: String,
    pub discovery_max_tasks: usize,
    pub discovery_max_bytes: usize,
    pub discovery_fetch: bool,
    pub public_search_max_tasks: usize,
    pub github_search: bool,
    pub dns_check: bool,
    pub autopilot_cycles: usize,
    pub autopilot_new_limit: usize,
}

impl RunProfileConfig {
    pub fn default_profile() -> Self {
        Self {
            name: RunProfileName::Default,
            label: "default".to_string(),
            discovery_max_tasks: 40,
            discovery_max_bytes: 512 * 1024,
            discovery_fetch: true,
            public_search_max_tasks: 20,
            github_search: true,
            dns_check: true,
            autopilot_cycles: 2,
            autopilot_new_limit: 80,
        }
    }

    pub fn quick() -> Self {
        Self {
            name: RunProfileName::Quick,
            label: "quick".to_string(),
            discovery_max_tasks: 12,
            discovery_max_bytes: 160 * 1024,
            discovery_fetch: true,
            public_search_max_tasks: 8,
            github_search: true,
            dns_check: true,
            autopilot_cycles: 1,
            autopilot_new_limit: 30,
        }
    }

    pub fn deep() -> Self {
        Self {
            name: RunProfileName::Deep,
            label: "deep".to_string(),
            discovery_max_tasks: 120,
            discovery_max_bytes: 1024 * 1024,
            discovery_fetch: true,
            public_search_max_tasks: 80,
            github_search: true,
            dns_check: true,
            autopilot_cycles: 4,
            autopilot_new_limit: 180,
        }
    }

    pub fn offline() -> Self {
        Self {
            name: RunProfileName::Offline,
            label: "offline".to_string(),
            discovery_max_tasks: 0,
            discovery_max_bytes: 64 * 1024,
            discovery_fetch: false,
            public_search_max_tasks: 0,
            github_search: false,
            dns_check: false,
            autopilot_cycles: 1,
            autopilot_new_limit: 30,
        }
    }

    pub fn safe() -> Self {
        Self {
            name: RunProfileName::Safe,
            label: "safe".to_string(),
            discovery_max_tasks: 20,
            discovery_max_bytes: 256 * 1024,
            discovery_fetch: true,
            public_search_max_tasks: 10,
            github_search: true,
            dns_check: true,
            autopilot_cycles: 1,
            autopilot_new_limit: 40,
        }
    }
}

pub fn init_runtime_profile() -> &'static RunProfileConfig {
    RUNTIME_PROFILE.get_or_init(resolve_runtime_profile)
}

pub fn config() -> &'static RunProfileConfig {
    RUNTIME_PROFILE.get_or_init(resolve_runtime_profile)
}

pub fn discovery_max_tasks() -> usize {
    override_usize("OSINT_DISCOVERY_MAX_TASKS").unwrap_or_else(|| config().discovery_max_tasks)
}

pub fn discovery_max_bytes() -> usize {
    override_usize("OSINT_DISCOVERY_MAX_BYTES").unwrap_or_else(|| config().discovery_max_bytes)
}

pub fn discovery_fetch() -> bool {
    override_bool("OSINT_DISCOVERY_FETCH").unwrap_or_else(|| config().discovery_fetch)
}

pub fn public_search_max_tasks() -> usize {
    override_usize("OSINT_PUBLIC_SEARCH_MAX_TASKS").unwrap_or_else(|| config().public_search_max_tasks)
}

pub fn github_search() -> bool {
    override_bool("OSINT_GITHUB_SEARCH").unwrap_or_else(|| config().github_search)
}

pub fn dns_check() -> bool {
    override_bool("OSINT_DNS_CHECK").unwrap_or_else(|| config().dns_check)
}

pub fn autopilot_cycles() -> usize {
    override_usize("OSINT_AUTOPILOT_CYCLES")
        .unwrap_or_else(|| config().autopilot_cycles)
        .clamp(1, 5)
}

pub fn autopilot_new_limit() -> usize {
    override_usize("OSINT_AUTOPILOT_NEW_LIMIT").unwrap_or_else(|| config().autopilot_new_limit)
}

pub fn save_run_profile_report(path: &str) -> Result<(), String> {
    let json = serde_json::to_string_pretty(config())
        .map_err(|err| format!("serialize run profile: {}", err))?;
    std::fs::write(path, json).map_err(|err| format!("write {}: {}", path, err))
}

fn resolve_runtime_profile() -> RunProfileConfig {
    let profile_name = profile_name_from_args()
        .or_else(|| std::env::var("OSINT_PROFILE").ok())
        .unwrap_or_else(|| "default".to_string());

    match profile_name.trim().to_lowercase().as_str() {
        "quick" | "fast" => RunProfileConfig::quick(),
        "deep" | "full" | "100" => RunProfileConfig::deep(),
        "offline" | "local" => RunProfileConfig::offline(),
        "safe" | "careful" => RunProfileConfig::safe(),
        _ => RunProfileConfig::default_profile(),
    }
}

fn profile_name_from_args() -> Option<String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut idx = 0;
    while idx < args.len() {
        let arg = &args[idx];
        if let Some(value) = arg.strip_prefix("--profile=") {
            return Some(value.to_string());
        }
        if arg == "--profile" {
            return args.get(idx + 1).cloned();
        }
        if matches!(arg.as_str(), "quick" | "deep" | "offline" | "safe" | "default" | "100") {
            return Some(arg.clone());
        }
        idx += 1;
    }
    None
}

fn override_usize(key: &str) -> Option<usize> {
    std::env::var(key).ok()?.parse::<usize>().ok()
}

fn override_bool(key: &str) -> Option<bool> {
    let value = std::env::var(key).ok()?;
    match value.trim().to_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_profile_has_more_cycles_than_quick() {
        assert!(RunProfileConfig::deep().autopilot_cycles > RunProfileConfig::quick().autopilot_cycles);
    }

    #[test]
    fn offline_disables_network_fetches() {
        let offline = RunProfileConfig::offline();
        assert!(!offline.discovery_fetch);
        assert!(!offline.github_search);
        assert!(!offline.dns_check);
    }
}
