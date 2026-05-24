pub fn inject_manual_review_gate_block(report_path: &str) -> Result<(), String> {
    let mut html = std::fs::read_to_string(report_path)
        .map_err(|err| format!("read {}: {}", report_path, err))?;

    if html.contains("id=\"manualGateBox\"") {
        return Ok(());
    }

    let block = r#"
<div class="section"><div class="section-title">Manual Review Gate</div><div class="list" id="manualGateBox"><div class="row"><span>Загрузка manual_review_gate_report.json…</span></div></div></div>
"#;

    let marker = "<div class=\"section\"><div class=\"section-title\">Phone Intel</div>";
    if html.contains(marker) {
        html = html.replace(marker, &format!("{}{}", block, marker));
    } else {
        let sidebar_marker = "</aside>";
        html = html.replace(sidebar_marker, &format!("{}{}", block, sidebar_marker));
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
    box.innerHTML=row('Operator review cards',`pending=${n(data.pending_count)} | rejected=${n(data.rejected_count)} | questionable=${n(data.questionable_count)} | more_verification=${n(data.more_verification_count)} | probable=${n(data.probable_count)}`,data.pending_count?'warn':(data.probable_count?'ok':''));
    for(const c of cards.slice(0,8)){
      const cls=statusCls(c.decision);
      box.innerHTML+=row(`${c.review_id||'review'} / ${c.decision||'Pending'}`,
        `selector=${escapeHtml(c.selector_type)} <code>${escapeHtml(c.selector_masked)}</code><br>`+
        `source=${escapeHtml(c.source_id)} | stage=${escapeHtml(c.stage)} | source_trust=${escapeHtml(c.source_trust)}<br>`+
        `match=${escapeHtml(c.selector_match_quality)} | date=${c.has_date_hint} | context=${c.has_context_near_selector} | independent=${c.has_independent_allowed_confirmation}<br>`+
        `cap=${n(c.confidence_cap)} | main_confidence=${c.contributes_to_main_confidence} | identity_confirm=${c.identity_confirmation_allowed}<br>`+
        `raw_stored=${c.raw_record_stored} | raw_visible=${c.raw_record_visible}<br>`+
        `steps=${tags((c.required_manual_steps||[]).slice(0,4))}<br>`+
        `rules=${tags((c.decision_rules||[]).slice(0,4))}<br>`+
        `warnings=${tags((c.warnings||[]).slice(0,4),true)}`,
        cls
      );
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
