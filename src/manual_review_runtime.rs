#[path = "assisted_verification_flow.rs"]
mod assisted_verification_flow;

use crate::manual_review_gate::{
    build_manual_review_gate_report, create_manual_review_card, save_manual_review_gate_report,
    ManualReviewGateCard,
};
use crate::models::{EntityNode, EntityType};
use assisted_verification_flow::{
    build_phone_assisted_verification_flow, validate_assisted_verification_flow,
    AssistedFlowValidationReport, AssistedPlatform, AssistedVerificationFlow,
};
use std::sync::Once;
use std::time::Duration;

static DASHBOARD_PATCHER: Once = Once::new();

pub fn build_and_save_manual_review_gate_for_seeds(seeds: &[EntityNode], path: &str) -> Result<usize, String> {
    let mut cards = Vec::new();
    for seed in seeds {
        if let Some(card) = card_for_seed(seed) {
            cards.push(card);
        }
    }
    let count = cards.len();
    let report = build_manual_review_gate_report(cards);
    save_manual_review_gate_report(&report, path)?;
    enrich_manual_review_gate_report_with_assisted_flows(path, seeds)?;
    save_standalone_assisted_verification_report(seeds, "assisted_verification_flow.json")?;
    start_dashboard_patcher_once();
    Ok(count)
}

fn enrich_manual_review_gate_report_with_assisted_flows(path: &str, seeds: &[EntityNode]) -> Result<(), String> {
    let flows = assisted_flows_for_phone_seeds(seeds);
    let validations = validation_reports_for_flows(&flows);
    let summary = assisted_validation_summary(&validations);
    let raw = std::fs::read_to_string(path).map_err(|err| format!("read {}: {}", path, err))?;
    let mut value: serde_json::Value = serde_json::from_str(&raw).map_err(|err| format!("parse {}: {}", path, err))?;
    if let Some(obj) = value.as_object_mut() {
        obj.insert("assisted_verification_flow_count".to_string(), serde_json::json!(flows.len()));
        obj.insert("assisted_verification_platforms".to_string(), serde_json::json!(["Telegram", "Viber", "WhatsApp", "VK", "MAX"]));
        obj.insert("assisted_verification_flows".to_string(), serde_json::to_value(&flows).map_err(|err| format!("serialize assisted flows: {}", err))?);
        obj.insert("assisted_verification_flow_validations".to_string(), serde_json::to_value(&validations).map_err(|err| format!("serialize assisted validations: {}", err))?);
        obj.insert("assisted_verification_validation".to_string(), summary);
        obj.insert("assisted_verification_policy".to_string(), serde_json::json!({
            "mode": "assisted_verification_not_hidden_probe",
            "raw_data_stored": false,
            "automated_account_discovery": false,
            "operator_decision_required": true,
            "identity_confirmation_allowed": false
        }));
    }
    let updated = serde_json::to_string_pretty(&value).map_err(|err| format!("serialize {}: {}", path, err))?;
    std::fs::write(path, updated).map_err(|err| format!("write {}: {}", path, err))
}

fn save_standalone_assisted_verification_report(seeds: &[EntityNode], path: &str) -> Result<(), String> {
    let flows = assisted_flows_for_phone_seeds(seeds);
    let validations = validation_reports_for_flows(&flows);
    let summary = assisted_validation_summary(&validations);
    let value = serde_json::json!({
        "flow_count": flows.len(),
        "platforms": ["Telegram", "Viber", "WhatsApp", "VK", "MAX"],
        "mode": "assisted_verification_not_hidden_probe",
        "raw_data_stored": false,
        "automated_account_discovery": false,
        "identity_confirmation_allowed": false,
        "validation": summary,
        "flow_validations": validations,
        "flows": flows
    });
    let json = serde_json::to_string_pretty(&value).map_err(|err| format!("serialize {}: {}", path, err))?;
    std::fs::write(path, json).map_err(|err| format!("write {}: {}", path, err))
}

fn assisted_flows_for_phone_seeds(seeds: &[EntityNode]) -> Vec<AssistedVerificationFlow> {
    seeds
        .iter()
        .filter(|seed| matches!(seed.entity_type, EntityType::Phone))
        .map(|seed| build_phone_assisted_verification_flow(&seed.value))
        .collect()
}

fn validation_reports_for_flows(flows: &[AssistedVerificationFlow]) -> Vec<AssistedFlowValidationReport> {
    flows.iter().map(validate_assisted_verification_flow).collect()
}

