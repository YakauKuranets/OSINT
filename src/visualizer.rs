use crate::models::IdentityProfile;
use std::fs;

pub fn generate_html_report(profile: &IdentityProfile, output_path: &str) {
    let mut nodes_json = String::from("[");
    let mut edges_json = String::from("[");

    nodes_json.push_str(&format!(
        "{{id: '{}', label: '{}', group: '{:?}'}},",
        profile.root_entity.value, profile.root_entity.value, profile.root_entity.entity_type
    ));

    for (value, node) in &profile.associated_nodes {
        nodes_json.push_str(&format!(
            "{{id: '{}', label: '{}', group: '{:?}'}},",
            value, value, node.entity_type
        ));
    }

    for link in &profile.active_links {
        edges_json.push_str(&format!(
            "{{from: '{}', to: '{}', label: '{}'}},",
            link.source_node_value, link.target_node_value, link.metadata.source_id
        ));
    }

    if nodes_json.ends_with(',') { nodes_json.pop(); }
    if edges_json.ends_with(',') { edges_json.pop(); }
    nodes_json.push(']');
    edges_json.push(']');

    let html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>X-GEN OSINT Graph</title>
    <script src="https://unpkg.com/vis-network/standalone/umd/vis-network.min.js"></script>
    <style>
        body {{ font-family: Arial, sans-serif; margin: 0; background: #1a1a1a; color: white; }}
        #graph {{ width: 100vw; height: 90vh; }}
        .info-panel {{ position: absolute; top: 10px; left: 10px; background: rgba(0,0,0,0.8); padding: 15px; border-radius: 8px; border: 1px solid #444; }}
        button {{ background: #2B7CE9; color: white; border: none; padding: 8px 12px; cursor: pointer; border-radius: 4px; margin-top: 10px; }}
        button:hover {{ background: #1A56B8; }}
    </style>
</head>
<body>
<div id="graph"></div>
<div id="info" class="info-panel" style="display:none;"></div>
<script>
    var nodes = new vis.DataSet({nodes_json});
    var edges = new vis.DataSet({edges_json});
    var container = document.getElementById('graph');
    var data = {{ nodes: nodes, edges: edges }};
    var options = {{
        nodes: {{ shape: 'dot', size: 20, font: {{ color: '#fff' }} }},
        groups: {{
            Nickname: {{ color: '#FF9900' }}, Email: {{ color: '#2B7CE9' }},
            Phone: {{ color: '#41A317' }}, FullName: {{ color: '#E3319D' }}
        }}
    }};
    var network = new vis.Network(container, data, options);

    network.on('click', function(params) {{
        if (params.nodes.length > 0) {{
            var id = params.nodes[0];
            var infoDiv = document.getElementById('info');
            infoDiv.style.display = 'block';
            infoDiv.innerHTML = '<b>Узел:</b> ' + id + '<br><button onclick="expandNode(\'' + id + '\')">🔍 Искать связи</button>';
        }}
    }});

    function expandNode(nodeId) {{
        fetch('/expand', {{
            method: 'POST',
            headers: {{ 'Content-Type': 'application/json' }},
            body: JSON.stringify({{ target: nodeId }})
        }}).then(response => {{
            if(response.ok) alert('Запрос на поиск связей для ' + nodeId + ' отправлен!');
        }});
    }}
</script>
</body>
</html>"#,
        nodes_json = nodes_json,
        edges_json = edges_json
    );

    fs::write(output_path, html).expect("Не удалось записать HTML-отчёт");
}