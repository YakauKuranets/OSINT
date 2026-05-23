use crate::models::{EntityNode, EntityType, SourceMetadata, SourceClass};
use reqwest::{Client, redirect::Policy};
use serde::Deserialize;
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct TargetSite {
    pub name: String,
    pub check_url: String,
    pub error_indicator: Option<String>,
    pub success_indicator: Option<String>,
}


#[derive(Deserialize)]
struct SiteConfigEntry {
    name: String,
    check_url: String,
    error_indicator: Option<String>,
    success_indicator: Option<String>,
    requires_tor: Option<bool>,
}

fn load_sites_from_json(path: &str) -> Vec<TargetSite> {
    let allow_onion = std::env::var("OSINT_ENABLE_ONION")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes"))
        .unwrap_or(false);

    let data = match std::fs::read_to_string(path) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let parsed: Vec<SiteConfigEntry> = serde_json::from_str(&data).unwrap_or_default();
    parsed
        .into_iter()
        .filter(|s| !s.name.trim().is_empty() && s.check_url.contains("{username}"))
        .filter(|s| {
            let is_tor = s.requires_tor.unwrap_or(false) || s.check_url.contains(".onion");
            allow_onion || !is_tor
        })
        .map(|s| TargetSite {
            name: s.name,
            check_url: s.check_url.replace("{username}", "{}"),
            error_indicator: s.error_indicator,
            success_indicator: s.success_indicator,
        })
        .collect()
}

pub fn get_default_sites() -> Vec<TargetSite> {
    let mut sites = priority_cis_social_sites();

    sites.extend(vec![
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
            error_indicator: None,
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
    ]);

    for site in extra_social_sites() {
        sites.push(site);
    }

    let mut known: HashSet<String> = sites.iter().map(|s| s.name.to_lowercase()).collect();
    for external in load_sites_from_json("sites.json") {
        let key = external.name.to_lowercase();
        if !known.contains(&key) {
            known.insert(key);
            sites.push(external);
        }
    }

    sites
}

// 🔥 СТРОГИЙ КЛИЕНТ: НИКАКИХ РЕДИРЕКТОВ 🔥


fn extra_social_sites() -> Vec<TargetSite> {
    vec![
        TargetSite { name: "Threads".to_string(), check_url: "https://www.threads.net/@{}".to_string(), error_indicator: Some("Sorry, this page isn't available".to_string()), success_indicator: None },
        TargetSite { name: "Bluesky".to_string(), check_url: "https://bsky.app/profile/{}".to_string(), error_indicator: Some("Profile not found".to_string()), success_indicator: Some("did:plc:".to_string()) },
        TargetSite { name: "Mastodon.social".to_string(), check_url: "https://mastodon.social/@{}".to_string(), error_indicator: Some("Record not found".to_string()), success_indicator: Some("@".to_string()) },
        TargetSite { name: "GitLab".to_string(), check_url: "https://gitlab.com/{}".to_string(), error_indicator: Some("404 Page not found".to_string()), success_indicator: Some("profile-page".to_string()) },
        TargetSite { name: "Bitbucket".to_string(), check_url: "https://bitbucket.org/{}/".to_string(), error_indicator: Some("Page not found".to_string()), success_indicator: Some("profile".to_string()) },
        TargetSite { name: "Kaggle".to_string(), check_url: "https://www.kaggle.com/{}".to_string(), error_indicator: Some("404 - Not Found".to_string()), success_indicator: Some("profile".to_string()) },
        TargetSite { name: "Goodreads".to_string(), check_url: "https://www.goodreads.com/{}".to_string(), error_indicator: Some("Page not found".to_string()), success_indicator: Some("profile".to_string()) },
        TargetSite { name: "Letterboxd".to_string(), check_url: "https://letterboxd.com/{}/".to_string(), error_indicator: Some("Sorry, we can’t find the page you’ve requested".to_string()), success_indicator: Some("profile".to_string()) },
        TargetSite { name: "Last.fm".to_string(), check_url: "https://www.last.fm/user/{}".to_string(), error_indicator: Some("Page not found".to_string()), success_indicator: Some("header-new-title".to_string()) },
        TargetSite { name: "Deezer".to_string(), check_url: "https://www.deezer.com/en/profile/{}".to_string(), error_indicator: Some("404".to_string()), success_indicator: Some("profile".to_string()) },
        TargetSite { name: "Tripadvisor".to_string(), check_url: "https://www.tripadvisor.com/members/{}".to_string(), error_indicator: Some("Page Not Found".to_string()), success_indicator: Some("member".to_string()) },
        TargetSite { name: "Strava".to_string(), check_url: "https://www.strava.com/athletes/{}".to_string(), error_indicator: Some("Page Not Found".to_string()), success_indicator: Some("athlete-name".to_string()) },
        TargetSite { name: "Chess.com".to_string(), check_url: "https://www.chess.com/member/{}".to_string(), error_indicator: Some("Page not found".to_string()), success_indicator: Some("profile".to_string()) },
        TargetSite { name: "Lichess".to_string(), check_url: "https://lichess.org/@/{}".to_string(), error_indicator: Some("404".to_string()), success_indicator: Some("user-show".to_string()) },
    ]
}


