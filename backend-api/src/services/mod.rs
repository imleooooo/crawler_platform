pub mod crawler;
pub mod queue;
pub mod s3;

// Re-export sanitize_bucket_name for convenience
pub use s3::sanitize_bucket_name;

/// Validate a caller-supplied URL for use as a crawl target.
///
/// Rejects:
/// - Non-http/https schemes (file://, ftp://, etc.)
/// - Hostnames or literal IPs that resolve to loopback, private, link-local,
///   unique-local, or unspecified ranges (IPv4 and IPv6).
///
/// DNS resolution is performed so that attacker-controlled hostnames that
/// point to internal addresses are caught. Note that DNS rebinding can still
/// bypass this check after validation; this check is a defence-in-depth
/// measure, not a complete SSRF prevention strategy.
///
/// Returns the parsed `url::Url` on success.
pub async fn validate_url(raw: &str) -> Result<url::Url, String> {
    let url = url::Url::parse(raw).map_err(|e| format!("Invalid URL '{}': {}", raw, e))?;

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

    // Check literal IP addresses first (no DNS needed)
    let ip_str = host.trim_matches(|c| c == '[' || c == ']');
    if let Ok(ip) = ip_str.parse::<std::net::IpAddr>() {
        if is_ssrf_ip(ip) {
            return Err(format!(
                "Requests to private or reserved address '{}' are not allowed",
                ip
            ));
        }
        // Literal IP is public — no DNS needed
        return Ok(url);
    }

    // Hostname: resolve via DNS and check every returned address.
    // A 5s cap prevents a slow/unresponsive resolver from blocking the request.
    let lookup_addr = format!("{}:80", host);
    let addrs = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::net::lookup_host(&lookup_addr),
    )
    .await
    .map_err(|_| format!("DNS lookup for '{}' timed out", host))?
    .map_err(|e| format!("Could not resolve host '{}': {}", host, e))?;

    for addr in addrs {
        if is_ssrf_ip(addr.ip()) {
            return Err(format!(
                "Host '{}' resolves to a private or reserved address and is not allowed",
                host
            ));
        }
    }

    Ok(url)
}

fn is_ssrf_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            v4.is_loopback()          // 127.0.0.0/8
                || v4.is_private()    // 10/8, 172.16/12, 192.168/16
                || v4.is_link_local() // 169.254/16
                || v4.is_unspecified() // 0.0.0.0
                || v4.is_broadcast()  // 255.255.255.255
        }
        std::net::IpAddr::V6(v6) => {
            let s = v6.segments();
            v6.is_loopback()              // ::1
                || v6.is_unspecified()    // ::
                || (s[0] & 0xffc0) == 0xfe80 // fe80::/10  link-local
                || (s[0] & 0xfe00) == 0xfc00 // fc00::/7   unique-local (ULA)
                || (s[0] & 0xff00) == 0xff00 // ff00::/8   multicast
        }
    }
}
