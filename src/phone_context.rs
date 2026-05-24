use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PhonePageContext {
    pub matched_variant: Option<String>,
    pub title: Option<String>,
    pub meta_description: Option<String>,
    pub json_ld_types: Vec<String>,
    pub date_hints: Vec<String>,
    pub email_hints: Vec<String>,
    pub username_hints: Vec<String>,
    pub url_hints: Vec<String>,
    pub context_snippet: String,
    pub cleaned_text_chars: usize,
    pub parser_notes: Vec<String>,
}

pub fn parse_phone_page_context(body: &str, variants: &[String], source_url: Option<&str>) -> Option<PhonePageContext> {
    let matched_variant = find_matched_variant(body, variants)?;
    let cleaned = clean_html_text(body);
    let title = extract_title(body);
    let meta_description = extract_meta_description(body);
    let json_ld_types = extract_json_ld_types(body);
    let mut context_snippet = rich_context_snippet(&cleaned, &matched_variant, 260);

    let mut parser_notes = Vec::new();
    if let Some(t) = &title {
        context_snippet = format!("title: {} | {}", t, context_snippet);
        parser_notes.push("title_extracted".to_string());
    }
    if let Some(meta) = &meta_description {
        context_snippet = format!("{} | meta: {}", context_snippet, meta);
        parser_notes.push("meta_description_extracted".to_string());
    }
    if !json_ld_types.is_empty() { parser_notes.push("json_ld_types_extracted".to_string()); }

    let mut context_for_hints = context_snippet.clone();
    if let Some(url) = source_url {
        context_for_hints.push(' ');
        context_for_hints.push_str(url);
    }
    let email_hints = extract_emails(&context_for_hints);
    let username_hints = extract_usernames(&context_for_hints);
    let url_hints = extract_urls(&context_for_hints);
    let date_hints = extract_date_hints(body, &context_snippet, source_url);

    if !email_hints.is_empty() { parser_notes.push("email_hints_near_phone".to_string()); }
    if !username_hints.is_empty() { parser_notes.push("username_hints_near_phone".to_string()); }
    if !url_hints.is_empty() { parser_notes.push("url_hints_near_phone".to_string()); }
    if !date_hints.is_empty() { parser_notes.push("date_hints_extracted".to_string()); }

    Some(PhonePageContext {
        matched_variant: Some(matched_variant),
        title,
        meta_description,
        json_ld_types,
        date_hints,
        email_hints,
        username_hints,
        url_hints,
        context_snippet: compact_spaces(&context_snippet),
        cleaned_text_chars: cleaned.chars().count(),
        parser_notes,
    })
}

pub fn format_context_for_phone_hit(ctx: &PhonePageContext) -> String {
    let mut parts = Vec::new();
    parts.push(ctx.context_snippet.clone());
    if !ctx.email_hints.is_empty() { parts.push(format!("emails={:?}", ctx.email_hints)); }
    if !ctx.username_hints.is_empty() { parts.push(format!("usernames={:?}", ctx.username_hints)); }
    if !ctx.url_hints.is_empty() { parts.push(format!("urls={:?}", ctx.url_hints)); }
    if !ctx.date_hints.is_empty() { parts.push(format!("dates={:?}", ctx.date_hints)); }
    if !ctx.json_ld_types.is_empty() { parts.push(format!("json_ld_types={:?}", ctx.json_ld_types)); }
    if !ctx.parser_notes.is_empty() { parts.push(format!("parser_notes={:?}", ctx.parser_notes)); }
    parts.join(" | ")
}

fn find_matched_variant(body: &str, variants: &[String]) -> Option<String> {
    let lower = body.to_lowercase();
    variants.iter().filter(|v| !v.trim().is_empty()).find(|v| lower.contains(&v.to_lowercase())).cloned()
}

fn extract_title(html: &str) -> Option<String> {
    extract_between_case_insensitive(html, "<title", "</title>")
        .and_then(|chunk| chunk.split_once('>').map(|(_, rest)| rest.to_string()))
        .map(|value| compact_spaces(&decode_basic_entities(&strip_tags(&value))))
        .filter(|value| !value.is_empty())
}