fn priority_cis_social_sites() -> Vec<TargetSite> {
    vec![
        TargetSite { name: "VK".to_string(), check_url: "https://vk.com/{}".to_string(), error_indicator: Some("Page not found".to_string()), success_indicator: Some("profile_photo".to_string()) },
        TargetSite { name: "Telegram".to_string(), check_url: "https://t.me/{}".to_string(), error_indicator: None, success_indicator: Some("tgme_page_title".to_string()) },
        TargetSite { name: "TikTok".to_string(), check_url: "https://www.tiktok.com/@{}".to_string(), error_indicator: Some("Couldn't find this account".to_string()), success_indicator: Some("uniqueId".to_string()) },
        TargetSite { name: "Instagram".to_string(), check_url: "https://www.instagram.com/{}/".to_string(), error_indicator: Some("Sorry, this page isn't available".to_string()), success_indicator: Some("www.instagram.com/{}/channel/".to_string()) },
        TargetSite { name: "YouTube".to_string(), check_url: "https://www.youtube.com/@{}".to_string(), error_indicator: Some("This channel does not exist".to_string()), success_indicator: Some("canonical".to_string()) },
        // У Viber нет стабильных публичных username-страниц для профилей; используем web-поиск как источник зацепок.
        TargetSite { name: "Viber (web mentions)".to_string(), check_url: "https://www.google.com/search?q=site%3Aviber.com+{}".to_string(), error_indicator: None, success_indicator: None },
    ]
}

fn build_strict_client() -> Client {
    let mut builder = Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .redirect(Policy::none()); // Блокирует Soft 404

    if let Ok(proxy_url) = std::env::var("OSINT_TOR_PROXY") {
        if let Ok(proxy) = reqwest::Proxy::all(&proxy_url) {
            builder = builder.proxy(proxy);
        }
    }

    builder.build().unwrap()
}

/// Извлекает дополнительные данные из УЖЕ скачанного HTML (без лишних запросов)
pub fn mine_profile_details(body: &str, site_name: &str) -> Vec<(String, String)> {
    let mut details = Vec::new();
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
        _ => {}
    }
    details
}

pub async fn hunt_social_profiles(
    _shared_client: &Client, // Игнорируем обычный клиент, используем строгий
    raw_username: &str,
    sites: &[TargetSite],
) -> Vec<(EntityNode, SourceMetadata)> {
    let mut found_profiles = Vec::new();
    let strict_client = build_strict_client();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let source_meta = SourceMetadata {
        source_id: "Social_Spider_Strict".to_string(),
        class: SourceClass::PublicOSINT,
        import_timestamp: now,
        data_actual_year: 2026,
    };

    let username = raw_username.trim_start_matches('@').trim();

    println!("  [Spider] Приоритетный охват: VK, Telegram, TikTok, Instagram, YouTube, Viber mentions");
    println!("  [Spider] Жесткая проверка {} сайтов для {}", sites.len(), username);
    for site in sites {
        let url = site.check_url.replace("{}", username);
        let resp = match strict_client.get(&url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .send()
            .await
        {
            Ok(r) => r,
            Err(_) => continue,
        };

        let status = resp.status();

        // 🔥 ЖЕСТКИЙ ФИЛЬТР: Отсекаем редиректы (301, 302) и ошибки (404, 403, 429)
        if status.is_server_error() || status.is_redirection() || status.as_u16() == 403 || status.as_u16() == 429 || status.as_u16() == 404 {
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

        // Профиль 100% подтверждён
        println!("  [+] Найден верифицированный профиль: {} -> {}", site.name, url);
        let node = EntityNode {
            value: format!("{}:{}", site.name.to_lowercase().replace(" ", "_"), username),
            entity_type: EntityType::Nickname,
            first_seen: now,
        };
        found_profiles.push((node, source_meta.clone()));

        // ===== Активируем ProfileMiner без повторного скачивания =====
        let details = mine_profile_details(&body, &site.name);
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
