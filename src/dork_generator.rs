use crate::models::IdentityProfile;
use std::fs;

/// Генерирует поисковые дорки на основе профиля и сохраняет в файл
pub fn generate_dorks(profile: &IdentityProfile, output_path: &str) -> Vec<String> {
    let mut dorks = Vec::new();

    // Собираем все email
    let mut emails: Vec<&str> = Vec::new();
    for (_, node) in &profile.associated_nodes {
        if let crate::models::EntityType::Email = node.entity_type {
            emails.push(&node.value);
        }
    }
    if let crate::models::EntityType::Email = profile.root_entity.entity_type {
        emails.push(&profile.root_entity.value);
    }

    // Дорки для email
    for email in &emails {
        dorks.push(format!("\"{}\" filetype:pdf", email));
        dorks.push(format!("\"{}\" site:linkedin.com", email));
        dorks.push(format!("\"{}\" intitle:resume", email));
        dorks.push(format!("\"{}\" \"@gmail.com\" OR \"@yandex.ru\"", email));
        dorks.push(format!("intext:\"{}\" \"password\" OR \"pass\"", email));
    }

    // Дорки для телефонов
    let mut phones: Vec<&str> = Vec::new();
    for (_, node) in &profile.associated_nodes {
        if let crate::models::EntityType::Phone = node.entity_type {
            phones.push(&node.value);
        }
    }
    for phone in &phones {
        let digits: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();
        dorks.push(format!("\"{}\" OR \"{}\" intitle:account", phone, digits));
        dorks.push(format!("\"{}\" site:facebook.com OR site:vk.com", phone));
    }

    // Дорки для ников (Nickname)
    let mut nicks: Vec<&str> = Vec::new();
    if let crate::models::EntityType::Nickname = profile.root_entity.entity_type {
        nicks.push(&profile.root_entity.value);
    }
    for (_, node) in &profile.associated_nodes {
        if let crate::models::EntityType::Nickname = node.entity_type {
            nicks.push(&node.value);
        }
    }
    for nick in &nicks {
        dorks.push(format!("{} site:github.com", nick));
        dorks.push(format!("{} site:pastebin.com", nick));
        dorks.push(format!("{} intitle:\"forum\" OR intitle:\"board\"", nick));
    }

    // Убираем дубликаты и сортируем
    dorks.sort();
    dorks.dedup();

    // Сохраняем в файл
    let content = dorks.join("\n");
    fs::write(output_path, &content).expect("Не удалось записать файл дорков");

    dorks
}