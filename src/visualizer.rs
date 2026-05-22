use crate::models::IdentityProfile;
use std::fs;

pub fn generate_html_report(profile: &IdentityProfile, output_path: &str) {
    let mut nodes_json = String::from("[");
    let mut edges_json = String::from("[");

    // Корневой узел
    nodes_json.push_str(&format!(
        "{{id: '{}', label: '{}', group: '{:?}'}},",
        profile.root_entity.value,
        profile.root_entity.value,
        profile.root_entity.entity_type
    ));

    // Связанные узлы
    for (value, node) in &profile.associated_nodes {
        nodes_json.push_str(&format!(
            "{{id: '{}', label: '{}', group: '{:?}'}},",
            value, value, node.entity_type
        ));
    }

    // Связи
    for link in &profile.active_links {
        edges_json.push_str(&format!(
            "{{from: '{}', to: '{}', label: '{}'}},",
            link.source_node_value, link.target_node_value, link.metadata.source_id
        ));
    }

    // Убираем последние запятые
    if nodes_json.ends_with(',') {
        nodes_json.pop();
    }
    if edges_json.ends_with(',') {
        edges_json.pop();
    }
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
        body {{ font-family: Arial, sans-serif; margin: 0; }}
        #graph {{ width: 100vw; height: 100vh; border: none; }}
        .info-panel {{
            position: absolute;
            top: 10px;
            left: 10px;
            background: rgba(0,0,0,0.7);
            color: white;
            padding: 10px;
            border-radius: 5px;
            font-size: 14px;
            max-width: 300px;
            display: none;
        }}
    </style>
</head>
<body>
<div id="graph"></div>
<div id="info" class="info-panel"></div>
<script>
    var nodes = new vis.DataSet({nodes_json});
    var edges = new vis.DataSet({edges_json});

    var container = document.getElementById('graph');
    var data = {{ nodes: nodes, edges: edges }};
    var options = {{
        nodes: {{
            shape: 'dot',
            size: 20,
            font: {{ size: 14, color: '#fff' }},
            borderWidth: 2,
            shadow: true,
            color: {{
                background: '#2B7CE9',
                border: '#1A56B8'
            }}
        }},
        edges: {{
            arrows: 'to',
            smooth: {{ type: 'curvedCW', roundness: 0.2 }},
            font: {{ size: 10, align: 'middle' }},
            color: {{ color: '#848484', highlight: '#ff0000' }}
        }},
        groups: {{
            Nickname: {{ color: {{ background: '#FF9900' }} }},
            Email: {{ color: {{ background: '#2B7CE9' }} }},
            Phone: {{ color: {{ background: '#41A317' }} }},
            DateOfBirth: {{ color: {{ background: '#A131B9' }} }},
            FullName: {{ color: {{ background: '#E3319D' }} }}
        }},
        interaction: {{
            hover: true,
            navigationButtons: true,
            keyboard: true
        }}
    }};

    var network = new vis.Network(container, data, options);

    // Показ информации при клике
    network.on('click', function(params) {{
        if (params.nodes.length > 0) {{
            var nodeId = params.nodes[0];
            var node = nodes.get(nodeId);
            var infoDiv = document.getElementById('info');
            infoDiv.innerHTML = '<b>Тип:</b> ' + node.group + '<br><b>Значение:</b> ' + node.label;
            infoDiv.style.display = 'block';
        }} else {{
            document.getElementById('info').style.display = 'none';
        }}
    }});
</script>
</body>
</html>"#
    );

    fs::write(output_path, html).expect("Не удалось записать HTML-отчёт");
}