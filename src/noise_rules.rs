use crate::models::EntityType;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoiseAction {
    Allow,
    Downrank,
    Block,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoiseDecision {
    pub action: NoiseAction,
    pub confidence_delta: i16,
    pub reason: String,
}

impl NoiseDecision {
    pub fn allow() -> Self {
        Self { action: NoiseAction::Allow, confidence_delta: 0, reason: "allowed".to_string() }
    }

    pub fn downrank(delta: i16, reason: impl Into<String>) -> Self {
        Self { action: NoiseAction::Downrank, confidence_delta: -delta.abs(), reason: reason.into() }
    }

    pub fn block(reason: impl Into<String>) -> Self {
        Self { action: NoiseAction::Block, confidence_delta: -100, reason: reason.into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoiseDecisionInput {
    pub source_id: String,
    pub note: String,
    pub entity_type: EntityType,
    pub value: String,
    pub url: Option<String>,
    pub confidence: u8,
}

pub fn evaluate_noise(input: &NoiseDecisionInput) -> NoiseDecision {
    let source_id = input.source_id.to_lowercase();
    let note = input.note.to_lowercase();
    let value = input.value.trim();
    let value_lower = value.to_lowercase();
    let url_lower = input.url.as_deref().unwrap_or_default().to_lowercase();

    if value.is_empty() {
        return NoiseDecision::block("empty value");
    }

    if value_lower.starts_with("seed_") || value_lower.contains("seed_nickname:") || value_lower.contains("seed_email:") {
        return NoiseDecision::block("seed pseudo-selector must not become evidence");
    }

    if value_lower.contains("[redacted]") || value_lower.contains("[secret-redacted]") {
        return NoiseDecision::block("redacted value must not expand graph");
    }

    if matches!(input.entity_type, EntityType::Username | EntityType::Nickname) {
        if !is_valid_username_like(value) {
            return NoiseDecision::block("invalid username-like value");
        }
    }

    if matches!(input.entity_type, EntityType::Url) && !is_safe_public_url(value) {
        return NoiseDecision::block("unsafe or non-public URL");
    }

    if source_id.contains("viber") || note.contains("viber") {
        return NoiseDecision::block("Viber web mentions are too noisy for identity confirmation");
    }

    if source_id.contains("github_public_repo_search") && note.contains("github_repo_owner") {
        return NoiseDecision::downrank(25, "repo owner from search result is weak correlation");
    }

    if source_id.contains("github_public_repo_search") && note.contains("github_repo_search_result") {
        return NoiseDecision::downrank(15, "repo search URL is mention-level evidence only");
    }

    if source_id.contains("planned_web_search_query") {
        return NoiseDecision::block("planned search query is not evidence until executed by a search adapter");
    }

    if source_id.contains("mastodon") && !url_lower.contains("mastodon.social/@") {
        return NoiseDecision::block("mastodon result without canonical profile URL");
    }

    if source_id.contains("pinterest") && !url_lower.contains("pinterest.com/") {
        return NoiseDecision::block("pinterest result without canonical profile URL");
    }

    if input.confidence < 35 {
        return NoiseDecision::downrank(10, "very low source confidence");
    }

    NoiseDecision::allow()
}

pub fn adjusted_confidence(confidence: u8, decision: &NoiseDecision) -> Option<u8> {
    match decision.action {
        NoiseAction::Block => None,
        NoiseAction::Allow => Some(confidence),
        NoiseAction::Downrank => {
            let adjusted = confidence as i16 + decision.confidence_delta;
            Some(adjusted.clamp(0, 100) as u8)
        }
    }
}

fn is_valid_username_like(value: &str) -> bool {
    let v = value.trim().trim_start_matches('@');
    if v.len() < 3 || v.len() > 64 || v.contains(':') || v.contains('/') || v.contains('\\') || v.contains(' ') {
        return false;
    }
    v.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
}

fn is_safe_public_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://"))
        && !lower.contains("localhost")
        && !lower.contains("127.0.0.1")
        && !lower.contains("0.0.0.0")
        && !lower.contains("169.254.")
        && !lower.contains(".onion")
        && !lower.starts_with("file:")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(source_id: &str, note: &str, entity_type: EntityType, value: &str) -> NoiseDecisionInput {
        NoiseDecisionInput {
            source_id: source_id.to_string(),
            note: note.to_string(),
            entity_type,
            value: value.to_string(),
            url: None,
            confidence: 50,
        }
    }

    #[test]
    fn blocks_seed_values() {
        let decision = evaluate_noise(&input("test", "note", EntityType::Username, "seed_nickname:@abc"));
        assert_eq!(decision.action, NoiseAction::Block);
    }

    #[test]
    fn downranks_github_repo_owner() {
        let decision = evaluate_noise(&input("github_public_repo_search", "github_repo_owner", EntityType::Username, "owner"));
        assert_eq!(decision.action, NoiseAction::Downrank);
        assert_eq!(adjusted_confidence(45, &decision), Some(20));
    }

    #[test]
    fn blocks_unsafe_urls() {
        let decision = evaluate_noise(&input("test", "note", EntityType::Url, "http://127.0.0.1/admin"));
        assert_eq!(decision.action, NoiseAction::Block);
    }
}
