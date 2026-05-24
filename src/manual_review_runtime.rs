use crate::manual_review_gate::{
    build_manual_review_gate_report, create_manual_review_card, save_manual_review_gate_report,
    ManualReviewGateCard,
};
use crate::models::{EntityNode, EntityType};

pub fn build_and_save_manual_review_gate_for_seeds(seeds: &[EntityNode], path: &str) -> Result<usize, String> {
    let mut cards = Vec::new();
    for seed in seeds {
        if let Some(card) = card_for_seed(seed) {
            cards.push(card);
        }
    }
    let count = cards.len();
    let report = build_manual_review_gate_report(cards);
    save_manual_review_gate_report(&report, path)?;
    Ok(count)
}

fn card_for_seed(seed: &EntityNode) -> Option<ManualReviewGateCard> {
    let selector_type = match seed.entity_type {
        EntityType::Phone => "phone",
        EntityType::Email => "email",
        EntityType::Username | EntityType::Nickname => "username",
        EntityType::FullName => "full_name",
        _ => return None,
    };
    let masked = mask_selector(&seed.value, &seed.entity_type);
    let review_id = format!(
        "review_{}_{}",
        selector_type,
        stable_short_id(&masked)
    );
    Some(create_manual_review_card(
        &review_id,
        selector_type,
        &masked,
        "manual_operator_review",
    ))
}

fn mask_selector(value: &str, entity_type: &EntityType) -> String {
    match entity_type {
        EntityType::Phone => mask_phone(value),
        EntityType::Email => mask_email(value),
        EntityType::Username | EntityType::Nickname => mask_username(value),
        EntityType::FullName => mask_full_name(value),
        _ => "selector_masked".to_string(),
    }
}

fn mask_phone(value: &str) -> String {
    let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 6 {
        return "phone_masked".to_string();
    }
    let prefix_len = digits.len().min(4);
    let suffix_len = 2usize.min(digits.len().saturating_sub(prefix_len));
    format!("+{}***{}", &digits[..prefix_len], &digits[digits.len() - suffix_len..])
}

fn mask_email(value: &str) -> String {
    let clean = value.trim().to_lowercase();
    let Some((local, domain)) = clean.split_once('@') else {
        return "email_masked".to_string();
    };
    let local_head = local.chars().next().unwrap_or('*');
    let domain_tail = domain.rsplit('.').next().unwrap_or("domain");
    format!("{}***@***.{}", local_head, domain_tail)
}

fn mask_username(value: &str) -> String {
    let clean = value.trim().trim_start_matches('@');
    if clean.len() <= 2 {
        return "user***".to_string();
    }
    let first = clean.chars().next().unwrap_or('u');
    let last = clean.chars().last().unwrap_or('r');
    format!("@{}***{}", first, last)
}

fn mask_full_name(value: &str) -> String {
    let parts = value.split_whitespace().collect::<Vec<_>>();
    if parts.is_empty() {
        return "name_masked".to_string();
    }
    let initials = parts
        .iter()
        .filter_map(|p| p.chars().next())
        .take(3)
        .collect::<String>();
    format!("{}***", initials)
}

fn stable_short_id(value: &str) -> String {
    let mut hash = 5381u64;
    for byte in value.as_bytes() {
        hash = ((hash << 5).wrapping_add(hash)).wrapping_add(*byte as u64);
    }
    format!("{:x}", hash)[..8].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_masks_phone_and_creates_card() {
        let seeds = vec![EntityNode { value: "+000000000000".to_string(), entity_type: EntityType::Phone, first_seen: 1 }];
        let card = card_for_seed(&seeds[0]).expect("card");
        assert_eq!(card.selector_type, "phone");
        assert!(card.selector_masked.contains("***"));
        assert!(!card.raw_record_stored);
        assert!(!card.raw_record_visible);
    }
}
