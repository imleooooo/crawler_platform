pub mod crawler;
pub mod queue;
pub mod s3;

// Re-export sanitize_bucket_name for convenience
pub use s3::sanitize_bucket_name;

/// Validate a caller-supplied URL for use as a crawl target.
///
/// Rejects:
/// - Non-http/https schemes (file://, ftp://, etc.)
/// - Loopback addresses (localhost, 127.0.0.1, ::1)
/// - Private / link-local / unspecified IPv4 and IPv6 ranges
///
/// Returns the parsed `url::Url` on success so callers can use it without
/// re-parsing.
pub fn validate_url(raw: &str) -> Result<url::Url, String> {
    let url =
        url::Url::parse(raw).map_err(|e| format!("Invalid URL '{}': {}", raw, e))?;

    match url.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(format!(
                "URL scheme '{}' is not allowed; only http and https are permitted",
                scheme
            ))
        }
    }

    let host = url
        .host_str()
        .ok_or_else(|| format!("URL '{}' has no host", raw))?;

    // Block bare "localhost" (covers both IPv4 and IPv6 loopback via name)
    if host.eq_ignore_ascii_case("localhost") {
        return Err(format!("Requests to '{}' are not allowed", host));
    }

    // Parse the host as an IP address; reject private / reserved ranges
    // (strip surrounding brackets from IPv6 literals first)
    let ip_str = host.trim_matches(|c| c == '[' || c == ']');
    if let Ok(ip) = ip_str.parse::<std::net::IpAddr>() {
        if is_ssrf_ip(ip) {
            return Err(format!(
                "Requests to private or reserved address '{}' are not allowed",
                ip
            ));
        }
    }

    Ok(url)
}

fn is_ssrf_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            v4.is_loopback()       // 127.0.0.0/8
                || v4.is_private() // 10/8, 172.16/12, 192.168/16
                || v4.is_link_local() // 169.254/16
                || v4.is_unspecified() // 0.0.0.0
                || v4.is_broadcast() // 255.255.255.255
        }
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback()          // ::1
                || v6.is_unspecified() // ::
        }
    }
}
