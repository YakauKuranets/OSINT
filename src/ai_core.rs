use reqwest::Client;
use serde_json::{Value, json};
use std::time::Duration;

/// Ядро управления локальными нейросетями X-GEN
pub struct AiCore {
    http_client: Client,
    api_url: String,
}

impl AiCore {
    pub fn new() -> Self {
        Self {
            // Устанавливаем долгий таймаут (5 минут), так как тяжелые модели (Mistral) могут "думать"
            http_client: Client::builder()
                .timeout(Duration::from_secs(300))
                .build()
                .unwrap(),
            api_url: "http://127.0.0.1:11434/api/generate".to_string(), // Стандартный API Ollama
        }
    }

    /// Базовый метод общения с локальной LLM
    async fn ask_model(&self, model: &str, prompt: &str, require_json: bool) -> Option<String> {
        let mut payload = json!({
            "model": model,
            "prompt": prompt,
            "stream": false
        });

        // Форсируем вывод строго в JSON формате (поддерживается в Ollama)
        if require_json {
            payload["format"] = json!("json");
        }

        match self.http_client.post(&self.api_url).json(&payload).send().await {
            Ok(resp) if resp.status().is_success() => {
                let json_resp: Value = resp.json().await.unwrap_or_default();
                json_resp["response"].as_str().map(|s| s.to_string())
            }
            Err(e) => {
                eprintln!("[!] Ошибка связи с локальным ИИ ({}): {}", model, e);
                None
            }
            _ => None,
        }
    }

    /// ⚡ РОЛЬ: "Аналитик" (Phi-3:mini)
    /// Быстрое извлечение сущностей (NER) из сырого грязного текста
    pub async fn analyst_extract_entities(&self, raw_text: &str) -> Option<Value> {
        let prompt = format!(
            "Task: Extract all names, emails, phones, and nicknames from the following text. \
             Output strictly in JSON format like {{ \"emails\": [], \"phones\": [], \"nicknames\": [] }}. \
             Text to analyze: {}",
            raw_text
        );

        println!("  [AI Analyst] Запуск Phi-3:mini для потокового извлечения данных...");
        let response = self.ask_model("phi3:mini", &prompt, true).await?;
        serde_json::from_str(&response).ok()
    }

    /// 🕵️ РОЛЬ: "Следователь" (Mistral:7b)
    /// Глубокий анализ профиля и подготовка структурированного отчета
    pub async fn investigator_summarize(&self, profile_data: &str) -> Option<String> {
        let prompt = format!(
            "Ты — опытный аналитик киберразведки (Threat Intelligence). Проанализируй следующие извлеченные данные \
             о цели. Напиши краткое, профессиональное психологическое и поведенческое резюме объекта СТРОГО НА РУССКОМ ЯЗЫКЕ. \
             Выдели потенциальные риски, привычки, паттерны поведения и дай рекомендации по дальнейшему поиску. \
             ВЕСЬ ОТВЕТ ДОЛЖЕН БЫТЬ НА РУССКОМ ЯЗЫКЕ. \
             Данные цели: {}",
            profile_data
        );

        println!("  [AI Investigator] Запуск Mistral:7b для глубокой аналитики профиля...");
        // Здесь JSON не форсируем, нам нужен красивый связный текст
        self.ask_model("mistral", &prompt, false).await
    }
}