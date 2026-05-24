use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PersonaProfileKind {
    EverydayCisEu,
    TechTail,
    BusinessOwner,
    JobSeeker,
    MarketplaceUser,
    SocialUser,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PersonaSourceCategory {
    Classifieds,
    Marketplace,
    Jobs,
    PublicSocial,
    MessengerAssisted,
    LocalBusinessCatalog,
    RegionalSearch,
    ForumCommunity,
    TechTail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PersonaSourceMode {
    PublicUrlProbe,
    PublicSearchQuery,
    ManualAssistedVerification,
    OptionalTechTail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaSourceTemplate {
    pub source_id: String,
    pub category: PersonaSourceCategory,
    pub mode: PersonaSourceMode,
    pub priority: u8,
    pub region: String,
    pub url_template: Option<String>,
    pub query_hint: Option<String>,
    pub safety_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaSourceProfile {
    pub profile_id: String,
    pub kind: PersonaProfileKind,
    pub description: String,
    pub default_enabled_categories: Vec<PersonaSourceCategory>,
    pub templates: Vec<PersonaSourceTemplate>,
    pub tech_tail_enabled_by_default: bool,
    pub messenger_auto_lookup_enabled: bool,
    pub raw_private_data_storage_enabled: bool,
    pub safety_policy: Vec<String>,
}

pub fn load_persona_source_profile_from_env() -> PersonaSourceProfile {
    let raw = std::env::var("XGEN_PERSONA_SOURCE_PROFILE")
        .or_else(|_| std::env::var("XGEN_PHONE_PERSONA_PROFILE"))
        .unwrap_or_else(|_| "everyday_cis_eu".to_string())
        .to_lowercase();

    match raw.as_str() {
        "tech" | "tech_tail" | "developer" => tech_tail_profile(),
        "business" | "business_owner" => business_owner_profile(),
        "job" | "job_seeker" => job_seeker_profile(),
        "market" | "marketplace" | "marketplace_user" => marketplace_user_profile(),
        "social" | "social_user" => social_user_profile(),
        _ => everyday_cis_eu_profile(),
    }
}

pub fn everyday_cis_eu_profile() -> PersonaSourceProfile {
    let mut templates = Vec::new();
    templates.extend(classifieds_templates());
    templates.extend(marketplace_templates());
    templates.extend(job_templates());
    templates.extend(public_social_templates());
    templates.extend(local_catalog_templates());
    templates.extend(forum_templates());
    templates.extend(regional_search_templates());
    templates.extend(messenger_assisted_templates());
    templates.extend(tech_tail_templates().into_iter().map(|mut t| { t.priority = 5; t.mode = PersonaSourceMode::OptionalTechTail; t }));

    PersonaSourceProfile {
        profile_id: "everyday_cis_eu".to_string(),
        kind: PersonaProfileKind::EverydayCisEu,
        description: "Default profile for ordinary CIS/EU people: marketplaces, classifieds, jobs, public social, local catalogs, regional search, and messenger assisted verification. Tech sources are optional tail only.".to_string(),
        default_enabled_categories: vec![
            PersonaSourceCategory::Classifieds,
            PersonaSourceCategory::Marketplace,
            PersonaSourceCategory::Jobs,
            PersonaSourceCategory::PublicSocial,
            PersonaSourceCategory::MessengerAssisted,
            PersonaSourceCategory::LocalBusinessCatalog,
            PersonaSourceCategory::RegionalSearch,
            PersonaSourceCategory::ForumCommunity,
        ],
        templates,
        tech_tail_enabled_by_default: false,
        messenger_auto_lookup_enabled: false,
        raw_private_data_storage_enabled: false,
        safety_policy: default_safety_policy(),
    }
}

pub fn profile_public_url_templates(profile: &PersonaSourceProfile) -> Vec<String> {
    let mut out = profile.templates.iter()
        .filter(|t| matches!(t.mode, PersonaSourceMode::PublicUrlProbe))
        .filter_map(|t| t.url_template.clone())
        .filter(|t| is_safe_public_template(t))
        .collect::<Vec<_>>();
    out.sort();
    out.dedup();
    out
}

pub fn profile_public_query_hints(profile: &PersonaSourceProfile) -> Vec<String> {
    let mut out = profile.templates.iter()
        .filter(|t| matches!(t.mode, PersonaSourceMode::PublicSearchQuery))
        .filter_map(|t| t.query_hint.clone())
        .collect::<Vec<_>>();
    out.sort();
    out.dedup();
    out
}

pub fn profile_enabled_provider_ids(profile: &PersonaSourceProfile) -> Vec<String> {
    let mut providers = vec!["url_probe".to_string()];
    if std::env::var("XGEN_ENABLE_TECH_TAIL").map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(profile.tech_tail_enabled_by_default) {
        providers.extend([
            "github_code".to_string(),
            "github_issues".to_string(),
            "hackernews".to_string(),
            "gitlab".to_string(),
            "npm_registry".to_string(),
        ]);
    }
    providers.sort();
    providers.dedup();
    providers
}

pub fn is_safe_public_template(template: &str) -> bool {
    let lowered = template.to_lowercase();
    !template.is_empty()
        && template.contains("{term}")
        && (lowered.starts_with("https://") || lowered.starts_with("http://"))
        && !lowered.contains("localhost")
        && !lowered.contains("127.0.0.1")
        && !lowered.contains("169.254.")
        && !lowered.contains("file:")
}

fn classifieds_templates() -> Vec<PersonaSourceTemplate> {
    vec![
        tpl("kufar_by", PersonaSourceCategory::Classifieds, "BY", 1, "https://www.kufar.by/l/r~belarus?query={term}"),
        tpl("onliner_baraholka", PersonaSourceCategory::Classifieds, "BY", 1, "https://baraholka.onliner.by/search.php?q={term}"),
        tpl("olx_ua", PersonaSourceCategory::Classifieds, "UA", 1, "https://www.olx.ua/uk/list/q-{term}/"),
        tpl("olx_pl", PersonaSourceCategory::Classifieds, "PL", 1, "https://www.olx.pl/oferty/q-{term}/"),
        tpl("olx_kz", PersonaSourceCategory::Classifieds, "KZ", 2, "https://www.olx.kz/list/q-{term}/"),
        tpl("avito", PersonaSourceCategory::Classifieds, "RU", 2, "https://www.avito.ru/all?q={term}"),
        tpl("kleinanzeigen", PersonaSourceCategory::Classifieds, "DE", 2, "https://www.kleinanzeigen.de/s-{term}/k0"),
        tpl("leboncoin", PersonaSourceCategory::Classifieds, "FR", 2, "https://www.leboncoin.fr/recherche?text={term}"),
        tpl("subito", PersonaSourceCategory::Classifieds, "IT", 2, "https://www.subito.it/annunci-italia/vendita/usato/?q={term}"),
        tpl("milanuncios", PersonaSourceCategory::Classifieds, "ES", 2, "https://www.milanuncios.com/anuncios/{term}.htm"),
        tpl("gumtree", PersonaSourceCategory::Classifieds, "UK", 3, "https://www.gumtree.com/search?search_category=all&q={term}"),
    ]
}

fn marketplace_templates() -> Vec<PersonaSourceTemplate> {
    vec![
        tpl("deal_by", PersonaSourceCategory::Marketplace, "BY", 1, "https://deal.by/search?search_term={term}"),
        tpl("prom_ua", PersonaSourceCategory::Marketplace, "UA", 1, "https://prom.ua/ua/search?search_term={term}"),
        tpl("allegro_pl", PersonaSourceCategory::Marketplace, "PL", 1, "https://allegro.pl/listing?string={term}"),
        tpl("av_by", PersonaSourceCategory::Marketplace, "BY", 2, "https://av.by/search?keyword={term}"),
    ]
}

fn job_templates() -> Vec<PersonaSourceTemplate> {
    vec![
        tpl("rabota_by", PersonaSourceCategory::Jobs, "BY", 1, "https://rabota.by/search/vacancy?text={term}"),
        tpl("work_ua", PersonaSourceCategory::Jobs, "UA", 1, "https://www.work.ua/jobs/?search={term}"),
        tpl("pracuj_pl", PersonaSourceCategory::Jobs, "PL", 1, "https://www.pracuj.pl/praca/{term};kw"),
    ]
}

fn public_social_templates() -> Vec<PersonaSourceTemplate> {
    vec![
        search_hint("vk_public", PersonaSourceCategory::PublicSocial, "CIS", 1, "site:vk.com {term}"),
        search_hint("ok_public", PersonaSourceCategory::PublicSocial, "CIS", 2, "site:ok.ru {term}"),
        search_hint("telegram_public", PersonaSourceCategory::PublicSocial, "CIS", 1, "site:t.me {term}"),
        search_hint("instagram_public", PersonaSourceCategory::PublicSocial, "EU", 2, "site:instagram.com {term}"),
        search_hint("tiktok_public", PersonaSourceCategory::PublicSocial, "EU", 2, "site:tiktok.com {term}"),
        search_hint("facebook_public", PersonaSourceCategory::PublicSocial, "EU", 3, "site:facebook.com {term}"),
    ]
}

fn local_catalog_templates() -> Vec<PersonaSourceTemplate> {
    vec![
        search_hint("org_catalog_by", PersonaSourceCategory::LocalBusinessCatalog, "BY", 2, "{term} Минск телефон"),
        search_hint("org_catalog_ru", PersonaSourceCategory::LocalBusinessCatalog, "RU", 3, "{term} телефон организация"),
        search_hint("org_catalog_pl", PersonaSourceCategory::LocalBusinessCatalog, "PL", 3, "{term} telefon firma"),
    ]
}

fn forum_templates() -> Vec<PersonaSourceTemplate> {
    vec![
        search_hint("onliner_forum", PersonaSourceCategory::ForumCommunity, "BY", 2, "site:forum.onliner.by {term}"),
        search_hint("drive2", PersonaSourceCategory::ForumCommunity, "CIS", 3, "site:drive2.ru {term}"),
        search_hint("local_forums", PersonaSourceCategory::ForumCommunity, "CIS/EU", 4, "{term} форум"),
    ]
}

fn regional_search_templates() -> Vec<PersonaSourceTemplate> {
    vec![
        search_hint("regional_fullname_phone", PersonaSourceCategory::RegionalSearch, "CIS/EU", 1, "\"{term}\" телефон"),
        search_hint("regional_fullname_email", PersonaSourceCategory::RegionalSearch, "CIS/EU", 1, "\"{term}\" email"),
        search_hint("regional_username", PersonaSourceCategory::RegionalSearch, "CIS/EU", 1, "\"{term}\""),
    ]
}

fn messenger_assisted_templates() -> Vec<PersonaSourceTemplate> {
    ["Telegram", "Viber", "WhatsApp", "VK", "MAX"].iter().map(|name| PersonaSourceTemplate {
        source_id: format!("{}_assisted", name.to_lowercase()),
        category: PersonaSourceCategory::MessengerAssisted,
        mode: PersonaSourceMode::ManualAssistedVerification,
        priority: 1,
        region: "CIS/EU".to_string(),
        url_template: None,
        query_hint: None,
        safety_notes: vec![
            "manual assisted verification only".to_string(),
            "no automated account discovery".to_string(),
            "no raw private profile storage".to_string(),
        ],
    }).collect()
}

fn tech_tail_templates() -> Vec<PersonaSourceTemplate> {
    ["github_code", "github_issues", "hackernews", "gitlab", "npm_registry"].iter().map(|id| PersonaSourceTemplate {
        source_id: id.to_string(),
        category: PersonaSourceCategory::TechTail,
        mode: PersonaSourceMode::OptionalTechTail,
        priority: 5,
        region: "GLOBAL".to_string(),
        url_template: None,
        query_hint: None,
        safety_notes: vec!["optional tech-tail only; not default for ordinary people".to_string()],
    }).collect()
}

fn business_owner_profile() -> PersonaSourceProfile {
    let mut p = everyday_cis_eu_profile();
    p.profile_id = "business_owner".to_string();
    p.kind = PersonaProfileKind::BusinessOwner;
    p
}

fn job_seeker_profile() -> PersonaSourceProfile {
    let mut p = everyday_cis_eu_profile();
    p.profile_id = "job_seeker".to_string();
    p.kind = PersonaProfileKind::JobSeeker;
    p.templates.retain(|t| matches!(t.category, PersonaSourceCategory::Jobs | PersonaSourceCategory::PublicSocial | PersonaSourceCategory::RegionalSearch | PersonaSourceCategory::MessengerAssisted));
    p
}

fn marketplace_user_profile() -> PersonaSourceProfile {
    let mut p = everyday_cis_eu_profile();
    p.profile_id = "marketplace_user".to_string();
    p.kind = PersonaProfileKind::MarketplaceUser;
    p.templates.retain(|t| matches!(t.category, PersonaSourceCategory::Classifieds | PersonaSourceCategory::Marketplace | PersonaSourceCategory::RegionalSearch | PersonaSourceCategory::MessengerAssisted));
    p
}

fn social_user_profile() -> PersonaSourceProfile {
    let mut p = everyday_cis_eu_profile();
    p.profile_id = "social_user".to_string();
    p.kind = PersonaProfileKind::SocialUser;
    p.templates.retain(|t| matches!(t.category, PersonaSourceCategory::PublicSocial | PersonaSourceCategory::ForumCommunity | PersonaSourceCategory::RegionalSearch | PersonaSourceCategory::MessengerAssisted));
    p
}

fn tech_tail_profile() -> PersonaSourceProfile {
    PersonaSourceProfile {
        profile_id: "tech_tail".to_string(),
        kind: PersonaProfileKind::TechTail,
        description: "Optional developer/geek profile. Not default for ordinary CIS/EU person search.".to_string(),
        default_enabled_categories: vec![PersonaSourceCategory::TechTail],
        templates: tech_tail_templates(),
        tech_tail_enabled_by_default: true,
        messenger_auto_lookup_enabled: false,
        raw_private_data_storage_enabled: false,
        safety_policy: default_safety_policy(),
    }
}

fn tpl(id: &str, category: PersonaSourceCategory, region: &str, priority: u8, url: &str) -> PersonaSourceTemplate {
    PersonaSourceTemplate {
        source_id: id.to_string(),
        category,
        mode: PersonaSourceMode::PublicUrlProbe,
        priority,
        region: region.to_string(),
        url_template: Some(url.to_string()),
        query_hint: None,
        safety_notes: vec!["public URL probe only".to_string(), "exact selector match required".to_string()],
    }
}

fn search_hint(id: &str, category: PersonaSourceCategory, region: &str, priority: u8, query: &str) -> PersonaSourceTemplate {
    PersonaSourceTemplate {
        source_id: id.to_string(),
        category,
        mode: PersonaSourceMode::PublicSearchQuery,
        priority,
        region: region.to_string(),
        url_template: None,
        query_hint: Some(query.to_string()),
        safety_notes: vec!["public search query only".to_string(), "rate-limited lawful search".to_string()],
    }
}

fn default_safety_policy() -> Vec<String> {
    vec![
        "public sources only".to_string(),
        "no password recovery checkers".to_string(),
        "no hidden messenger probing".to_string(),
        "no private profile scraping".to_string(),
        "no raw private data storage".to_string(),
        "messengers are manual assisted verification only".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everyday_profile_has_life_sources_and_no_default_tech_tail() {
        let p = everyday_cis_eu_profile();
        assert!(!p.tech_tail_enabled_by_default);
        assert!(!p.messenger_auto_lookup_enabled);
        assert!(!p.raw_private_data_storage_enabled);
        assert!(p.templates.iter().any(|t| t.category == PersonaSourceCategory::Classifieds));
        assert!(p.templates.iter().any(|t| t.category == PersonaSourceCategory::Marketplace));
        assert!(p.templates.iter().any(|t| t.category == PersonaSourceCategory::Jobs));
        assert!(p.templates.iter().any(|t| t.category == PersonaSourceCategory::MessengerAssisted));
    }

    #[test]
    fn public_url_templates_are_safe() {
        let p = everyday_cis_eu_profile();
        let urls = profile_public_url_templates(&p);
        assert!(!urls.is_empty());
        assert!(urls.iter().all(|url| is_safe_public_template(url)));
    }

    #[test]
    fn default_enabled_providers_exclude_tech_tail() {
        let p = everyday_cis_eu_profile();
        let providers = profile_enabled_provider_ids(&p);
        assert_eq!(providers, vec!["url_probe".to_string()]);
    }
}
