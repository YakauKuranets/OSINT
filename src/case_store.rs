use crate::models::IdentityProfile;
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize)]
struct CaseSnapshot<'a> {
    case_id: String,
    created_at: u64,
    profile: &'a IdentityProfile,
}

#[derive(Serialize, serde::Deserialize, Clone)]
struct CaseIndexEntry {
    case_id: String,
    created_at: u64,
    root_value: String,
    confidence: u8,
    links: usize,
}

fn update_case_index(entry: CaseIndexEntry) {
    let index_path = "cases/index.json";
    let mut items: Vec<CaseIndexEntry> = std::fs::read_to_string(index_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Vec<CaseIndexEntry>>(&raw).ok())
        .unwrap_or_default();

    items.push(entry);
    if let Ok(json) = serde_json::to_string_pretty(&items) {
        let _ = std::fs::write(index_path, json);
    }
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

    update_case_index(CaseIndexEntry {
        case_id: case_id.clone(),
        created_at: now,
        root_value: profile.root_entity.value.clone(),
        confidence: profile.calculated_confidence,
        links: profile.active_links.len(),
    });

    Some(case_id)
}
