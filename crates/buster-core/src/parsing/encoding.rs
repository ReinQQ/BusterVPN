use std::collections::HashMap;
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use percent_encoding::percent_decode_str;

/// Tries to decode standard and URL-safe base64 strings with or without padding.
pub fn try_decode_base64(input: &str) -> Option<String> {
    let compact: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.is_empty() {
        return None;
    }

    let mut normalized = compact.replace('-', "+").replace('_', "/");
    let pad_len = (4 - normalized.len() % 4) % 4;
    for _ in 0..pad_len {
        normalized.push('=');
    }

    if let Ok(bytes) = STANDARD.decode(&normalized) {
        if let Ok(text) = String::from_utf8(bytes) {
            return Some(text);
        }
    }

    if let Ok(bytes) = URL_SAFE_NO_PAD.decode(&compact) {
        if let Ok(text) = String::from_utf8(bytes) {
            return Some(text);
        }
    }

    None
}

/// Percent-decodes a string, preserving '+' characters for base64 payloads and cryptographic keys.
pub fn unescape_query_value(value: &str) -> String {
    percent_decode_str(value).decode_utf8_lossy().to_string()
}

/// Parses a query string (?a=b&c=d) while preserving the first duplicate value.
pub fn parse_query(query: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let trimmed = query.trim_start_matches('?');
    if trimmed.is_empty() {
        return map;
    }

    for pair in trimmed.split('&') {
        if pair.is_empty() {
            continue;
        }

        let (raw_key, raw_val) = match pair.find('=') {
            Some(idx) => (&pair[..idx], &pair[idx + 1..]),
            None => (pair, ""),
        };

        let key = unescape_query_value(raw_key).to_lowercase();
        if key.is_empty() || map.contains_key(&key) {
            continue;
        }

        let val = unescape_query_value(raw_val);
        map.insert(key, val);
    }

    map
}

/// Splits a list by commas or whitespace and returns non-empty trimmed items.
pub fn split_list(value: &str) -> Vec<String> {
    value
        .split(|c: char| c == ',' || c == ';' || c == '|' || c.is_whitespace())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// Parses common boolean representations ("1", "true", "yes").
pub fn parse_bool(value: Option<&str>) -> bool {
    matches!(
        value.map(|s| s.trim().to_lowercase()).as_deref(),
        Some("1") | Some("true") | Some("yes")
    )
}
