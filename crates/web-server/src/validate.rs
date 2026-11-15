//! Host, IP, URL, and identifier validation for outbound server requests.
//!
//! All user- or remote-instance-supplied hosts and resource identifiers pass through
//! here before the server constructs an outbound HTTP request, preventing SSRF against
//! internal/loopback/metadata addresses and path traversal in relayed endpoints.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use reqwest::Url;

/// Returns `true` when `ip` is a globally routable public IP address.
///
/// Rejects loopback, private (RFC 1918 / ULA), link-local (including cloud metadata
/// `169.254.169.254`), unspecified, CGNAT, multicast, documentation, benchmarking,
/// reserved, and IPv4-mapped/compatible IPv6 addresses wrapping non-public IPv4s.
pub(crate) fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_ipv4(v4),
        IpAddr::V6(v6) => is_public_ipv6(v6),
    }
}

fn is_public_ipv4(v4: Ipv4Addr) -> bool {
    let [a, b, c, _] = v4.octets();
    let is_this_network = a == 0;
    let is_cgnat = a == 100 && (b & 0b1100_0000) == 64;
    let is_ietf_protocol = a == 192 && b == 0 && c == 0;
    let is_benchmarking = a == 198 && (b & 0b1111_1110) == 18;
    let is_reserved_or_broadcast = a >= 240;

    !is_this_network
        && !v4.is_loopback()
        && !v4.is_private()
        && !v4.is_link_local()
        && !v4.is_documentation()
        && !v4.is_multicast()
        && !is_cgnat
        && !is_ietf_protocol
        && !is_benchmarking
        && !is_reserved_or_broadcast
}

fn is_public_ipv6(v6: Ipv6Addr) -> bool {
    if let Some(mapped) = v6.to_ipv4_mapped() {
        return is_public_ipv4(mapped);
    }
    if let Some(compat) = v6.to_ipv4()
        && !v6.is_loopback()
        && !v6.is_unspecified()
    {
        return is_public_ipv4(compat);
    }
    let s = v6.segments();
    let is_documentation = s[0] == 0x2001 && s[1] == 0x0db8;
    let is_benchmarking = s[0] == 0x2001 && s[1] == 0x0002 && s[2] == 0;
    let is_discard = s[0] == 0x0100 && s[1] == 0 && s[2] == 0 && s[3] == 0;
    let is_nat64 = (s[0] == 0x0064 && s[1] == 0xff9b && s[2..6] == [0, 0, 0, 0])
        || (s[0] == 0x0064 && s[1] == 0xff9b && s[2] == 0x0001);
    let is_ipv4_translated = s[0..6] == [0, 0, 0, 0, 0xffff, 0];
    let is_6to4 = s[0] == 0x2002;
    let is_teredo = s[0] == 0x2001 && s[1] == 0x0000;

    !v6.is_unspecified()
        && !v6.is_loopback()
        && !v6.is_multicast()
        && !v6.is_unique_local()
        && !v6.is_unicast_link_local()
        && !is_documentation
        && !is_benchmarking
        && !is_discard
        && !is_nat64
        && !is_ipv4_translated
        && !is_6to4
        && !is_teredo
}

/// Returns `true` when `domain` is a syntactically valid, non-internal multi-label hostname.
pub(crate) fn is_public_domain(domain: &str) -> bool {
    if domain.is_empty()
        || domain.len() > 253
        || domain.starts_with('.')
        || domain.ends_with('.')
        || !domain.contains('.')
    {
        return false;
    }

    let mut last_label = "";
    for label in domain.split('.') {
        if label.is_empty()
            || label.len() > 63
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return false;
        }
        last_label = label;
    }

    // Reject single-char TLDs and all-numeric TLDs (e.g. non-canonical IP tricks).
    if last_label.len() < 2 || last_label.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }

    let lower = domain.to_ascii_lowercase();
    !(lower == "localhost"
        || lower.ends_with(".localhost")
        || lower.ends_with(".local")
        || lower.ends_with(".internal")
        || lower.ends_with(".localdomain")
        || lower.ends_with(".home.arpa")
        || lower.ends_with(".in-addr.arpa")
        || lower.ends_with(".ip6.arpa"))
}

fn is_public_host_str(host: &str) -> bool {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = bare.parse::<IpAddr>() {
        is_public_ip(ip)
    } else {
        is_public_domain(host)
    }
}

/// Validates a bare `host` or `host:port` string before interpolating into `https://{host}/...`.
pub(crate) fn host_valid(host: &str) -> bool {
    if host.is_empty()
        || !host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
    {
        return false;
    }

    let (hostname, port_part) = match host.split_once(':') {
        Some((h, p)) => (h, Some(p)),
        None => (host, None),
    };
    if let Some(port) = port_part {
        let Ok(port_num) = port.parse::<u16>() else {
            return false;
        };
        if port_num == 0 {
            return false;
        }
    }
    if hostname.is_empty() || hostname.starts_with('.') || hostname.ends_with('.') {
        return false;
    }

    // Parse through URL parser so non-canonical IPv4 forms (hex/octal/short) are normalized.
    let Ok(parsed) = Url::parse(&format!("https://{host}")) else {
        return false;
    };
    validate_instance_url(&parsed).is_ok()
}

