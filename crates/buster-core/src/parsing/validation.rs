use std::net::IpAddr;
use std::sync::OnceLock;
use regex::Regex;

static DOMAIN_REGEX: OnceLock<Regex> = OnceLock::new();

fn domain_regex() -> &'static Regex {
    DOMAIN_REGEX.get_or_init(|| {
        Regex::new(r"^[a-zA-Z0-9](?:[a-zA-Z0-9.\-]{0,251}[a-zA-Z0-9])?$").expect("Invalid domain regex")
    })
}

/// Validates a hostname or IP address without resolving it.
pub fn is_valid_host(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 253 {
        return false;
    }

    let unbracketed = trimmed.trim_start_matches('[').trim_end_matches(']');
    if unbracketed.parse::<IpAddr>().is_ok() {
        return true;
    }

    domain_regex().is_match(trimmed)
}

/// Validates that port is in range 1..=65535.
pub fn is_valid_port(port: u32) -> bool {
    (1..=65535).contains(&port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_hosts() {
        assert!(is_valid_host("example.com"));
        assert!(is_valid_host("sub.example.com"));
        assert!(is_valid_host("192.168.1.1"));
        assert!(is_valid_host("[::1]"));
        assert!(is_valid_host("::1"));
        assert!(!is_valid_host(""));
        assert!(!is_valid_host("   "));
        assert!(!is_valid_host("bad host with spaces.com"));
    }

    #[test]
    fn test_valid_ports() {
        assert!(is_valid_port(80));
        assert!(is_valid_port(443));
        assert!(is_valid_port(65535));
        assert!(!is_valid_port(0));
        assert!(!is_valid_port(70000));
    }
}
