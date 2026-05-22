use crate::models::{EntityNode, EntityType, SourceMetadata, SourceClass};
use reqwest::Client;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct TargetSite {
    pub name: String,
    pub check_url: String,
    pub error_indicator: Option<String>,
    pub success_indicator: Option<String>, // Что ДОЛЖНО быть на странице при успехе
}

pub fn get_default_sites() -> Vec<TargetSite> {
    vec![
        // --- Социальные сети и медиа ---
        TargetSite {
            name: "GitHub".to_string(),
            check_url: "https://github.com/{}".to_string(),
            error_indicator: None,
            success_indicator: Some("vcard-details".to_string()),
        },
        TargetSite {
            name: "Reddit".to_string(),
            check_url: "https://www.reddit.com/user/{}/".to_string(),
            error_indicator: Some("page not found".to_string()),
            success_indicator: Some("profile--avatar".to_string()),
        },
        TargetSite {
            name: "Steam".to_string(),
            check_url: "https://steamcommunity.com/id/{}".to_string(),
            error_indicator: Some("The specified profile could not be found".to_string()),
            success_indicator: None,
        },
        TargetSite {
            name: "Twitter/X".to_string(),
            check_url: "https://twitter.com/{}".to_string(),
            error_indicator: Some("This account doesn’t exist".to_string()),
            success_indicator: Some("profile-website".to_string()),
        },
        TargetSite {
            name: "Instagram".to_string(),
            check_url: "https://www.instagram.com/{}/".to_string(),
            error_indicator: Some("Sorry, this page isn't available".to_string()),
            success_indicator: Some("www.instagram.com/{}/channel/".to_string()),
        },
        TargetSite {
            name: "TikTok".to_string(),
            check_url: "https://www.tiktok.com/@{}".to_string(),
            error_indicator: Some("Couldn't find this account".to_string()),
            success_indicator: Some("uniqueId".to_string()),
        },
        TargetSite {
            name: "VK".to_string(),
            check_url: "https://vk.com/{}".to_string(),
            error_indicator: Some("Page not found".to_string()),
            success_indicator: Some("profile_photo".to_string()),
        },
        TargetSite {
            name: "Telegram".to_string(),
            check_url: "https://t.me/{}".to_string(),
            error_indicator: None,
            success_indicator: Some("tgme_page_title".to_string()),
        },
        TargetSite {
            name: "Pinterest".to_string(),
            check_url: "https://www.pinterest.com/{}/".to_string(),
            error_indicator: Some("User not found".to_string()),
            success_indicator: None,
        },
        TargetSite {
            name: "OK.ru".to_string(),
            check_url: "https://ok.ru/{}".to_string(),
            error_indicator: Some("page not found".to_string()),
            success_indicator: Some("profile-photo_img".to_string()),
        },
        TargetSite {
            name: "Facebook".to_string(),
            check_url: "https://www.facebook.com/{}".to_string(),
            error_indicator: Some("This page isn't available".to_string()),
            success_indicator: Some("profilePic".to_string()),
        },
        TargetSite {
            name: "YouTube".to_string(),
            check_url: "https://www.youtube.com/@{}".to_string(),
            error_indicator: Some("This channel does not exist".to_string()),
            success_indicator: Some("canonical".to_string()),
        },
        TargetSite {
            name: "LinkedIn".to_string(),
            check_url: "https://www.linkedin.com/in/{}".to_string(),
            error_indicator: Some("Page not found".to_string()),
            success_indicator: Some("profile-picture".to_string()),
        },
        TargetSite {
            name: "Snapchat".to_string(),
            check_url: "https://www.snapchat.com/add/{}".to_string(),
            error_indicator: Some("This content could not be found".to_string()),
            success_indicator: Some("profile-card".to_string()),
        },
        TargetSite {
            name: "Spotify".to_string(),
            check_url: "https://open.spotify.com/user/{}".to_string(),
            error_indicator: Some("Page not found".to_string()),
            success_indicator: Some("playlist".to_string()),
        },
        TargetSite {
            name: "Twitch".to_string(),
            check_url: "https://www.twitch.tv/{}".to_string(),
            error_indicator: Some("Sorry. Unless you’ve got a time machine, that content is unavailable.".to_string()),
            success_indicator: Some("channel-header__user".to_string()),
        },
        TargetSite {
            name: "Patreon".to_string(),
            check_url: "https://www.patreon.com/{}".to_string(),
            error_indicator: Some("This page is not available".to_string()),
            success_indicator: Some("profile-header".to_string()),
        },
        TargetSite {
            name: "Medium".to_string(),
            check_url: "https://medium.com/@{}".to_string(),
            error_indicator: Some("Out of nothing, something".to_string()),
            success_indicator: Some("profile-header".to_string()),
        },
        TargetSite {
            name: "Tumblr".to_string(),
            check_url: "https://{}.tumblr.com".to_string(),
            error_indicator: Some("Not found.".to_string()),
            success_indicator: Some("tumblr_avatar".to_string()),
        },
        TargetSite {
            name: "Roblox".to_string(),
            check_url: "https://www.roblox.com/user.aspx?username={}".to_string(),
            error_indicator: Some("Page cannot be found or no longer exists".to_string()),
            success_indicator: Some("profile-avatar-thumb".to_string()),
        },
        // --- Форумы и сообщества ---
        TargetSite {
            name: "DeviantArt".to_string(),
            check_url: "https://www.deviantart.com/{}".to_string(),
            error_indicator: Some("Page not found".to_string()),
            success_indicator: Some("user-profile-header".to_string()),
        },
        TargetSite {
            name: "Flickr".to_string(),
            check_url: "https://www.flickr.com/people/{}".to_string(),
            error_indicator: Some("Page Not Found".to_string()),
            success_indicator: Some("profile-photo".to_string()),
        },
        TargetSite {
            name: "Quora".to_string(),
            check_url: "https://www.quora.com/profile/{}".to_string(),
            error_indicator: Some("Page not found".to_string()),
            success_indicator: Some("profile_header".to_string()),
        },
        TargetSite {
            name: "SoundCloud".to_string(),
            check_url: "https://soundcloud.com/{}".to_string(),
            error_indicator: Some("We can’t find that user".to_string()),
            success_indicator: Some("profile-header".to_string()),
        },
        TargetSite {
            name: "Bandcamp".to_string(),
            check_url: "https://bandcamp.com/{}".to_string(),
            error_indicator: Some("Sorry, that something isn’t here".to_string()),
            success_indicator: Some("bandcamp-following".to_string()),
        },
        TargetSite {
            name: "Dribbble".to_string(),
            check_url: "https://dribbble.com/{}".to_string(),
            error_indicator: Some("Whoops, that page is gone".to_string()),
            success_indicator: Some("profile-avatar".to_string()),
        },
        TargetSite {
            name: "Behance".to_string(),
            check_url: "https://www.behance.net/{}".to_string(),
            error_indicator: Some("Sorry, we couldn’t find that page".to_string()),
            success_indicator: Some("profile-avatar".to_string()),
        },
        // --- Специфические сервисы ---
        TargetSite {
            name: "CashApp".to_string(),
            check_url: "https://cash.app/{}".to_string(),
            error_indicator: Some("Couldn't find that".to_string()),
            success_indicator: Some("profilePic".to_string()),
        },
        TargetSite {
            name: "Venmo".to_string(),
            check_url: "https://venmo.com/{}".to_string(),
            error_indicator: Some("Page not found".to_string()),
            success_indicator: Some("profile-picture".to_string()),
        },
        TargetSite {
            name: "PayPal".to_string(),
            check_url: "https://www.paypal.com/paypalme/{}".to_string(),
            error_indicator: Some("This profile could not be found".to_string()),
            success_indicator: Some("profile-header".to_string()),
        },
        TargetSite {
            name: "Keybase".to_string(),
            check_url: "https://keybase.io/{}".to_string(),
            error_indicator: Some("Page not found".to_string()),
            success_indicator: Some("profile-avatar".to_string()),
        },
        // --- Игровые платформы ---
        TargetSite {
            name: "Xbox".to_string(),
            check_url: "https://account.xbox.com/en-us/profile?gamertag={}".to_string(),
            error_indicator: Some("We couldn't find a profile for that Gamertag".to_string()),
            success_indicator: Some("gamertag".to_string()),
        },
        TargetSite {
            name: "PlayStation".to_string(),
            check_url: "https://psnprofiles.com/{}".to_string(),
            error_indicator: Some("Profile not found".to_string()),
            success_indicator: Some("profile-name".to_string()),
        },
        TargetSite {
            name: "Nintendo".to_string(),
            check_url: "https://www.nintendo.com/search/?q={}".to_string(),
            error_indicator: None, // этот сайт сложнее, можно просто проверять наличие блока поиска
            success_indicator: Some("search-results".to_string()),
        },
        // --- Разное ---
        TargetSite {
            name: "WordPress.com".to_string(),
            check_url: "https://{}.wordpress.com".to_string(),
            error_indicator: Some("this site is not available".to_string()),
            success_indicator: Some("site-title".to_string()),
        },
        TargetSite {
            name: "Blogger".to_string(),
            check_url: "https://{}.blogspot.com".to_string(),
            error_indicator: Some("Blog not found".to_string()),
            success_indicator: Some("profile-name".to_string()),
        },
        TargetSite {
            name: "Foursquare".to_string(),
            check_url: "https://foursquare.com/{}".to_string(),
            error_indicator: Some("Page not found".to_string()),
            success_indicator: Some("user-profile-header".to_string()),
        },
        TargetSite {
            name: "MyAnimeList".to_string(),
            check_url: "https://myanimelist.net/profile/{}".to_string(),
            error_indicator: Some("Not Found".to_string()),
            success_indicator: Some("profile-pic".to_string()),
        },
    ]
}