/// Validates a parsed instance base URL against SSRF and URL smuggling.
pub(crate) fn validate_instance_url(url: &Url) -> Result<(), String> {
    if url.scheme() != "https" {
        return Err("only https instance URLs are allowed".to_owned());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("instance URL must not include credentials".to_owned());
    }
    if url.path() != "/" || url.query().is_some() || url.fragment().is_some() {
        return Err("instance URL must not include a path, query, or fragment".to_owned());
    }
    let Some(host) = url.host_str() else {
        return Err("instance URL is missing a host".to_owned());
    };
    if host.ends_with('.') {
        return Err("instance host must not end with a trailing dot".to_owned());
    }
    if !is_public_host_str(host) {
        return Err("instance host is not a valid public host".to_owned());
    }
    Ok(())
}

/// Returns the normalized `host` or `host:port` of a validated instance URL.
pub(crate) fn normalized_host(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default();
    match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    }
}

/// Validates a Fediverse status/account/cursor identifier against path traversal and query injection.
pub(crate) fn id_valid(id: &str) -> bool {
    !id.is_empty()
        && id != "."
        && id != ".."
        && !id.contains("..")
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

fn validate_resolved_addrs(addrs: &[SocketAddr]) -> std::io::Result<()> {
    if addrs.is_empty() || addrs.iter().any(|addr| !is_public_ip(addr.ip())) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "host resolved to a non-public IP address",
        ));
    }
    Ok(())
}

/// DNS resolver that rejects any host resolving to empty or non-public IP addresses.
#[derive(Debug)]
pub(crate) struct PublicDnsResolver;

impl reqwest::dns::Resolve for PublicDnsResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(async move {
            let addrs: Vec<SocketAddr> =
                tokio::net::lookup_host((name.as_str(), 0)).await?.collect();
            validate_resolved_addrs(&addrs)
                .map_err(|err| Box::new(err) as Box<dyn std::error::Error + Send + Sync>)?;
            Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

fn validate_redirect_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str().is_some_and(|host| {
            let candidate = match url.port() {
                Some(port) => format!("{host}:{port}"),
                None => host.to_owned(),
            };
            host_valid(&candidate)
        })
}

fn public_redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= 5 {
            return attempt.error("too many redirects");
        }
        if validate_redirect_url(attempt.url()) {
            attempt.follow()
        } else {
            attempt.error("blocked redirect to non-public target")
        }
    })
}

