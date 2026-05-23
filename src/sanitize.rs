use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetectedSecretKind {
    PasswordField,
    TokenField,
    CookieField,
    PrivateKeyMarker,
    BearerToken,
    BankCardLike,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectedSecret {
    pub kind: DetectedSecretKind,
    pub marker: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanitizedText {
    pub value: String,
    pub original_len: usize,
    pub sanitized_len: usize,
    pub truncated: bool,
    pub detected_secrets: Vec<DetectedSecret>,
}

#[derive(Debug, Clone)]
pub struct SanitizeOptions {
    pub max_chars: usize,
    pub mask_secrets: bool,
    pub compact_whitespace: bool,
}

impl Default for SanitizeOptions {
    fn default() -> Self {
        Self {
            max_chars: 240,
            mask_secrets: true,
            compact_whitespace: true,
        }
    }
}

pub fn strip_control_chars(input: &str) -> String {
    input
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect()
}

pub fn compact_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_chars(input: &str, max_chars: usize) -> (String, bool) {
    if input.chars().count() <= max_chars {
        return (input.to_string(), false);
    }
    (input.chars().take(max_chars).collect::<String>(), true)
}

fn mask_keep_last(value: &str, keep: usize) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    if chars.len() <= keep {
        return "*".repeat(chars.len());
    }
    let visible: String = chars[chars.len() - keep..].iter().collect();
    format!("{}{}", "*".repeat(chars.len() - keep), visible)
}

fn looks_like_sensitive_key(key: &str) -> Option<DetectedSecretKind> {
    let normalized = key
        .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
        .to_lowercase();

    if matches!(
        normalized.as_str(),
        "password" | "pass" | "passwd" | "pwd" | "secret" | "client_secret"
    ) {
        return Some(DetectedSecretKind::PasswordField);
    }

    if normalized.contains("token")
        || normalized.contains("api_key")
        || normalized.contains("apikey")
        || normalized.contains("access_key")
        || normalized.contains("auth")
    {
        return Some(DetectedSecretKind::TokenField);
    }

    if normalized.contains("cookie") || normalized.contains("session") {
        return Some(DetectedSecretKind::CookieField);
    }

    None
}

fn mask_key_value_token(token: &str) -> Option<(String, DetectedSecret)> {
    for separator in ['=', ':'] {
        if let Some((key, value)) = token.split_once(separator) {
            if value.trim().is_empty() {
                continue;
            }
            if let Some(kind) = looks_like_sensitive_key(key) {
                let masked = format!("{}{}[redacted]", key, separator);
                return Some((masked, DetectedSecret { kind, marker: key.to_string() }));
            }
        }
    }
    None
}

fn luhn_valid(digits: &str) -> bool {
    if !(13..=19).contains(&digits.len()) {
        return false;
    }

    let mut sum = 0_u32;
    let mut double = false;
    for ch in digits.chars().rev() {
        let Some(mut n) = ch.to_digit(10) else {
            return false;
        };
        if double {
            n *= 2;
            if n > 9 {
                n -= 9;
            }
        }
        sum += n;
        double = !double;
    }
    sum % 10 == 0
}

fn split_edge_punctuation(token: &str) -> (&str, &str, &str) {
    let start = token
        .char_indices()
        .find(|(_, c)| c.is_ascii_alphanumeric())
        .map(|(idx, _)| idx)
        .unwrap_or(token.len());
    let end = token
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_ascii_alphanumeric())
        .map(|(idx, c)| idx + c.len_utf8())
        .unwrap_or(start);
    (&token[..start], &token[start..end], &token[end..])
}

fn mask_bank_card_token(token: &str) -> Option<(String, DetectedSecret)> {
    let (prefix, core, suffix) = split_edge_punctuation(token);
    let digits: String = core.chars().filter(|c| c.is_ascii_digit()).collect();
    let separator_count = core.chars().filter(|c| *c == '-' || *c == ' ').count();

    if digits.len() < 13 || separator_count > 6 || !luhn_valid(&digits) {
        return None;
    }

    let masked = format!("{}{}{}", prefix, mask_keep_last(&digits, 4), suffix);
    Some((masked, DetectedSecret {
        kind: DetectedSecretKind::BankCardLike,
        marker: "bank_card_like".to_string(),
    }))
}