fn extract_meta_description(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let mut cursor = 0usize;
    while let Some(pos) = lower[cursor..].find("<meta") {
        let start = cursor + pos;
        let end = lower[start..].find('>').map(|idx| start + idx + 1).unwrap_or(html.len());
        let tag = &html[start..end];
        let tag_lower = tag.to_lowercase();
        let is_description = tag_lower.contains("name=\"description\"")
            || tag_lower.contains("name='description'")
            || tag_lower.contains("property=\"og:description\"")
            || tag_lower.contains("property='og:description'");
        if is_description {
            if let Some(content) = extract_attr(tag, "content") {
                let clean = compact_spaces(&decode_basic_entities(&strip_tags(&content)));
                if !clean.is_empty() { return Some(clean); }
            }
        }
        cursor = end;
    }
    None
}

fn extract_json_ld_types(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lower = html.to_lowercase();
    let mut cursor = 0usize;
    while let Some(pos) = lower[cursor..].find("application/ld+json") {
        let around_start = cursor + pos;
        let script_start = lower[..around_start].rfind("<script").unwrap_or(around_start);
        let script_end = lower[around_start..].find("</script>").map(|idx| around_start + idx).unwrap_or(html.len());
        let block = &html[script_start..script_end];
        for marker in ["\"@type\"", "'@type'"] {
            let mut local = 0usize;
            while let Some(tp) = block[local..].find(marker) {
                let start = local + tp + marker.len();
                let tail = &block[start..];
                if let Some(colon) = tail.find(':') {
                    let value_tail = tail[colon + 1..].trim_start();
                    if let Some(value) = extract_quoted_value(value_tail) { out.push(value); }
                }
                local = start;
            }
        }
        cursor = script_end.saturating_add(9);
    }
    out.sort();
    out.dedup();
    out
}

fn extract_date_hints(body: &str, context: &str, source_url: Option<&str>) -> Vec<String> {
    let mut hints = Vec::new();
    let combined = format!("{} {} {}", body, context, source_url.unwrap_or_default());
    let lower = combined.to_lowercase();
    for key in ["datepublished", "datemodified", "published_time", "modified_time", "upload_date", "created_at", "updated_at"] {
        if let Some(pos) = lower.find(key) {
            let start = pos.saturating_sub(20);
            let end = (pos + 120).min(combined.len());
            let window = &combined[start..end];
            for date in extract_iso_dates(window) { hints.push(format!("{}:{}", key, date)); }
        }
    }
    for date in extract_iso_dates(&combined) { hints.push(date); }
    for year in extract_years(&combined) { hints.push(year.to_string()); }
    hints.sort();
    hints.dedup();
    hints.truncate(12);
    hints
}

fn extract_iso_dates(text: &str) -> Vec<String> {
    let mut dates = Vec::new();
    let bytes = text.as_bytes();
    for i in 0..bytes.len().saturating_sub(9) {
        if bytes[i].is_ascii_digit()
            && bytes.get(i + 1).map(|b| b.is_ascii_digit()).unwrap_or(false)
            && bytes.get(i + 2).map(|b| b.is_ascii_digit()).unwrap_or(false)
            && bytes.get(i + 3).map(|b| b.is_ascii_digit()).unwrap_or(false)
            && bytes.get(i + 4) == Some(&b'-')
            && bytes.get(i + 5).map(|b| b.is_ascii_digit()).unwrap_or(false)
            && bytes.get(i + 6).map(|b| b.is_ascii_digit()).unwrap_or(false)
            && bytes.get(i + 7) == Some(&b'-')
            && bytes.get(i + 8).map(|b| b.is_ascii_digit()).unwrap_or(false)
            && bytes.get(i + 9).map(|b| b.is_ascii_digit()).unwrap_or(false)
        { dates.push(text[i..i + 10].to_string()); }
    }
    dates.sort();
    dates.dedup();
    dates
}

fn extract_years(text: &str) -> Vec<u32> {
    let mut years = Vec::new();
    let mut buf = String::new();
    for ch in text.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_digit() { buf.push(ch); }
        else {
            if buf.len() == 4 {
                if let Ok(year) = buf.parse::<u32>() { if (1990..=2035).contains(&year) { years.push(year); } }
            }
            buf.clear();
        }
    }
    years.sort();
    years.dedup();
    years
}

fn rich_context_snippet(cleaned: &str, needle: &str, radius: usize) -> String {
    let lower = cleaned.to_lowercase();
    let needle_lower = needle.to_lowercase();
    if let Some(pos) = lower.find(&needle_lower) {
        let start = char_boundary_before(cleaned, pos.saturating_sub(radius));
        let end = char_boundary_after(cleaned, (pos + needle.len() + radius).min(cleaned.len()));
        compact_spaces(&cleaned[start..end])
    } else {
        compact_spaces(cleaned).chars().take(radius * 2).collect()
    }
}

