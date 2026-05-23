use crate::models::IdentityProfile;
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize)]
struct CaseSnapshot<'a> {
    case_id: String,
    created_at: u64,
    profile: &'a IdentityProfile,
}

pub fn persist_case_snapshot(profile: &IdentityProfile) -> Option<String> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    let case_id = format!("case-{}", now);
    let snapshot = CaseSnapshot {
        case_id: case_id.clone(),
        created_at: now,
        profile,
    };

    let json = serde_json::to_string_pretty(&snapshot).ok()?;
    let _ = std::fs::create_dir_all("cases");
    let path = format!("cases/{}.json", case_id);
    std::fs::write(path, json).ok()?;
    Some(case_id)
}