fn mask_bearer_token_pair(tokens: &[&str], idx: usize) -> Option<(String, DetectedSecret)> {
    if tokens.get(idx).map(|t| t.eq_ignore_ascii_case("bearer")) == Some(true) {
        if let Some(next) = tokens.get(idx + 1) {
            if next.len() >= 8 {
                return Some(("Bearer [redacted]".to_string(), DetectedSecret {
                    kind: DetectedSecretKind::BearerToken,
                    marker: "Bearer".to_string(),
                }));
            }
        }
    }
    None
}

pub fn mask_secrets(input: &str) -> (String, Vec<DetectedSecret>) {
    let tokens: Vec<&str> = input.split_whitespace().collect();
    let mut output = Vec::new();
    let mut detected = Vec::new();
    let mut idx = 0;

    while idx < tokens.len() {
        if let Some((masked, secret)) = mask_bearer_token_pair(&tokens, idx) {
            output.push(masked);
            detected.push(secret);
            idx += 2;
            continue;
        }

        let token = tokens[idx];
        if token.contains("-----BEGIN") || token.contains("PRIVATE") {
            output.push("[private-key-marker-redacted]".to_string());
            detected.push(DetectedSecret {
                kind: DetectedSecretKind::PrivateKeyMarker,
                marker: "private_key_marker".to_string(),
            });
            idx += 1;
            continue;
        }

        if let Some((masked, secret)) = mask_key_value_token(token) {
            output.push(masked);
            detected.push(secret);
            idx += 1;
            continue;
        }

        if let Some((masked, secret)) = mask_bank_card_token(token) {
            output.push(masked);
            detected.push(secret);
            idx += 1;
            continue;
        }

        output.push(token.to_string());
        idx += 1;
    }

    (output.join(" "), detected)
}

pub fn sanitize_text(input: &str, options: &SanitizeOptions) -> SanitizedText {
    let original_len = input.chars().count();
    let mut value = strip_control_chars(input);

    if options.compact_whitespace {
        value = compact_whitespace(&value);
    }

    let mut detected_secrets = Vec::new();
    if options.mask_secrets {
        let (masked, detected) = mask_secrets(&value);
        value = masked;
        detected_secrets = detected;
    }

    let (value, truncated) = truncate_chars(&value, options.max_chars);
    let sanitized_len = value.chars().count();

    SanitizedText {
        value,
        original_len,
        sanitized_len,
        truncated,
        detected_secrets,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_control_chars() {
        let sanitized = sanitize_text("abc\u{0000}\u{0008}def", &SanitizeOptions::default());
        assert_eq!(sanitized.value, "abcdef");
    }

    #[test]
    fn truncates_long_text() {
        let sanitized = sanitize_text(
            "abcdefghijklmnopqrstuvwxyz",
            &SanitizeOptions { max_chars: 5, ..SanitizeOptions::default() },
        );
        assert_eq!(sanitized.value, "abcde");
        assert!(sanitized.truncated);
    }

    #[test]
    fn masks_password_field() {
        let sanitized = sanitize_text("user=a password=supersecret", &SanitizeOptions::default());
        assert!(sanitized.value.contains("password=[redacted]"));
        assert!(sanitized.detected_secrets.iter().any(|s| s.kind == DetectedSecretKind::PasswordField));
    }

    #[test]
    fn masks_bearer_token() {
        let sanitized = sanitize_text("Authorization: Bearer abcdefghijklmnop", &SanitizeOptions::default());
        assert!(sanitized.value.contains("Bearer [redacted]"));
        assert!(sanitized.detected_secrets.iter().any(|s| s.kind == DetectedSecretKind::BearerToken));
    }

    #[test]
    fn masks_luhn_card_like_number() {
        let sanitized = sanitize_text("card=4111111111111111", &SanitizeOptions::default());
        assert!(sanitized.value.contains("card=[redacted]") || sanitized.value.contains("************1111"));
    }
}