fn clean_html_text(html: &str) -> String {
    let without_scripts = remove_blocks_case_insensitive(html, "script");
    let without_styles = remove_blocks_case_insensitive(&without_scripts, "style");
    let stripped = strip_tags(&without_styles);
    compact_spaces(&decode_basic_entities(&stripped))
}

fn remove_blocks_case_insensitive(input: &str, tag: &str) -> String {
    let mut out = String::new();
    let lower = input.to_lowercase();
    let open = format!("<{}", tag.to_lowercase());
    let close = format!("</{}>", tag.to_lowercase());
    let mut cursor = 0usize;
    while let Some(pos) = lower[cursor..].find(&open) {
        let start = cursor + pos;
        out.push_str(&input[cursor..start]);
        if let Some(end_rel) = lower[start..].find(&close) { cursor = start + end_rel + close.len(); }
        else { cursor = input.len(); break; }
    }
    out.push_str(&input[cursor..]);
    out
}

fn strip_tags(input: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => { in_tag = true; out.push(' '); }
            '>' => { in_tag = false; out.push(' '); }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

fn extract_between_case_insensitive(input: &str, start_pat: &str, end_pat: &str) -> Option<String> {
    let lower = input.to_lowercase();
    let start_lower = start_pat.to_lowercase();
    let end_lower = end_pat.to_lowercase();
    let start = lower.find(&start_lower)?;
    let end = lower[start..].find(&end_lower).map(|idx| start + idx)?;
    Some(input[start..end].to_string())
}

fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let lower = tag.to_lowercase();
    let name = attr.to_lowercase();
    for pattern in [format!("{}=\"", name), format!("{}='", name)] {
        if let Some(pos) = lower.find(&pattern) {
            let quote = pattern.chars().last()?;
            let start = pos + pattern.len();
            let rest = &tag[start..];
            if let Some(end) = rest.find(quote) { return Some(rest[..end].to_string()); }
        }
    }
    None
}

fn extract_quoted_value(text: &str) -> Option<String> {
    let quote = text.chars().find(|c| *c == '"' || *c == '\'')?;
    let start = text.find(quote)? + 1;
    let rest = &text[start..];
    let end = rest.find(quote)?;
    Some(rest[..end].trim().to_string()).filter(|v| !v.is_empty())
}

fn extract_emails(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in text.split_whitespace() {
        let clean = clean_token(token);
        if clean.contains('@') && clean.contains('.') && clean.len() <= 254 && !clean.starts_with('@') && !clean.ends_with('@') { out.push(clean.to_lowercase()); }
    }
    out.sort();
    out.dedup();
    out.truncate(12);
    out
}

fn extract_urls(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in text.split_whitespace() {
        let clean = clean_token(token);
        let lower = clean.to_lowercase();
        if (lower.starts_with("http://") || lower.starts_with("https://")) && clean.len() <= 512 { out.push(clean); }
    }
    out.sort();
    out.dedup();
    out.truncate(12);
    out
}

fn extract_usernames(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in text.split_whitespace() {
        let clean = clean_token(token);
        if clean.starts_with('@') && clean.len() >= 4 && clean.len() <= 33 {
            let body = &clean[1..];
            if body.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && body.chars().any(|c| c.is_ascii_alphabetic()) { out.push(clean.to_lowercase()); }
        }
    }
    out.sort();
    out.dedup();
    out.truncate(12);
    out
}

fn clean_token(token: &str) -> String {
    token.trim_matches(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | ':' | ')' | '(' | '[' | ']' | '{' | '}' | '<' | '>' | '"' | '\'' | '`')).to_string()
}

fn compact_spaces(value: &str) -> String { value.split_whitespace().collect::<Vec<_>>().join(" ") }

fn decode_basic_entities(value: &str) -> String {
    value.replace("&nbsp;", " ").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'")
}

fn char_boundary_before(value: &str, mut idx: usize) -> usize {
    idx = idx.min(value.len());
    while idx > 0 && !value.is_char_boundary(idx) { idx -= 1; }
    idx
}

fn char_boundary_after(value: &str, mut idx: usize) -> usize {
    idx = idx.min(value.len());
    while idx < value.len() && !value.is_char_boundary(idx) { idx += 1; }
    idx
}