/// Извлекает дополнительные данные профиля (имя, био и т.д.) с указанной страницы.
/// Возвращает вектор кортежей (ключ, значение), где ключ – это тип данных (например, "full_name", "bio", "location").
pub async fn mine_profile_details(client: &Client, url: &str, site_name: &str) -> Vec<(String, String)> {
    let mut details = Vec::new();
    if let Ok(resp) = client.get(url).send().await {
        if resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            match site_name {
                "GitHub" => {
                    if let Some(start) = body.find("\"name\":\"") {
                        let val = &body[start+8..];
                        if let Some(end) = val.find('"') {
                            details.push(("full_name".to_string(), val[..end].to_string()));
                        }
                    }
                    if let Some(start) = body.find("\"bio\":\"") {
                        let val = &body[start+7..];
                        if let Some(end) = val.find('"') {
                            details.push(("bio".to_string(), val[..end].to_string()));
                        }
                    }
                    if let Some(start) = body.find("\"location\":\"") {
                        let val = &body[start+13..];
                        if let Some(end) = val.find('"') {
                            details.push(("location".to_string(), val[..end].to_string()));
                        }
                    }
                },
                "Telegram" => {
                    if let Some(start) = body.find("og:title") {
                        if let Some(cont_start) = body[start..].find("content=\"") {
                            let abs_start = start + cont_start + 9;
                            if let Some(cont_end) = body[abs_start..].find('"') {
                                let title = body[abs_start..abs_start+cont_end].to_string();
                                if !title.contains("Telegram") && !title.is_empty() {
                                    details.push(("full_name".to_string(), title));
                                }
                            }
                        }
                    }
                    if let Some(start) = body.find("og:description") {
                        if let Some(cont_start) = body[start..].find("content=\"") {
                            let abs_start = start + cont_start + 9;
                            if let Some(cont_end) = body[abs_start..].find('"') {
                                let desc = body[abs_start..abs_start+cont_end].to_string();
                                if !desc.is_empty() {
                                    details.push(("bio".to_string(), desc));
                                }
                            }
                        }
                    }
                },
                _ => {
                    // Для остальных сайтов можно добавить парсинг по аналогии
                }
            }
        }
    }
    details
}

