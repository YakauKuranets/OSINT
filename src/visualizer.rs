use crate::models::IdentityProfile;
use std::fs;

pub fn generate_html_report(profile: &IdentityProfile, output_path: &str) {
    let mut nodes_json = String::from("[");
    let mut edges_json = String::from("[");

    nodes_json.push_str(&format!(
        "{{id:'{}',label:'{}',group:'{:?}',title:'Root: {}'}},",
        js_escape(&profile.root_entity.value),
        js_escape(&profile.root_entity.value),
        profile.root_entity.entity_type,
        js_escape(&profile.root_entity.value)
    ));

    for (value, node) in &profile.associated_nodes {
        nodes_json.push_str(&format!(
            "{{id:'{}',label:'{}',group:'{:?}',title:'Type: {:?}'}},",
            js_escape(value),
            js_escape(value),
            node.entity_type,
            node.entity_type
        ));
    }

    for link in &profile.active_links {
        edges_json.push_str(&format!(
            "{{from:'{}',to:'{}',label:'{}',title:'source={} | class={:?} | year={} | weight={}'}},",
            js_escape(&link.source_node_value),
            js_escape(&link.target_node_value),
            js_escape(&link.metadata.source_id),
            js_escape(&link.metadata.source_id),
            link.metadata.class,
            link.metadata.data_actual_year,
            link.weight_modifier
        ));
    }

    if nodes_json.ends_with(',') { nodes_json.pop(); }
    if edges_json.ends_with(',') { edges_json.pop(); }
    nodes_json.push(']');
    edges_json.push(']');

    let html = format!(r#"<!DOCTYPE html>
<html lang="ru">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>X-GEN OSINT Dashboard</title>
<script src="https://unpkg.com/vis-network/standalone/umd/vis-network.min.js"></script>
<style>
:root {{ --bg:#0b1020; --panel:#111827; --panel2:#1f2937; --text:#e5e7eb; --muted:#9ca3af; --line:rgba(148,163,184,.25); --green:#34d399; --yellow:#fbbf24; --red:#fb7185; --blue:#38bdf8; }}
* {{ box-sizing:border-box; }}
body {{ margin:0; font-family:Segoe UI,Arial,sans-serif; background:var(--bg); color:var(--text); overflow:hidden; }}
.app {{ display:grid; grid-template-columns:460px 1fr; height:100vh; width:100vw; }}
.sidebar {{ overflow:auto; padding:16px; border-right:1px solid var(--line); background:linear-gradient(180deg,#0f172a,#111827); }}
h1 {{ font-size:18px; margin:0 0 4px; }}
small {{ color:var(--muted); }}
.cards {{ display:grid; grid-template-columns:1fr 1fr; gap:10px; margin:14px 0; }}
.card,.row {{ background:var(--panel); border:1px solid var(--line); border-radius:14px; padding:12px; }}
.card h2 {{ margin:0 0 8px; font-size:12px; color:var(--muted); }}
.metric {{ font-size:24px; font-weight:800; }}
.section {{ margin-top:12px; }}
.section-title {{ color:var(--muted); font-size:12px; text-transform:uppercase; letter-spacing:.08em; margin-bottom:8px; }}
.list {{ display:grid; gap:8px; }}
.row strong {{ display:block; font-size:13px; margin-bottom:4px; }}
.row span,.row code {{ color:var(--muted); font-size:12px; overflow-wrap:anywhere; }}
.ok {{ color:var(--green); }} .warn {{ color:var(--yellow); }} .bad {{ color:var(--red); }}
.tag {{ display:inline-block; margin:2px 4px 2px 0; padding:2px 6px; border-radius:999px; background:rgba(148,163,184,.14); border:1px solid rgba(148,163,184,.22); font-size:11px; color:var(--muted); }}
.badtag {{ color:var(--red); border-color:rgba(251,113,133,.35); background:rgba(251,113,133,.12); }}
#graph {{ width:100%; height:100vh; background:radial-gradient(circle at top left,rgba(56,189,248,.12),transparent 35%),#090e1a; }}
.info-panel {{ position:absolute; top:14px; left:14px; max-width:460px; background:rgba(2,6,23,.88); padding:14px; border-radius:14px; border:1px solid var(--line); display:none; }}
.graph-wrap {{ position:relative; min-width:0; }}
button {{ background:#2563eb; color:white; border:0; padding:8px 12px; border-radius:10px; margin-top:10px; cursor:pointer; }}
.mono {{ font-family:ui-monospace,Consolas,monospace; }}
@media(max-width:980px) {{ body{{overflow:auto}} .app{{grid-template-columns:1fr;height:auto}} #graph{{height:70vh}} }}
</style>
</head>
<body>
<div class="app">
<aside class="sidebar">
<h1>X-GEN OSINT Dashboard</h1>
<small>Graph + runtime reports</small>
<div class="cards">
<div class="card"><h2>Узлы</h2><div class="metric" id="mNodes">—</div></div>
<div class="card"><h2>Связи</h2><div class="metric" id="mEdges">—</div></div>
<div class="card"><h2>Confidence</h2><div class="metric" id="mConfidence">—</div></div>
<div class="card"><h2>Conflicts</h2><div class="metric" id="mConflicts">—</div></div>
</div>
<div class="section"><div class="section-title">Master Verdict</div><div class="list" id="masterBox"><div class="row"><span>Загрузка master_report.json…</span></div></div></div>
<div class="section"><div class="section-title">Phone Intel</div><div class="list" id="phoneBox"><div class="row"><span>Загрузка phone_intel_report.json…</span></div></div></div>
<div class="section"><div class="section-title">Autopilot</div><div class="list" id="autopilotBox"><div class="row"><span>Загрузка autopilot_report.json…</span></div></div></div>
<div class="section"><div class="section-title">Email / Domain</div><div class="list" id="emailBox"><div class="row"><span>Загрузка email_domain_report.json…</span></div></div></div>
<div class="section"><div class="section-title">Discovery / Noise</div><div class="list" id="discoveryBox"><div class="row"><span>Загрузка discovery_report.json…</span></div></div></div>
<div class="section"><div class="section-title">Public Search / Noise</div><div class="list" id="searchBox"><div class="row"><span>Загрузка public_search_report.json…</span></div></div></div>
<div class="section"><div class="section-title">Confidence Guardrails</div><div class="list" id="confidenceBox"><div class="row"><span>Загрузка confidence_report.json…</span></div></div></div>
<div class="section"><div class="section-title">Conflicts</div><div class="list" id="conflictBox"><div class="row"><span>Загрузка conflict_report.json…</span></div></div></div>
</aside>
<main class="graph-wrap"><div id="graph"></div><div id="info" class="info-panel"></div></main>
</div>
<script>
const nodes = new vis.DataSet({nodes_json});
const edges = new vis.DataSet({edges_json});
document.getElementById('mNodes').textContent = nodes.length;
document.getElementById('mEdges').textContent = edges.length;
const network = new vis.Network(document.getElementById('graph'), {{nodes, edges}}, {{
  nodes: {{shape:'dot', size:18, font:{{color:'#e5e7eb'}}, borderWidth:2}},
  edges: {{color:{{color:'rgba(148,163,184,.5)'}}, font:{{color:'#9ca3af', size:10}}, smooth:true}},
  physics: {{stabilization:true, barnesHut:{{gravitationalConstant:-26000, springLength:130}}}},
  groups: {{Nickname:{{color:'#f59e0b'}},Username:{{color:'#f59e0b'}},Email:{{color:'#38bdf8'}},Phone:{{color:'#34d399'}},FullName:{{color:'#e879f9'}},Url:{{color:'#a78bfa'}},Domain:{{color:'#60a5fa'}},Country:{{color:'#facc15'}},DateOfBirth:{{color:'#fb7185'}},DataSource:{{color:'#94a3b8'}}}}
}});
network.on('click', p => {{
  if(!p.nodes.length) return;
  const id = p.nodes[0];
  const safe = String(id).replaceAll('\\','\\\\').replaceAll('`','\\`').replaceAll("'","\\'");
  const box = document.getElementById('info');
  box.style.display = 'block';
  box.innerHTML = `<strong>Узел:</strong><br><span class="mono">${{escapeHtml(id)}}</span><br><button onclick="expandNode('${{safe}}')">🔍 Искать связи</button>`;
}});
function expandNode(nodeId) {{ fetch('/expand', {{method:'POST', headers:{{'Content-Type':'application/json'}}, body:JSON.stringify({{target:nodeId}})}}).then(r=>{{if(r.ok) alert('Запрос отправлен. Обнови страницу после завершения.')}}); }}
async function loadJson(path) {{ try {{ const r = await fetch(path, {{cache:'no-store'}}); if(!r.ok) throw new Error(r.status); return await r.json(); }} catch(e) {{ return null; }} }}
function escapeHtml(v) {{ return String(v ?? '').replace(/[&<>'"]/g, c => ({{'&':'&amp;','<':'&lt;','>':'&gt;',"'":'&#39;','"':'&quot;'}}[c])); }}
function n(v) {{ return Number.isFinite(Number(v)) ? Number(v).toLocaleString('ru-RU') : '—'; }}
function row(t,b,cls='') {{ return `<div class="row"><strong class="${{cls}}">${{escapeHtml(t)}}</strong><span>${{b}}</span></div>`; }}
function tags(values,bad=false) {{ return (values||[]).map(v=>`<span class="tag ${{bad?'badtag':''}}">${{escapeHtml(v)}}</span>`).join('') || '<span class="tag">none</span>'; }}
Promise.all(['master_report.json','phone_intel_report.json','autopilot_report.json','email_domain_report.json','discovery_report.json','public_search_report.json','confidence_report.json','conflict_report.json'].map(loadJson))
.then(([master, phone, auto, email, discovery, search, confidence, conflicts]) => {{ renderMaster(master); renderPhone(phone); renderAutopilot(auto); renderEmail(email); renderDiscovery(discovery); renderSearch(search); renderConfidence(confidence); renderConflicts(conflicts); }});
function renderMaster(r) {{ const b=document.getElementById('masterBox'); if(!r){{b.innerHTML=row('Нет данных','master_report.json не найден','warn');return;}} const v=r.verdict||{{}}, s=r.summary||{{}}; b.innerHTML=row(v.status||'unknown',`confidence=${{v.confidence_adjusted??'—'}} | high_risk=${{v.high_risk??false}} | missing=${{n((r.missing_reports||[]).length)}}`,v.high_risk?'bad':'warn'); b.innerHTML+=row('Pipeline',`profile=${{escapeHtml(s.run_profile||'—')}} | phone=${{n(s.phone_checked)}} | phone_carrier=${{n(s.phone_carrier_guesses)}} | auto_new=${{n(s.autopilot_new_nodes)}} | discovery=${{n(s.discovery_findings)}} | public=${{n(s.public_search_findings)}}`); if((v.reasons||[]).length)b.innerHTML+=row('Reasons',tags(v.reasons,true)); }}
function renderPhone(r) {{ const b=document.getElementById('phoneBox'); if(!r){{b.innerHTML=row('Нет данных','phone_intel_report.json не найден','warn');return;}} const s=r.stats||{{}}; b.innerHTML=row('Phone summary',`checked=${{n(s.phones_checked)}} | valid=${{n(s.valid_shape)}} | carrier=${{n(s.carrier_guesses)}} | terms=${{n(s.search_terms_generated)}} | linked=${{n(s.linked_entities)}}`); for(const p of (r.phones||[]).slice(0,4)) {{ b.innerHTML+=row(p.e164||p.digits||p.raw,`raw=<code>${{escapeHtml(p.raw)}}</code><br>country=${{escapeHtml(p.country_guess||'—')}} | valid=${{p.valid_shape}}<br>notes=${{tags(p.notes||[])}}`,p.valid_shape?'ok':'warn'); }} for(const g of (r.carrier_guesses||[]).slice(0,4)) {{ b.innerHTML+=row(`Carrier guess ${{g.confidence}}%`,`country=${{escapeHtml(g.country)}} | operator=${{escapeHtml(g.operator||'unknown')}} | type=${{escapeHtml(g.number_type)}}<br>${{escapeHtml(g.reason)}}`,'warn'); }} }}
function renderAutopilot(r) {{ const b=document.getElementById('autopilotBox'); if(!r){{b.innerHTML=row('Нет данных','autopilot_report.json не найден','warn');return;}} const phoneNew=(r.cycles||[]).reduce((s,c)=>s+Number(c.new_phone_intel_nodes||0),0); b.innerHTML=row('Сводка',`cycles=${{n(r.cycles?.length)}} | initial=${{n(r.initial_seed_count)}} | final=${{n(r.final_seed_count)}} | new=${{n(r.total_new_nodes)}} | phone_new=${{n(phoneNew)}}`); for(const c of (r.cycles||[]).slice(0,4)) b.innerHTML+=row(`Cycle ${{c.cycle}}`,`input=${{n(c.input_seed_count)}} | phone=${{n(c.new_phone_intel_nodes)}} | email=${{n(c.new_email_domain_nodes)}} | discovery=${{n(c.new_discovery_nodes)}} | search=${{n(c.new_public_search_nodes)}} | total=${{n(c.total_seed_count_after_cycle)}}`); }}
function renderEmail(r) {{ const b=document.getElementById('emailBox'); if(!r){{b.innerHTML=row('Нет данных','email_domain_report.json не найден','warn');return;}} const s=r.stats||{{}}; b.innerHTML=row('Email/domain',`emails=${{n(s.emails_checked)}} | valid=${{n(s.valid_emails)}} | domains=${{n(s.domains_checked)}} | candidates=${{n(s.username_candidates)}} | suspicious=${{n(s.suspicious_domains)}}`); for(const d of (r.dns_summaries||[]).slice(0,5)) b.innerHTML+=row(d.domain||'domain',`provider=${{escapeHtml(d.provider_class||'unknown')}} | MX=${{d.has_mx?'yes':'no'}} | TXT=${{d.has_txt?'yes':'no'}}<br>risk=${{tags(d.risk_flags||[],true)}}`,(d.risk_flags||[]).length?'warn':'ok'); }}
function renderDiscovery(r) {{ const b=document.getElementById('discoveryBox'); if(!r){{b.innerHTML=row('Нет данных','discovery_report.json не найден','warn');return;}} const s=r.stats||{{}}; b.innerHTML=row('Discovery',`tasks=${{n(s.tasks_planned)}} | fetched=${{n(s.tasks_fetched)}} | findings=${{n(s.findings_count)}} | blocked=${{n(s.blocked_by_noise_rules)}} | downranked=${{n(s.downranked_by_noise_rules)}} | errors=${{n(s.fetch_errors)}}`); }}
function renderSearch(r) {{ const b=document.getElementById('searchBox'); if(!r){{b.innerHTML=row('Нет данных','public_search_report.json не найден','warn');return;}} const s=r.stats||{{}}; b.innerHTML=row('Public search',`tasks=${{n(s.tasks_planned)}} | executed=${{n(s.tasks_executed)}} | findings=${{n(s.findings_count)}} | blocked=${{n(s.blocked_by_noise_rules)}} | downranked=${{n(s.downranked_by_noise_rules)}}`); }}
function renderConfidence(r) {{ const b=document.getElementById('confidenceBox'); if(!r){{b.innerHTML=row('Нет данных','confidence_report.json не найден','warn');return;}} document.getElementById('mConfidence').textContent=`${{r.adjusted_score??'—'}}%`; b.innerHTML=row('Score',`original=${{r.original_score}} | adjusted=${{r.adjusted_score}} | sources=${{n(r.unique_sources)}} | classes=${{n(r.unique_source_classes)}}`); }}
function renderConflicts(r) {{ const b=document.getElementById('conflictBox'); if(!r){{b.innerHTML=row('Нет данных','conflict_report.json не найден','warn');return;}} const f=r.findings||[]; document.getElementById('mConflicts').textContent=f.length; b.innerHTML=row('Conflict Engine',`findings=${{n(f.length)}} | severity=${{n(r.severity_score)}} | high_risk=${{r.high_risk??false}}`,f.length?'bad':'ok'); for(const x of f.slice(0,4)) b.innerHTML+=row(`${{x.severity||'Conflict'}} / ${{x.kind||''}}`,escapeHtml(x.message||x.entity_value||'')); }}
</script>
</body>
</html>"#, nodes_json = nodes_json, edges_json = edges_json);

    fs::write(output_path, html).expect("Не удалось записать HTML-отчёт");
}

fn js_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'").replace('\n', " ").replace('\r', " ")
}