fn assisted_validation_summary(validations: &[AssistedFlowValidationReport]) -> serde_json::Value {
    let flow_count = validations.len();
    let ready_count = validations.iter().filter(|v| v.ready).count();
    let readiness_percent = if validations.is_empty() {
        100
    } else {
        validations.iter().map(|v| v.readiness_percent as usize).min().unwrap_or(0)
    };
    let raw_data_violations: usize = validations.iter().map(|v| v.raw_data_violations).sum();
    let automated_discovery_violations: usize = validations.iter().map(|v| v.automated_discovery_violations).sum();
    let identity_confirmation_violations: usize = validations.iter().map(|v| v.identity_confirmation_violations).sum();
    let forbidden_action_violations: usize = validations.iter().map(|v| v.forbidden_action_violations).sum();
    let missing_platforms = validations
        .iter()
        .flat_map(|v| v.missing_platforms.clone())
        .collect::<Vec<_>>();

    serde_json::json!({
        "ready": readiness_percent == 100 && raw_data_violations == 0 && automated_discovery_violations == 0 && identity_confirmation_violations == 0 && forbidden_action_violations == 0,
        "readiness_percent": readiness_percent,
        "flow_count": flow_count,
        "ready_count": ready_count,
        "expected_platforms": ["Telegram", "Viber", "WhatsApp", "VK", "MAX"],
        "missing_platforms": missing_platforms,
        "raw_data_violations": raw_data_violations,
        "automated_discovery_violations": automated_discovery_violations,
        "identity_confirmation_violations": identity_confirmation_violations,
        "forbidden_action_violations": forbidden_action_violations,
        "status": if readiness_percent == 100 { "ready" } else { "needs_fix" }
    })
}