/// Builds a `reqwest::Client` hardened against DNS-rebinding and open-redirect SSRF.
pub(crate) fn public_http_client(timeout: Duration) -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .timeout(timeout)
        .user_agent(concat!("nodus-web/", env!("CARGO_PKG_VERSION")))
        .dns_resolver(Arc::new(PublicDnsResolver))
        .redirect(public_redirect_policy())
        .build()
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, SocketAddr};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use axum::Router;
    use axum::extract::Query;
    use axum::response::Redirect;
    use axum::routing::get;
    use reqwest::Url;
    use reqwest::dns::Resolve;
    use serde::Deserialize;

    use super::{
        PublicDnsResolver, id_valid, is_public_ip, normalized_host, public_http_client,
        public_redirect_policy, validate_instance_url, validate_redirect_url,
        validate_resolved_addrs,
    };

    #[test]
    fn id_valid_accepts_safe_ids_and_rejects_traversal() {
        for good in ["1", "112233445566", "01HYZ_abc-123", "status.v1"] {
            assert!(id_valid(good), "expected id_valid({good:?}) to be true");
        }
        for bad in [
            "",
            ".",
            "..",
            "../admin",
            "foo..bar",
            "1/2",
            "1\\2",
            "1?admin=true",
            "1&admin=true",
            "1#frag",
            "1%2f2",
            "1 2",
        ] {
            assert!(!id_valid(bad), "expected id_valid({bad:?}) to be false");
        }
    }

    #[test]
    fn is_public_ip_rejects_non_global_v4_and_v6() {
        for public in ["93.184.216.34", "1.1.1.1", "2606:4700:4700::1111"] {
            let ip: IpAddr = public.parse().unwrap();
            assert!(is_public_ip(ip), "expected {public} to be public");
        }
        for private in [
            "0.0.0.0",
            "0.1.2.3",
            "10.0.0.1",
            "100.64.0.1",
            "127.0.0.1",
            "169.254.169.254",
            "172.16.0.1",
            "192.0.0.1",
            "192.0.2.1",
            "192.168.1.1",
            "198.18.0.1",
            "224.0.0.1",
            "240.0.0.1",
            "255.255.255.255",
            "::",
            "::1",
            "fe80::1",
            "fc00::1",
            "fd12:3456::1",
            "ff02::1",
            "2001:db8::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "::ffff:169.254.169.254",
            "::ffff:0:127.0.0.1",
            "::ffff:0:93.184.216.34",
            "2002:7f00:0001::1",
            "2001::1",
            "2001:0:4136:e378:8000:63bf:3fff:fdd2",
        ] {
            let ip: IpAddr = private.parse().unwrap();
            assert!(!is_public_ip(ip), "expected {private} to be rejected");
        }
    }

    #[test]
    fn validate_instance_url_enforces_https_and_no_path_or_userinfo() {
        let ok = Url::parse("https://mastodon.social:8443/").unwrap();
        assert!(validate_instance_url(&ok).is_ok());
        assert_eq!(normalized_host(&ok), "mastodon.social:8443");

        for bad in [
            "http://mastodon.social/",
            "https://user:pass@mastodon.social/",
            "https://mastodon.social/sub",
            "https://mastodon.social/?a=1",
            "https://127.0.0.1/",
            "https://169.254.169.254/",
            "https://localhost/",
            "https://metadata.google.internal/",
        ] {
            let url = Url::parse(bad).unwrap();
            assert!(
                validate_instance_url(&url).is_err(),
                "expected {bad} to fail validation"
            );
        }
    }

    #[test]
    fn resolved_addrs_rejects_empty_non_public_and_mixed_sets() {
        assert!(validate_resolved_addrs(&[]).is_err());

        let public_only: Vec<SocketAddr> = vec!["93.184.216.34:443".parse().unwrap()];
        assert!(validate_resolved_addrs(&public_only).is_ok());

        for bad in [
            vec!["127.0.0.1:443".parse().unwrap()],
            vec!["169.254.169.254:80".parse().unwrap()],
            vec!["[::1]:443".parse().unwrap()],
            vec![
                "93.184.216.34:443".parse().unwrap(),
                "127.0.0.1:443".parse().unwrap(),
            ],
        ] {
            assert!(
                validate_resolved_addrs(&bad).is_err(),
                "expected {bad:?} to be rejected"
            );
        }
    }

    #[test]
    fn redirect_url_validation_allows_https_public_and_blocks_ssrf() {
        for good in [
            "https://mastodon.social/api/v1/statuses/1",
            "https://fosstodon.org:8443/users/alice?page=2",
        ] {
            let url = Url::parse(good).unwrap();
            assert!(
                validate_redirect_url(&url),
                "expected redirect to {good} to be allowed"
            );
        }

        for bad in [
            "http://169.254.169.254/latest/meta-data/",
            "https://127.0.0.1:8080/api/v1/instance",
            "https://localhost/api/v1/instance",
            "https://10.0.0.1/secret",
            "http://mastodon.social/downgrade",
            "https://user:pass@mastodon.social/api/v1/instance",
        ] {
            let url = Url::parse(bad).unwrap();
            assert!(
                !validate_redirect_url(&url),
                "expected redirect to {bad} to be blocked"
            );
        }
    }

    #[tokio::test]
    async fn public_dns_resolver_and_client_block_loopback_connections() {
        let name: reqwest::dns::Name = "localhost".parse().unwrap();
        assert!(PublicDnsResolver.resolve(name).await.is_err());

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_clone = Arc::clone(&hits);
        tokio::spawn(async move {
            while let Ok((_stream, _)) = listener.accept().await {
                hits_clone.fetch_add(1, Ordering::SeqCst);
            }
        });

        let client = public_http_client(Duration::from_secs(5)).unwrap();
        let err = client
            .get(format!(
                "http://localhost:{port}/api/v1/accounts/verify_credentials"
            ))
            .send()
            .await
            .unwrap_err();
        assert!(err.is_connect() || err.is_request());
        assert_eq!(hits.load(Ordering::SeqCst), 0);
    }

    #[derive(Deserialize)]
    struct RedirectParams {
        to: String,
    }

    #[tokio::test]
    async fn public_redirect_policy_blocks_ssrf_redirects_without_connecting() {
        let canary = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let canary_port = canary.local_addr().unwrap().port();
        let canary_hits = Arc::new(AtomicUsize::new(0));
        let canary_hits_clone = Arc::clone(&canary_hits);
        tokio::spawn(async move {
            while let Ok((_stream, _)) = canary.accept().await {
                canary_hits_clone.fetch_add(1, Ordering::SeqCst);
            }
        });

        let app =
            Router::new().route(
                "/redirect",
                get(|Query(params): Query<RedirectParams>| async move {
                    Redirect::temporary(&params.to)
                }),
            );
        let redirect_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let redirect_addr = redirect_listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(redirect_listener, app).await.unwrap();
        });

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .redirect(public_redirect_policy())
            .build()
            .unwrap();

        for target in [
            "http://169.254.169.254/latest/meta-data/".to_owned(),
            format!("https://127.0.0.1:{canary_port}/secret"),
            format!("http://localhost:{canary_port}/secret"),
            "http://mastodon.social/downgrade".to_owned(),
            "https://user:pass@mastodon.social/secret".to_owned(),
        ] {
            let url = Url::parse_with_params(
                &format!("http://{redirect_addr}/redirect"),
                &[("to", target.as_str())],
            )
            .unwrap();
            let err = client.get(url).send().await.unwrap_err();
            assert!(
                err.is_redirect(),
                "expected redirect to {target} to be rejected by redirect policy, got: {err}"
            );
        }
        assert_eq!(canary_hits.load(Ordering::SeqCst), 0);
    }
}
