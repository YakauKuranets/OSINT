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

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="ru">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>X-GEN OSINT Dashboard</title>
    <script src="https://unpkg.com/vis-network/standalone/umd/vis-network.min.js"></script>
    <style>
        :root {{
            --bg: #0b1020;
            --panel: rgba(17, 24, 39, 0.94);
            --panel2: rgba(31, 41, 55, 0.88);
            --text: #e5e7eb;
            --muted: #9ca3af;
            --line: rgba(148, 163, 184, 0.25);
            --blue: #38bdf8;
            --green: #34d399;
            --yellow: #fbbf24;
            --red: #fb7185;
        }}
        * {{ box-sizing: border-box; }}
        body {{ margin: 0; font-family: Inter, Segoe UI, Arial, sans-serif; background: var(--bg); color: var(--text); overflow: hidden; }}
        .app {{ display: grid; grid-template-columns: 420px 1fr; width: 100vw; height: 100vh; }}
        .sidebar {{ overflow: auto; padding: 16px; background: linear-gradient(180deg, rgba(15,23,42,0.98), rgba(15,23,42,0.9)); border-right: 1px solid var(--line); }}
        .brand {{ display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 14px; }}
        .brand h1 {{ font-size: 18px; line-height: 1.2; margin: 0; }}
        .brand small {{ color: var(--muted); }}
        .pill {{ padding: 4px 8px; border-radius: 999px; background: rgba(56,189,248,0.12); border: 1px solid rgba(56,189,248,0.35); color: var(--blue); font-size: 12px; white-space: nowrap; }}
        .cards {{ display: grid; grid-template-columns: 1fr 1fr; gap: 10px; margin-bottom: 12px; }}
        .card {{ background: var(--panel); border: 1px solid var(--line); border-radius: 14px; padding: 12px; box-shadow: 0 12px 30px rgba(0,0,0,0.25); }}
        .card h2 {{ margin: 0 0 8px; font-size: 13px; color: var(--muted); font-weight: 600; }}
        .metric {{ font-size: 24px; font-weight: 800; }}
        .metric.small {{ font-size: 18px; }}
        .section {{ margin-top: 12px; }}
        .section-title {{ display: flex; justify-content: space-between; align-items: center; margin: 0 0 8px; font-size: 13px; color: var(--muted); text-transform: uppercase; letter-spacing: 0.06em; }}
        .list {{ display: grid; gap: 8px; }}
        .row {{ background: var(--panel2); border: 1px solid var(--line); border-radius: 12px; padding: 10px; }}
        .row strong {{ display: block; font-size: 13px; margin-bottom: 4px; }}
        .row span, .row code {{ color: var(--muted); font-size: 12px; overflow-wrap: anywhere; }}
        .ok {{ color: var(--green); }} .warn {{ color: var(--yellow); }} .bad {{ color: var(--red); }}
        .graph-wrap {{ position: relative; min-width: 0; }}
        #graph {{ width: 100%; height: 100vh; background: radial-gradient(circle at top left, rgba(56,189,248,0.12), transparent 35%), #090e1a; }}
        .info-panel {{ position: absolute; top: 14px; left: 14px; max-width: 460px; background: rgba(2,6,23,0.88); padding: 14px; border-radius: 14px; border: 1px solid var(--line); backdrop-filter: blur(10px); display: none; }}
        button {{ background: #2563eb; color: white; border: none; padding: 8px 12px; cursor: pointer; border-radius: 10px; margin-top: 10px; font-weight: 700; }}
        button:hover {{ background: #1d4ed8; }}
        .mono {{ font-family: ui-monospace, SFMono-Regular, Consolas, monospace; }}
        .footer-note {{ margin-top: 12px; color: var(--muted); font-size: 12px; line-height: 1.45; }}
        @media (max-width: 980px) {{
            body {{ overflow: auto; }}
            .app {{ grid-template-columns: 1fr; height: auto; }}
            .sidebar {{ height: auto; max-height: none; }}
            #graph {{ height: 70vh; }}
        }}
    </style>
</head>
<body>
<div class="app">
    <aside class="sidebar">
        <div class="brand">
            <div>
                <h1>X-GEN OSINT Dashboard</h1>
                <small>Graph + pipeline reports</small>
            </div>
            <div class="pill">v3.0</div>
        </div>

        <div class="cards">
            <div class="card"><h2>Узлы</h2><div class="metric" id="mNodes">—</div></div>
            <div class="card"><h2>Связи</h2><div class="metric" id="mEdges">—</div></div>
            <div class="card"><h2>Confidence</h2><div class="metric" id="mConfidence">—</div></div>
            <div class="card"><h2>Conflicts</h2><div class="metric" id="mConflicts">—</div></div>
        </div>

        <div class="section">
            <div class="section-title">Autopilot</div>
            <div class="list" id="autopilotBox"><div class="row"><span>Загрузка autopilot_report.json…</span></div></div>
        </div>

        <div class="section">
            <div class="section-title">Email / Domain</div>
            <div class="list" id="emailBox"><div class="row"><span>Загрузка email_domain_report.json…</span></div></div>
        </div>

        <div class="section">
            <div class="section-title">Public Search / Noise</div>
            <div class="list" id="searchBox"><div class="row"><span>Загрузка public_search_report.json…</span></div></div>
        </div>

        <div class="section">
            <div class="section-title">Confidence Guardrails</div>
            <div class="list" id="confidenceBox"><div class="row"><span>Загрузка confidence_report.json…</span></div></div>
        </div>

        <div class="section">
            <div class="section-title">Conflicts</div>
            <div class="list" id="conflictBox"><div class="row"><span>Загрузка conflict_report.json…</span></div></div>
        </div>

        <div class="footer-note">
            Отчёт показывает только локально сгенерированные JSON-файлы рядом с report.html. Если блоки не грузятся, открой отчёт через сервер <span class="mono">http://127.0.0.1:3000</span>, а не как file://.
        </div>
    </aside>

    <main class="graph-wrap">
        <div id="graph"></div>
        <div id="info" class="info-panel"></div>
    </main>
</div>

<script>
    const nodes = new vis.DataSet({nodes_json});
    const edges = new vis.DataSet({edges_json});
    document.getElementById('mNodes').textContent = nodes.length;
    document.getElementById('mEdges').textContent = edges.length;

    const container = document.getElementById('graph');
    const data = {{ nodes, edges }};
    const options = {{
        nodes: {{ shape: 'dot', size: 18, font: {{ color: '#e5e7eb' }}, borderWidth: 2 }},
        edges: {{ color: {{ color: 'rgba(148,163,184,0.5)' }}, font: {{ color: '#9ca3af', size: 10 }}, smooth: true }},
        physics: {{ stabilization: true, barnesHut: {{ gravitationalConstant: -26000, springLength: 130 }} }},
        groups: {{
            Nickname: {{ color: '#f59e0b' }}, Username: {{ color: '#f59e0b' }},
            Email: {{ color: '#38bdf8' }}, Phone: {{ color: '#34d399' }},
            FullName: {{ color: '#e879f9' }}, Url: {{ color: '#a78bfa' }}, Domain: {{ color: '#60a5fa' }},
            Country: {{ color: '#facc15' }}, DateOfBirth: {{ color: '#fb7185' }}
        }}
    }};
    const network = new vis.Network(container, data, options);

    network.on('click', function(params) {{
        if (params.nodes.length > 0) {{
            const id = params.nodes[0];
            const infoDiv = document.getElementById('info');
            infoDiv.style.display = 'block';
            infoDiv.innerHTML = `<strong>Узел:</strong><br><span class="mono">${{escapeHtml(id)}}</span><br><button onclick="expandNode('${{String(id).replaceAll('\\', '\\\\').replaceAll('`', '\\`').replaceAll("'", "\\'")}}')">🔍 Искать связи</button>`;
        }}
    }});

    function expandNode(nodeId) {{
        fetch('/expand', {{ method: 'POST', headers: {{ 'Content-Type': 'application/json' }}, body: JSON.stringify({{ target: nodeId }}) }})
            .then(response => {{ if(response.ok) alert('Запрос на поиск связей для ' + nodeId + ' отправлен. Обнови страницу после завершения.'); }});
    }}

    async function loadJson(path) {{
        try {{
            const res = await fetch(path, {{ cache: 'no-store' }});
            if (!res.ok) throw new Error(res.status + ' ' + res.statusText);
            return await res.json();
        }} catch (e) {{ return null; }}
    }}

    function row(title, body, cls='') {{ return `<div class="row"><strong class="${{cls}}">${{escapeHtml(title)}}</strong><span>${{body}}</span></div>`; }}
    function escapeHtml(v) {{ return String(v ?? '').replace(/[&<>'"]/g, c => ({{'&':'&amp;','<':'&lt;','>':'&gt;',"'":'&#39;','"':'&quot;'}}[c])); }}
    function n(v) {{ return Number.isFinite(Number(v)) ? Number(v).toLocaleString('ru-RU') : '—'; }}

    Promise.all([
        loadJson('autopilot_report.json'),
        loadJson('email_domain_report.json'),
        loadJson('public_search_report.json'),
        loadJson('confidence_report.json'),
        loadJson('conflict_report.json')
    ]).then(([auto, email, search, confidence, conflicts]) => {{
        renderAutopilot(auto);
        renderEmail(email);
        renderSearch(search);
        renderConfidence(confidence);
        renderConflicts(conflicts);
    }});

    function renderAutopilot(r) {{
        const box = document.getElementById('autopilotBox');
        if (!r) {{ box.innerHTML = row('Нет данных', 'autopilot_report.json не найден', 'warn'); return; }}
        box.innerHTML = row('Сводка', `cycles=${{n(r.cycles?.length)}} | initial=${{n(r.initial_seed_count)}} | final=${{n(r.final_seed_count)}} | new=${{n(r.total_new_nodes)}}`);
        for (const c of (r.cycles || []).slice(0, 4)) {{
            box.innerHTML += row(`Cycle ${{c.cycle}}`, `input=${{n(c.input_seed_count)}} | discovery_new=${{n(c.new_discovery_nodes)}} | search_new=${{n(c.new_public_search_nodes)}} | total=${{n(c.total_seed_count_after_cycle)}}`);
        }}
    }}

    function renderEmail(r) {{
        const box = document.getElementById('emailBox');
        if (!r) {{ box.innerHTML = row('Нет данных', 'email_domain_report.json не найден', 'warn'); return; }}
        const s = r.stats || {{}};
        box.innerHTML = row('Email/domain', `emails=${{n(s.emails_checked)}} | valid=${{n(s.valid_emails)}} | domains=${{n(s.domains_checked)}} | candidates=${{n(s.username_candidates)}} | dns_errors=${{n(s.dns_errors)}}`);
        for (const d of (r.dns_summaries || []).slice(0, 4)) {{
            box.innerHTML += row(d.domain || 'domain', `MX=${{d.has_mx ? 'yes' : 'no'}} | TXT=${{d.has_txt ? 'yes' : 'no'}} | mx_hosts=${{escapeHtml((d.mx_hosts || []).slice(0,2).join(', '))}}`);
        }}
    }}

    function renderSearch(r) {{
        const box = document.getElementById('searchBox');
        if (!r) {{ box.innerHTML = row('Нет данных', 'public_search_report.json не найден', 'warn'); return; }}
        const s = r.stats || {{}};
        box.innerHTML = row('Public search', `tasks=${{n(s.tasks_planned)}} | executed=${{n(s.tasks_executed)}} | findings=${{n(s.findings_count)}} | blocked=${{n(s.blocked_by_noise_rules)}} | downranked=${{n(s.downranked_by_noise_rules)}}`);
        for (const f of (r.findings || []).slice(0, 5)) {{
            box.innerHTML += row(`${{f.entity_type}} / ${{f.confidence}}%`, `<code>${{escapeHtml(f.value)}}</code><br>${{escapeHtml(f.note || '')}}`);
        }}
    }}

    function renderConfidence(r) {{
        const box = document.getElementById('confidenceBox');
        if (!r) {{ box.innerHTML = row('Нет данных', 'confidence_report.json не найден', 'warn'); return; }}
        document.getElementById('mConfidence').textContent = `${{r.adjusted_score ?? '—'}}%`;
        document.getElementById('mConfidence').className = 'metric ' + ((r.adjusted_score || 0) >= 75 ? 'ok' : (r.adjusted_score || 0) >= 50 ? 'warn' : 'bad');
        box.innerHTML = row('Score', `original=${{r.original_score}} | adjusted=${{r.adjusted_score}} | sources=${{n(r.unique_sources)}} | classes=${{n(r.unique_source_classes)}}`);
        for (const g of (r.applied_guardrails || []).slice(0, 6)) {{
            box.innerHTML += row(`${{g.kind}} cap=${{g.cap}}`, escapeHtml(g.reason), 'warn');
        }}
    }}

    function renderConflicts(r) {{
        const box = document.getElementById('conflictBox');
        if (!r) {{ box.innerHTML = row('Нет данных', 'conflict_report.json не найден', 'warn'); document.getElementById('mConflicts').textContent = '—'; return; }}
        const findings = r.findings || [];
        document.getElementById('mConflicts').textContent = findings.length;
        document.getElementById('mConflicts').className = 'metric ' + (findings.length ? 'bad' : 'ok');
        box.innerHTML = row('Conflict Engine', `findings=${{n(findings.length)}} | severity=${{n(r.severity_score)}} | high_risk=${{r.high_risk ?? r.has_high_risk ?? false}}`);
        for (const f of findings.slice(0, 5)) {{ box.innerHTML += row(`${{f.severity || 'Conflict'}} / ${{f.kind || ''}}`, escapeHtml(f.message || f.entity_value || '')); }}
    }}
</script>
</body>
</html>"#,
        nodes_json = nodes_json,
        edges_json = edges_json
    );

    fs::write(output_path, html).expect("Не удалось записать HTML-отчёт");
}

fn js_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', " ")
        .replace('\r', " ")
}
