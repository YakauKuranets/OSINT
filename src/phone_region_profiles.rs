use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionalProbeTemplate {
    pub profile: String,
    pub source_id: String,
    pub category: String,
    pub country_or_region: String,
    pub url_template: String,
}

pub fn regional_phone_probe_templates() -> Vec<RegionalProbeTemplate> {
    let raw = std::env::var("XGEN_PHONE_REGION_PROFILE").unwrap_or_default();
    let profiles = raw
        .split(',')
        .map(|item| item.trim().to_lowercase())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();

    if profiles.is_empty() || profiles.iter().any(|p| p == "off" || p == "none") {
        return Vec::new();
    }

    let mut out = Vec::new();
    for profile in profiles {
        match profile.as_str() {
            "cis" => out.extend(cis_templates("cis")),
            "eu" => out.extend(eu_templates("eu")),
            "cis_eu" | "eu_cis" => {
                out.extend(cis_templates("cis_eu"));
                out.extend(eu_templates("cis_eu"));
            }
            "by" | "belarus" => out.extend(belarus_templates("by")),
            "pl" | "poland" => out.extend(poland_templates("pl")),
            "ua" | "ukraine" => out.extend(ukraine_templates("ua")),
            _ => {}
        }
    }
    out.sort_by(|a, b| a.url_template.cmp(&b.url_template));
    out.dedup_by(|a, b| a.url_template == b.url_template);
    out
}

fn cis_templates(profile: &str) -> Vec<RegionalProbeTemplate> {
    let mut out = Vec::new();
    out.extend(belarus_templates(profile));
    out.extend(ukraine_templates(profile));
    out.extend(vec![
        tpl(profile, "ru_avito_public_search", "classifieds", "RU", "https://www.avito.ru/all?q={term}"),
        tpl(profile, "ru_habr_public_search", "tech_profiles_and_posts", "RU", "https://habr.com/ru/search/?q={term}&target_type=posts"),
        tpl(profile, "ru_vc_public_search", "public_posts", "RU", "https://vc.ru/search/v2/content?query={term}"),
        tpl(profile, "kz_olx_public_search", "classifieds", "KZ", "https://www.olx.kz/list/q-{term}/"),
    ]);
    out
}

fn eu_templates(profile: &str) -> Vec<RegionalProbeTemplate> {
    let mut out = Vec::new();
    out.extend(poland_templates(profile));
    out.extend(vec![
        tpl(profile, "de_kleinanzeigen_public_search", "classifieds", "DE", "https://www.kleinanzeigen.de/s-{term}/k0"),
        tpl(profile, "fr_leboncoin_public_search", "classifieds", "FR", "https://www.leboncoin.fr/recherche?text={term}"),
        tpl(profile, "it_subito_public_search", "classifieds", "IT", "https://www.subito.it/annunci-italia/vendita/usato/?q={term}"),
        tpl(profile, "es_milanuncios_public_search", "classifieds", "ES", "https://www.milanuncios.com/anuncios/{term}.htm"),
        tpl(profile, "uk_gumtree_public_search", "classifieds", "UK", "https://www.gumtree.com/search?search_category=all&q={term}"),
    ]);
    out
}

fn belarus_templates(profile: &str) -> Vec<RegionalProbeTemplate> {
    vec![
        tpl(profile, "by_kufar_public_search", "classifieds", "BY", "https://www.kufar.by/l/r~belarus?query={term}"),
        tpl(profile, "by_av_public_search", "vehicles", "BY", "https://av.by/search?keyword={term}"),
        tpl(profile, "by_onliner_baraholka_public_search", "classifieds_forum", "BY", "https://baraholka.onliner.by/search.php?q={term}"),
        tpl(profile, "by_deal_public_search", "business_catalog_marketplace", "BY", "https://deal.by/search?search_term={term}"),
        tpl(profile, "by_rabota_public_search", "jobs_public_profiles", "BY", "https://rabota.by/search/vacancy?text={term}"),
    ]
}

fn poland_templates(profile: &str) -> Vec<RegionalProbeTemplate> {
    vec![
        tpl(profile, "pl_olx_public_search", "classifieds", "PL", "https://www.olx.pl/oferty/q-{term}/"),
        tpl(profile, "pl_allegro_public_search", "marketplace", "PL", "https://allegro.pl/listing?string={term}"),
        tpl(profile, "pl_pracuj_public_search", "jobs_public_profiles", "PL", "https://www.pracuj.pl/praca/{term};kw"),
    ]
}

fn ukraine_templates(profile: &str) -> Vec<RegionalProbeTemplate> {
    vec![
        tpl(profile, "ua_olx_public_search", "classifieds", "UA", "https://www.olx.ua/uk/list/q-{term}/"),
        tpl(profile, "ua_prom_public_search", "business_catalog_marketplace", "UA", "https://prom.ua/ua/search?search_term={term}"),
        tpl(profile, "ua_work_public_search", "jobs_public_profiles", "UA", "https://www.work.ua/jobs/?search={term}"),
    ]
}

fn tpl(profile: &str, source_id: &str, category: &str, country_or_region: &str, url_template: &str) -> RegionalProbeTemplate {
    RegionalProbeTemplate {
        profile: profile.to_string(),
        source_id: source_id.to_string(),
        category: category.to_string(),
        country_or_region: country_or_region.to_string(),
        url_template: url_template.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_shape_contains_term_placeholder() {
        for t in belarus_templates("by") {
            assert!(t.url_template.contains("{term}"));
            assert!(!t.source_id.is_empty());
            assert!(!t.category.is_empty());
        }
    }
}