fn start_dashboard_patcher_once() {
    DASHBOARD_PATCHER.call_once(|| {
        std::thread::spawn(|| {
            for _ in 0..180 {
                if std::path::Path::new("report.html").exists() {
                    let _ = inject_manual_review_gate_block("report.html");
                    break;
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        });
    });
}

fn inject_manual_review_gate_block(report_path: &str) -> Result<(), String> {
    let mut html = std::fs::read_to_string(report_path)
        .map_err(|err| format!("read {}: {}", report_path, err))?;
    if html.contains("id=\"manualGateBox\"") {
        return Ok(());
    }

    let block = r#"<div class="section"><div class="section-title">Manual Review Gate</div><div class="list" id="manualGateBox"><div class="row"><span>Загрузка manual_review_gate_report.json…</span></div></div></div>"#;
    let marker = "<div class=\"section\"><div class=\"section-title\">Phone Intel</div>";
    if html.contains(marker) {
        html = html.replace(marker, &format!("{}{}", block, marker));
    } else {
        html = html.replace("</aside>", &format!("{}{}", block, "</aside>"));
    }

    let script = r#"
async function loadManualReviewGate(){
  const box=document.getElementById('manualGateBox');
  if(!box)return;
  try{
    const r=await fetch('manual_review_gate_report.json',{cache:'no-store'});
    if(!r.ok)throw new Error(r.status);
    const data=await r.json();
    const cards=data.cards||[];
    const flows=data.assisted_verification_flows||[];
    const validation=data.assisted_verification_validation||{};
    const flowValidations=data.assisted_verification_flow_validations||[];
    box.innerHTML=row('Operator review cards',`pending=${n(data.pending_count)} | rejected=${n(data.rejected_count)} | questionable=${n(data.questionable_count)} | more_verification=${n(data.more_verification_count)} | probable=${n(data.probable_count)}`,data.pending_count?'warn':(data.probable_count?'ok':''));
    box.innerHTML+=row('Assisted verification readiness',`ready=${validation.ready===true} | readiness=${n(validation.readiness_percent)}% | status=${escapeHtml(validation.status||'unknown')}<br>`+
      `flows=${n(data.assisted_verification_flow_count||flows.length)} | ready_flows=${n(validation.ready_count)} | platforms=${tags(data.assisted_verification_platforms||['Telegram','Viber','WhatsApp','VK','MAX'])}<br>`+
      `raw_violations=${n(validation.raw_data_violations)} | auto_discovery_violations=${n(validation.automated_discovery_violations)} | identity_violations=${n(validation.identity_confirmation_violations)} | guardrail_violations=${n(validation.forbidden_action_violations)}<br>`+
      `mode=${escapeHtml(data.assisted_verification_policy?.mode||'assisted_verification_not_hidden_probe')}`,
      validation.ready===true?'ok':'bad');
    for(const v of flowValidations.slice(0,4)){
      box.innerHTML+=row(`Flow validation / ${v.ready?'ready':'needs_fix'}`,
        `readiness=${n(v.readiness_percent)}% | profiles=${n(v.platform_profile_count)} | steps=${n(v.total_steps)} | messenger_steps=${n(v.messenger_steps_present)}/${n(v.messenger_steps_expected)}<br>`+
        `missing=${tags(v.missing_platforms||[],true)}<br>`+
        `raw=${n(v.raw_data_violations)} | auto=${n(v.automated_discovery_violations)} | identity=${n(v.identity_confirmation_violations)} | guardrails=${n(v.forbidden_action_violations)}<br>`+
        `findings=${tags((v.findings||[]).slice(0,5))}`,
        v.ready?'ok':'bad');
    }
    for(const f of flows.slice(0,4)){
      const platformNames=[...new Set((f.platform_profiles||[]).map(p=>p.display_name).filter(Boolean))];
      const ready=(f.steps||[]).filter(s=>s.status==='ReadyForOperator').length;
      box.innerHTML+=row(`Assisted flow / ${f.decision||'Pending'}`,
        `selector=<code>${escapeHtml(f.selector_masked)}</code> | steps=${n((f.steps||[]).length)} | ready=${n(ready)}<br>`+
        `platforms=${tags(platformNames)}<br>`+
        `cap=${n(f.confidence_cap)} | main_confidence=${f.contributes_to_main_confidence} | identity_confirm=${f.identity_confirmation_allowed} | raw_data=${f.raw_data_stored} | auto_discovery=${f.automated_account_discovery}<br>`+
        `warnings=${tags((f.global_warnings||[]).slice(0,4),true)}`,
        statusCls(f.decision));
    }
    for(const c of cards.slice(0,8)){
      box.innerHTML+=row(`${c.review_id||'review'} / ${c.decision||'Pending'}`,
        `selector=${escapeHtml(c.selector_type)} <code>${escapeHtml(c.selector_masked)}</code><br>`+
        `source=${escapeHtml(c.source_id)} | stage=${escapeHtml(c.stage)} | source_trust=${escapeHtml(c.source_trust)}<br>`+
        `match=${escapeHtml(c.selector_match_quality)} | date=${c.has_date_hint} | context=${c.has_context_near_selector} | independent=${c.has_independent_allowed_confirmation}<br>`+
        `cap=${n(c.confidence_cap)} | main_confidence=${c.contributes_to_main_confidence} | identity_confirm=${c.identity_confirmation_allowed}<br>`+
        `raw_stored=${c.raw_record_stored} | raw_visible=${c.raw_record_visible}<br>`+
        `steps=${tags((c.required_manual_steps||[]).slice(0,4))}<br>`+
        `rules=${tags((c.decision_rules||[]).slice(0,4))}<br>`+
        `warnings=${tags((c.warnings||[]).slice(0,4),true)}`,
        statusCls(c.decision));
    }
  }catch(e){
    box.innerHTML=row('Нет данных','manual_review_gate_report.json не найден или недоступен','warn');
  }
}
loadManualReviewGate();
"#;

    if html.contains("</script>") {
        html = html.replacen("</script>", &format!("{}\n</script>", script), 1);
    } else {
        html.push_str(&format!("<script>{}</script>", script));
    }

    std::fs::write(report_path, html)
        .map_err(|err| format!("write {}: {}", report_path, err))
}

fn card_for_seed(seed: &EntityNode) -> Option<ManualReviewGateCard> {
    let selector_type = match seed.entity_type {
        EntityType::Phone => "phone",
        EntityType::Email => "email",
        EntityType::Username | EntityType::Nickname => "username",
        EntityType::FullName => "full_name",
        _ => return None,
    };
    let masked = mask_selector(&seed.value, &seed.entity_type);
    let review_id = format!(
        "review_{}_{}",
        selector_type,
        stable_short_id(&masked)
    );
    Some(create_manual_review_card(
        &review_id,
        selector_type,
        &masked,
        "manual_operator_review",
    ))
}

fn mask_selector(value: &str, entity_type: &EntityType) -> String {
    match entity_type {
        EntityType::Phone => mask_phone(value),
        EntityType::Email => mask_email(value),
        EntityType::Username | EntityType::Nickname => mask_username(value),
        EntityType::FullName => mask_full_name(value),
        _ => "selector_masked".to_string(),
    }
}

fn mask_phone(value: &str) -> String {
    let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 6 {
        return "phone_masked".to_string();
    }
    let prefix_len = digits.len().min(4);
    let suffix_len = 2usize.min(digits.len().saturating_sub(prefix_len));
    format!("+{}***{}", &digits[..prefix_len], &digits[digits.len() - suffix_len..])
}

fn mask_email(value: &str) -> String {
    let clean = value.trim().to_lowercase();
    let Some((local, domain)) = clean.split_once('@') else {
        return "email_masked".to_string();
    };
    let local_head = local.chars().next().unwrap_or('*');
    let domain_tail = domain.rsplit('.').next().unwrap_or("domain");
    format!("{}***@***.{}", local_head, domain_tail)
}

fn mask_username(value: &str) -> String {
    let clean = value.trim().trim_start_matches('@');
    if clean.len() <= 2 {
        return "user***".to_string();
    }
    let first = clean.chars().next().unwrap_or('u');
    let last = clean.chars().last().unwrap_or('r');
    format!("@{}***{}", first, last)
}

fn mask_full_name(value: &str) -> String {
    let parts = value.split_whitespace().collect::<Vec<_>>();
    if parts.is_empty() {
        return "name_masked".to_string();
    }
    let initials = parts
        .iter()
        .filter_map(|p| p.chars().next())
        .take(3)
        .collect::<String>();
    format!("{}***", initials)
}

fn stable_short_id(value: &str) -> String {
    let mut hash = 5381u64;
    for byte in value.as_bytes() {
        hash = ((hash << 5).wrapping_add(hash)).wrapping_add(*byte as u64);
    }
    format!("{:x}", hash)[..8].to_string()
}

fn assisted_platform_names(flow: &AssistedVerificationFlow) -> Vec<String> {
    let mut out = flow
        .steps
        .iter()
        .map(|step| match &step.platform {
            AssistedPlatform::Telegram => "Telegram".to_string(),
            AssistedPlatform::Viber => "Viber".to_string(),
            AssistedPlatform::WhatsApp => "WhatsApp".to_string(),
            AssistedPlatform::Vk => "VK".to_string(),
            AssistedPlatform::Max => "MAX".to_string(),
            AssistedPlatform::PublicWeb => "PublicWeb".to_string(),
            AssistedPlatform::Other(value) => value.clone(),
        })
        .collect::<Vec<_>>();
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_masks_phone_and_creates_card() {
        let seeds = vec![EntityNode { value: "+000000000000".to_string(), entity_type: EntityType::Phone, first_seen: 1 }];
        let card = card_for_seed(&seeds[0]).expect("card");
        assert_eq!(card.selector_type, "phone");
        assert!(card.selector_masked.contains("***"));
        assert!(!card.raw_record_stored);
        assert!(!card.raw_record_visible);
    }

    #[test]
    fn assisted_flow_for_phone_seed_contains_max() {
        let seeds = vec![EntityNode { value: "+000000000000".to_string(), entity_type: EntityType::Phone, first_seen: 1 }];
        let flows = assisted_flows_for_phone_seeds(&seeds);
        assert_eq!(flows.len(), 1);
        let names = assisted_platform_names(&flows[0]);
        assert!(names.contains(&"MAX".to_string()));
        assert!(flows[0].steps.iter().all(|step| !step.raw_data_stored));
        assert!(flows[0].steps.iter().all(|step| !step.automated_account_discovery));
    }

    #[test]
    fn assisted_validation_summary_is_ready_for_generated_phone_flow() {
        let seeds = vec![EntityNode { value: "+000000000000".to_string(), entity_type: EntityType::Phone, first_seen: 1 }];
        let flows = assisted_flows_for_phone_seeds(&seeds);
        let validations = validation_reports_for_flows(&flows);
        let summary = assisted_validation_summary(&validations);
        assert_eq!(summary.get("ready").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(summary.get("readiness_percent").and_then(|v| v.as_u64()), Some(100));
        assert_eq!(summary.get("raw_data_violations").and_then(|v| v.as_u64()), Some(0));
        assert_eq!(summary.get("automated_discovery_violations").and_then(|v| v.as_u64()), Some(0));
        assert_eq!(summary.get("identity_confirmation_violations").and_then(|v| v.as_u64()), Some(0));
        assert_eq!(summary.get("forbidden_action_violations").and_then(|v| v.as_u64()), Some(0));
    }
}
