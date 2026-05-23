use std::collections::{HashSet, VecDeque};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::Path;
use crate::models::{IdentityProfile, EntityNode, EntityType, SourceMetadata, SourceClass};
use reqwest::Client;
use serde_json::Value;
use crate::ai_core::AiCore;
use crate::data_broker::DataBroker;
use crate::connectors::EmailBreachConnector;
use crate::connectors::PhoneIntelConnector;
use crate::connectors::TelegramConnector;
use crate::connectors::BrokerConnector;

pub struct AnalysisEngine {
    pub task_queue: VecDeque<EntityNode>,
    visited_pool: HashSet<String>,
    pub final_profile: IdentityProfile,
    max_depth: usize,
    steps: usize,
    cached_lines: HashSet<String>,
    sources: Vec<(String, SourceMetadata)>,
    http_client: Client,
    pub ai_core: AiCore,
    data_broker: DataBroker,
    connector_last_run: std::collections::HashMap<String, u64>,
}

impl AnalysisEngine {
    pub fn new(root_entity: EntityNode, dumps_dir: &str) -> Self {
        let mut task_queue = VecDeque::new();
        task_queue.push_back(root_entity.clone());

        let final_profile = IdentityProfile {
            root_entity,
            associated_nodes: std::collections::HashMap::new(),
            active_links: Vec::new(),
            calculated_confidence: 0,
        };

        let mut sources = Vec::new();
        if let Ok(entries) = fs::read_dir(dumps_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                        let source_class = if filename.contains("verified") {
                            SourceClass::VerifiedRegistry
                        } else if filename.contains("public") {
                            SourceClass::PublicOSINT
                        } else {
                            SourceClass::UnverifiedDump
                        };
                        let metadata = SourceMetadata {
                            source_id: filename.to_string(),
                            class: source_class,
                            import_timestamp: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs(),
                            data_actual_year: 2026,
                        };
                        sources.push((filename.to_string(), metadata));
                    }
                }
            }
        }

        let http_client = Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap();

        AnalysisEngine {
            task_queue,
            visited_pool: HashSet::new(),
            final_profile,
            max_depth: 15,
            steps: 0,
            cached_lines: HashSet::new(),
            sources,
            http_client,
            ai_core: AiCore::new(),
            data_broker: DataBroker::new("brokers.json"),
            connector_last_run: std::collections::HashMap::new(),
        }
    }

    fn should_run_connector(&mut self, connector_id: &str, now: u64) -> bool {
        let interval = crate::connectors::ThrottlePolicy::interval_for_connector(connector_id);
        match self.connector_last_run.get(connector_id) {
            Some(prev) if now.saturating_sub(*prev) < interval => {
                println!(
                    "  [Throttle] {} skipped (elapsed={}s < interval={}s)",
                    connector_id,
                    now.saturating_sub(*prev),
                    interval
                );
                false
            }
            _ => {
                self.connector_last_run.insert(connector_id.to_string(), now);
                println!("  [Throttle] {} allowed (interval={}s)", connector_id, interval);
                true
            }
        }
    }

    fn normalize_for_search(value: &str, entity_type: &EntityType) -> String {
        match entity_type {
            EntityType::Phone => value.chars().filter(|c| c.is_ascii_digit()).collect(),
            _ => value.to_lowercase(),
        }
    }

    fn to_international(phone: &str) -> String {
        let digits: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.len() == 11 && digits.starts_with("80") {
            format!("375{}", &digits[2..])
        } else {
            digits
        }
    }

    async fn check_hibp(&self, email: &str) -> Vec<String> {
        let url = format!("https://haveibeenpwned.com/api/v3/breachedaccount/{}", email);
        match self.http_client.get(&url).header("User-Agent", "XGEN-OSINT-Engine/3.0").send().await {
            Ok(resp) if resp.status().is_success() => {
                let body = resp.text().await.unwrap_or_default();
                serde_json::from_str::<Vec<Value>>(&body)
                    .unwrap_or_default().iter()
                    .map(|v| v["Name"].as_str().unwrap_or("unknown").to_string())
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    async fn check_phone(&self, phone: &str) -> Vec<String> {
        let api_key = "63c398573905fb4ec3663b3602f9f695";
        let intl_number = Self::to_international(phone);
        let url = format!(
            "http://apilayer.net/api/validate?access_key={}&number={}&country_code=&format=1",
            api_key, intl_number
        );
        match self.http_client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let body = resp.text().await.unwrap_or_default();
                let v: Value = serde_json::from_str(&body).unwrap_or_default();
                let mut info = Vec::new();
                if v["valid"].as_bool() == Some(true) {
                    if let Some(country) = v["country_name"].as_str() { info.push(format!("country:{}", country)); }
                    if let Some(carrier) = v["carrier"].as_str() { info.push(format!("carrier:{}", carrier)); }
                    if let Some(line_type) = v["line_type"].as_str() { info.push(format!("line_type:{}", line_type)); }
                }
                info
            }
            _ => Vec::new(),
        }
    }

    async fn check_telegram(&self, username: &str) -> Vec<String> {
        let url = "http://127.0.0.1:5002/search";
        let payload = serde_json::json!({"username": username});

        match self.http_client.post(url).json(&payload).send().await {
            Ok(resp) if resp.status().is_success() => {
                let body = resp.text().await.unwrap_or_default();
                let v: Value = serde_json::from_str(&body).unwrap_or_default();
                let mut info = Vec::new();
                if let Some(results) = v["results"].as_array() {
                    for r in results {
                        if let Some(tp) = r["type"].as_str() {
                            match tp {
                                "user" => {
                                    let uname = r["username"].as_str().unwrap_or("");
                                    let first = r["first_name"].as_str().unwrap_or("");
                                    let last = r["last_name"].as_str().unwrap_or("");
                                    info.push(format!("tg_user:{} {} (@{})", first, last, uname));
                                    if let Some(phone) = r["phone"].as_str() {
                                        if !phone.is_empty() {
                                            info.push(format!("tg_phone:{}", phone));
                                        }
                                    }
                                }
                                "message" => {
                                    let chat = r["chat_title"].as_str().unwrap_or("");
                                    let text = r["text"].as_str().unwrap_or("");
                                    info.push(format!("tg_msg:{}: {}", chat, text));
                                }
                                "group_membership" => {
                                    let chat = r["chat_title"].as_str().unwrap_or("");
                                    let participants = r["participant_count"].as_u64().unwrap_or(0);
                                    info.push(format!("tg_group:{} ({} members)", chat, participants));
                                }
                                _ => {}
                            }
                        }
                    }
                }
                info
            }
            Err(e) => {
                println!("  [!] Ошибка связи с Telegram API: {}. Проверь, запущен ли Python-скрипт.", e);
                Vec::new()
            }
            _ => {
                println!("  [!] Telegram API вернул неожиданный статус.");
                Vec::new()
            }
        }
    }

    fn json_obj_to_line(&self, obj: &Value) -> Option<String> {
        let email = obj["email"].as_str().unwrap_or("");
        let phone = obj["phone"].as_str().unwrap_or("");
        let nick = obj["nick"].as_str().or(obj["username"].as_str()).unwrap_or("");
        let date = obj["date"].as_str().or(obj["dob"].as_str()).unwrap_or("");
        if email.is_empty() && phone.is_empty() && nick.is_empty() { return None; }
        Some(format!("{};{};{};{}", nick, email, phone, date))
    }

    fn extract_array_from_object(obj: &Value) -> Option<&Vec<Value>> {
        if let Some(arr) = obj.as_array() { return Some(arr); }
        if let Some(map) = obj.as_object() {
            for (_, v) in map { if let Some(arr) = v.as_array() { return Some(arr); } }
        }
        None
    }

    fn collect_relevant_lines(&mut self, target_value: &str, target_type: &EntityType) -> Vec<(String, SourceMetadata)> {
        let normalized_target = Self::normalize_for_search(target_value, target_type);
        let mut results = Vec::new();

        for (filename, metadata) in &self.sources {
            let path = Path::new("dumps").join(filename);
            if !filename.ends_with(".json") {
                if let Ok(file) = File::open(&path) {
                    let reader = BufReader::new(file);
                    for line_result in reader.lines() {
                        if let Ok(line) = line_result {
                            if line.len() > 10 * 1024 * 1024 { continue; }
                            let found = match target_type {
                                EntityType::Phone => line.chars().filter(|c| c.is_ascii_digit()).collect::<String>().contains(&normalized_target),
                                _ => line.to_lowercase().contains(&normalized_target),
                            };
                            if found && !self.cached_lines.contains(&line) {
                                self.cached_lines.insert(line.clone());
                                results.push((line, metadata.clone()));
                            }
                        }
                    }
                }
            } else {
                if let Ok(file) = File::open(&path) {
                    let reader = BufReader::new(file);
                    if let Ok(json_val) = serde_json::from_reader::<_, Value>(reader) {
                        if let Some(arr) = Self::extract_array_from_object(&json_val) {
                            for obj in arr {
                                if let Some(obj_map) = obj.as_object() {
                                    if let Some(line) = self.json_obj_to_line(&Value::Object(obj_map.clone())) {
                                        let found = match target_type {
                                            EntityType::Phone => line.chars().filter(|c| c.is_ascii_digit()).collect::<String>().contains(&normalized_target),
                                            _ => line.to_lowercase().contains(&normalized_target),
                                        };
                                        if found && !self.cached_lines.contains(&line) {
                                            self.cached_lines.insert(line.clone());
                                            results.push((line, metadata.clone()));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        results
    }

    pub async fn resolve_cascade(&mut self) {
        while let Some(current_node) = self.task_queue.pop_front() {
            self.steps += 1;
            if self.steps > self.max_depth {
                println!("[Engine] Достигнут лимит глубины поиска ({})", self.max_depth);
                break;
            }

            let normalized_value = Self::normalize_for_search(&current_node.value, &current_node.entity_type);
            if self.visited_pool.contains(&normalized_value) { continue; }
            self.visited_pool.insert(normalized_value.clone());

            println!("[Engine] Поиск связей для: [{:?}] {} (нормализовано: {})",
                     current_node.entity_type, current_node.value, normalized_value);

            // Social Spider + AI
            if current_node.entity_type == EntityType::Nickname {
                println!("  [Social Spider] Запуск охоты на профили для {}", current_node.value);
                let sites = crate::social_spider::get_default_sites();
                let found_profiles = crate::social_spider::hunt_social_profiles(&self.http_client, &current_node.value, &sites).await;
                for (node, meta) in found_profiles {
                    self.final_profile.associated_nodes.insert(node.value.clone(), node.clone());
                    self.final_profile.active_links.push(crate::models::EntityLink {
                        source_node_value: current_node.value.clone(),
                        target_node_value: node.value.clone(),
                        weight_modifier: 15,
                        metadata: meta.clone(),
                    });

                    if node.value.starts_with("telegram_bio:") || node.value.starts_with("pinterest_bio:") || node.value.starts_with("github_bio:") {
                        println!("  [AI Analyst] Перехват сырого текста. Phi-3:mini ищет скрытые контакты...");
                        if let Some(ai_json) = self.ai_core.analyst_extract_entities(&node.value).await {
                            if let Some(emails) = ai_json["emails"].as_array() {
                                for e in emails.iter().filter_map(|v| v.as_str()) {
                                    println!("    [+] Phi-3 нашел скрытую почту: {}", e);
                                    let new_node = EntityNode { value: e.to_string(), entity_type: EntityType::Email, first_seen: meta.import_timestamp };
                                    if !self.visited_pool.contains(&Self::normalize_for_search(e, &EntityType::Email)) {
                                        self.task_queue.push_back(new_node.clone());
                                    }
                                    self.final_profile.associated_nodes.insert(e.to_string(), new_node);
                                }
                            }
                            if let Some(phones) = ai_json["phones"].as_array() {
                                for p in phones.iter().filter_map(|v| v.as_str()) {
                                    println!("    [+] Phi-3 нашел скрытый телефон: {}", p);
                                    let new_node = EntityNode { value: p.to_string(), entity_type: EntityType::Phone, first_seen: meta.import_timestamp };
                                    if !self.visited_pool.contains(&Self::normalize_for_search(p, &EntityType::Phone)) {
                                        self.task_queue.push_back(new_node.clone());
                                    }
                                    self.final_profile.associated_nodes.insert(p.to_string(), new_node);
                                }
                            }
                        }
                    }
                }
            }

            // Telegram Deep Scan
            if current_node.entity_type == EntityType::Nickname || current_node.entity_type == EntityType::Phone {
                let tg_now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                if self.should_run_connector("telegram", tg_now) {
                let search_value = if current_node.entity_type == EntityType::Phone { &normalized_value } else { &current_node.value };
                println!("  [Telegram] Запрос для {} (ожидание может занять до 30 сек)...", search_value);
                let tg_info = self.check_telegram(search_value).await;
                if !tg_info.is_empty() {
                    let tg_connector = TelegramConnector;
                    let tg_meta = SourceMetadata {
                        source_id: "Telegram_API".to_string(),
                        class: SourceClass::PublicOSINT,
                        import_timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
                        data_actual_year: 2026,
                    };
                    let observations = tg_connector.collect_telegram_info(&tg_info, tg_meta.import_timestamp);
                    for obs in observations {
                        if obs.entity_type == EntityType::Phone {
                            println!("    [+] 📱 ТЕЛЕГРАМ РАСКРЫЛ НОМЕР ТЕЛЕФОНА: {}", obs.value);
                        }
                        let node = obs.to_entity_node();
                        if (node.entity_type == EntityType::Phone || node.entity_type == EntityType::Email)
                            && !self.visited_pool.contains(&Self::normalize_for_search(&node.value, &node.entity_type)) {
                            self.task_queue.push_back(node.clone());
                        }
                        self.final_profile.associated_nodes.insert(node.value.clone(), node.clone());
                        self.final_profile.active_links.push(crate::models::EntityLink {
                            source_node_value: current_node.value.clone(),
                            target_node_value: node.value,
                            weight_modifier: if node.entity_type == EntityType::Phone { 30 } else { 10 },
                            metadata: tg_meta.clone(),
                        });
                    }
                }
            }
            }

            // Email обработка (HIBP + DataBroker)
            if current_node.entity_type == EntityType::Email {
                let email_now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                if self.should_run_connector("email_breach", email_now) {
                let breaches = self.check_hibp(&current_node.value).await;
                if !breaches.is_empty() {
                    let email_connector = EmailBreachConnector;
                    let hibp_meta = SourceMetadata {
                        source_id: "HIBP_API".to_string(),
                        class: SourceClass::VerifiedRegistry,
                        import_timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
                        data_actual_year: 2026,
                    };
                    let observations = email_connector.collect_breaches(&current_node.value, &breaches, hibp_meta.import_timestamp);
                    for obs in observations {
                        let node = obs.to_entity_node();
                        let mut obs_meta = hibp_meta.clone();
                        obs_meta.source_id = obs.source_tag();
                        self.final_profile.associated_nodes.insert(node.value.clone(), node.clone());
                        self.final_profile.active_links.push(crate::models::EntityLink {
                            source_node_value: current_node.value.clone(),
                            target_node_value: node.value,
                            weight_modifier: 20,
                            metadata: obs_meta,
                        });
                    }
                }

                println!("  [DataBroker] Запрос к теневым источникам для {}", current_node.value);
                let broker_results = self.data_broker.query("email", &current_node.value).await;
                let broker_connector = BrokerConnector;
                for result in broker_results {
                    let observations = broker_connector.collect_nodes(&result.nodes, &result.source_meta.source_id, result.source_meta.import_timestamp);
                    for obs in observations {
                        let node = obs.to_entity_node();
                        println!("    [Broker] Найдена сущность: {} ({:?})", node.value, node.entity_type);
                        if !self.visited_pool.contains(&Self::normalize_for_search(&node.value, &node.entity_type)) {
                            self.task_queue.push_back(node.clone());
                        }
                        let mut obs_meta = result.source_meta.clone();
                        obs_meta.source_id = obs.source_tag();
                        self.final_profile.associated_nodes.insert(node.value.clone(), node.clone());
                        self.final_profile.active_links.push(crate::models::EntityLink {
                            source_node_value: current_node.value.clone(),
                            target_node_value: node.value,
                            weight_modifier: 30,
                            metadata: obs_meta,
                        });
                    }
                }
            }
            }

            // Phone обработка (NumVerify + DataBroker)
            if current_node.entity_type == EntityType::Phone {
                let phone_now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                if self.should_run_connector("phone_intel", phone_now) {
                let phone_info = self.check_phone(&current_node.value).await;
                if !phone_info.is_empty() {
                    let phone_connector = PhoneIntelConnector;
                    let phone_meta = SourceMetadata {
                        source_id: "NumVerify_API".to_string(),
                        class: SourceClass::VerifiedRegistry,
                        import_timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
                        data_actual_year: 2026,
                    };
                    let observations = phone_connector.collect_phone_traits(&phone_info, phone_meta.import_timestamp);
                    for obs in observations {
                        let node = obs.to_entity_node();
                        let mut obs_meta = phone_meta.clone();
                        obs_meta.source_id = obs.source_tag();
                        self.final_profile.associated_nodes.insert(node.value.clone(), node.clone());
                        self.final_profile.active_links.push(crate::models::EntityLink {
                            source_node_value: current_node.value.clone(),
                            target_node_value: node.value,
                            weight_modifier: 15,
                            metadata: obs_meta,
                        });
                    }
                }

                println!("  [DataBroker] Запрос к теневым источникам для {}", current_node.value);
                let broker_results = self.data_broker.query("phone", &current_node.value).await;
                let broker_connector = BrokerConnector;
                for result in broker_results {
                    let observations = broker_connector.collect_nodes(&result.nodes, &result.source_meta.source_id, result.source_meta.import_timestamp);
                    for obs in observations {
                        let node = obs.to_entity_node();
                        println!("    [Broker] Найдена сущность: {} ({:?})", node.value, node.entity_type);
                        if !self.visited_pool.contains(&Self::normalize_for_search(&node.value, &node.entity_type)) {
                            self.task_queue.push_back(node.clone());
                        }
                        let mut obs_meta = result.source_meta.clone();
                        obs_meta.source_id = obs.source_tag();
                        self.final_profile.associated_nodes.insert(node.value.clone(), node.clone());
                        self.final_profile.active_links.push(crate::models::EntityLink {
                            source_node_value: current_node.value.clone(),
                            target_node_value: node.value,
                            weight_modifier: 30,
                            metadata: obs_meta,
                        });
                    }
                }
            }
            }

            // Локальные дампы
            let relevant_lines = self.collect_relevant_lines(&current_node.value, &current_node.entity_type);
            for (line, source_meta) in &relevant_lines {
                let (discovered_nodes, discovered_links) = crate::parser::parse_raw_line(line, source_meta);
                for node in discovered_nodes {
                    if node.value != self.final_profile.root_entity.value {
                        if node.entity_type == EntityType::Phone || node.entity_type == EntityType::Email {
                            if !self.visited_pool.contains(&Self::normalize_for_search(&node.value, &node.entity_type)) {
                                self.task_queue.push_back(node.clone());
                            }
                        }
                        self.final_profile.associated_nodes.insert(node.value.clone(), node);
                    }
                }
                for link in discovered_links {
                    let is_duplicate = self.final_profile.active_links.iter().any(|existing| {
                        existing.source_node_value == link.source_node_value && existing.target_node_value == link.target_node_value && existing.metadata.source_id == link.metadata.source_id
                    });
                    if !is_duplicate { self.final_profile.active_links.push(link); }
                }
            }

            crate::scoring::evaluate_profile(&mut self.final_profile);
            println!("[Engine] Текущая достоверность досье: {}%", self.final_profile.calculated_confidence);
        }
        println!("[Engine] Каскадный анализ завершён.");
    }
}
