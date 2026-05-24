use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourcePolicyDecision {
    Allowed,
    NeedsReview,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourcePolicyKind {
    PublicWebPage,
    OfficialRegistry,
    PublicMarketplace,
    PublicProfile,
    SearchIndex,
    UserProvidedExport,
    VerifiedExposureSignal,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourcePolicyAssessment {
    pub source_id: String,
    pub kind: SourcePolicyKind,
    pub decision: SourcePolicyDecision,
    pub reasons: Vec<String>,
    pub allowed_use: Vec<String>,
    pub blocked_use: Vec<String>,
}

pub fn assess_source_policy(source_id: &str, description: &str) -> SourcePolicyAssessment {
    let combined = format!("{} {}", source_id.to_lowercase(), description.to_lowercase());

    if contains_any(&combined, &["official", "registry", "реестр", "government", "gov"]) {
        return allowed(source_id, SourcePolicyKind::OfficialRegistry, "official or registry-style public source");
    }
    if contains_any(&combined, &["marketplace", "classifieds", "catalog", "каталог", "объяв"] ) {
        return allowed(source_id, SourcePolicyKind::PublicMarketplace, "public marketplace or catalog source");
    }
    if contains_any(&combined, &["profile", "public profile", "соц", "social"] ) {
        return allowed(source_id, SourcePolicyKind::PublicProfile, "public profile source");
    }
    if contains_any(&combined, &["search", "index", "open web", "public page"] ) {
        return allowed(source_id, SourcePolicyKind::SearchIndex, "public search or index source");
    }
    if contains_any(&combined, &["user export", "user-provided", "own export", "личный экспорт"] ) {
        return SourcePolicyAssessment {
            source_id: source_id.to_string(),
            kind: SourcePolicyKind::UserProvidedExport,
            decision: SourcePolicyDecision::Allowed,
            reasons: vec!["user-provided scoped export".to_string()],
            allowed_use: vec!["local parsing within user-provided scope".to_string(), "evidence extraction with provenance".to_string()],
            blocked_use: vec!["using the export outside the provided scope".to_string()],
        };
    }
    if contains_any(&combined, &["verified exposure", "verified risk", "owned scope", "domain verified"] ) {
        return SourcePolicyAssessment {
            source_id: source_id.to_string(),
            kind: SourcePolicyKind::VerifiedExposureSignal,
            decision: SourcePolicyDecision::NeedsReview,
            reasons: vec!["risk signal provider requires verified scope and provider terms review".to_string()],
            allowed_use: vec!["high-level risk metadata for verified scope".to_string()],
            blocked_use: vec!["raw sensitive record retrieval".to_string(), "bulk unrelated lookups".to_string()],
        };
    }

    SourcePolicyAssessment {
        source_id: source_id.to_string(),
        kind: SourcePolicyKind::Unknown,
        decision: SourcePolicyDecision::NeedsReview,
        reasons: vec!["source is not in the allowlist and must be reviewed before enabling".to_string()],
        allowed_use: vec!["manual classification".to_string()],
        blocked_use: vec!["automatic provider activation".to_string(), "bulk import".to_string()],
    }
}

pub fn is_source_allowed(source_id: &str, description: &str) -> bool {
    assess_source_policy(source_id, description).decision == SourcePolicyDecision::Allowed
}

fn allowed(source_id: &str, kind: SourcePolicyKind, reason: &str) -> SourcePolicyAssessment {
    SourcePolicyAssessment {
        source_id: source_id.to_string(),
        kind,
        decision: SourcePolicyDecision::Allowed,
        reasons: vec![reason.to_string()],
        allowed_use: vec!["exact-match probing".to_string(), "context extraction with source URL".to_string(), "confidence scoring".to_string()],
        blocked_use: vec!["bypassing access controls".to_string(), "private-area collection".to_string(), "unverified identity conclusion".to_string()],
    }
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_public_catalog_source() {
        let result = assess_source_policy("regional_catalog", "public catalog search page");
        assert_eq!(result.decision, SourcePolicyDecision::Allowed);
    }

    #[test]
    fn unknown_source_needs_review() {
        let result = assess_source_policy("unknown_source", "unclear dataset");
        assert_eq!(result.decision, SourcePolicyDecision::NeedsReview);
    }
}