pub async fn hunt_social_profiles(
    client: &Client,
    raw_username: &str,
    sites: &[TargetSite],
) -> Vec<(EntityNode, SourceMetadata)> {
    let mut found_profiles = Vec::new();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let source_meta = SourceMetadata {
        source_id: "Social_Spider".to_string(),
        class: SourceClass::PublicOSINT,
        import_timestamp: now,
        data_actual_year: 2026,
    };

    let username = raw_username.trim_start_matches('@').trim();

    println!("  [Spider] Проверка {} сайтов для {}", sites.len(), username);
    for site in sites {
        let url = site.check_url.replace("{}", username);
        let resp = match client.get(&url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .send()
            .await
        {
            Ok(r) => r,
            Err(_) => continue,
        };

        let status = resp.status();
        if status.is_server_error() || status.is_redirection() || status.as_u16() == 403 || status.as_u16() == 429 {
            continue;
        }
        if status.as_u16() == 404 {
            continue;
        }

        let body = resp.text().await.unwrap_or_default();

        if let Some(error_text) = &site.error_indicator {
            if body.contains(error_text) {
                continue;
            }
        }

        if let Some(success_text) = &site.success_indicator {
            let check_str = success_text.replace("{}", username);
            if !body.contains(&check_str) {
                continue;
            }
        }

        // Профиль подтверждён
        println!("  [+] Найден профиль: {} -> {}", site.name, url);
        let node = EntityNode {
            value: format!("{}:{}", site.name.to_lowercase().replace(" ", "_"), username),
            entity_type: EntityType::Nickname,
            first_seen: now,
        };
        found_profiles.push((node, source_meta.clone()));

        // ===== Активируем ProfileMiner =====
        let details = mine_profile_details(client, &url, &site.name).await;
        for (detail_key, detail_value) in details {
            let entity_type = if detail_key == "full_name" {
                EntityType::FullName
            } else {
                EntityType::Nickname
            };

            let detail_node = EntityNode {
                value: format!("{}_{}:{}", site.name.to_lowercase(), detail_key, detail_value),
                entity_type,
                first_seen: now,
            };
            found_profiles.push((detail_node, source_meta.clone()));
        }
    }
    found_profiles
}