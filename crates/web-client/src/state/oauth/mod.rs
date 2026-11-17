//! Mastodon sign-in: OAuth 2.0 authorization-code flow with PKCE.
//!
//! The flow, in order — the same one Elk and Phanpy use:
//!
//! 1. register this client with the instance (`POST /api/v1/apps`). It is a public
//!    client, so the `client_secret` in the response is never read, stored or sent;
//! 2. send the browser to `/oauth/authorize` with an S256 challenge and a `state`;
//! 3. the instance redirects back to `/oauth/callback`, where [`finish`] checks
//!    `state` and exchanges the code for a token with the verifier.
//!
//! Two properties are deliberate:
//!
//! * **No secret anywhere.** PKCE replaces the client secret, and the verifier never
//!   leaves this browser, so a leaked `client_id` buys an attacker nothing.
//! * **The pending record lives in `sessionStorage`.** It is per-tab and dies with
//!   the tab, which is exactly the lifetime of a half-finished authorization; the
//!   resulting token goes to `localStorage` through `bridge::save_session`, as before.

use url::Url;

mod api;
mod pending;

pub use pending::Purpose;
mod pkce;

/// The scopes this client actually uses; must stay in step with the scopes the
/// bridge assumes when it restores a session (`state/bridge/session.rs`).
///
/// Mastodon accepts this space-separated form in the registration, the authorization
/// request and the token exchange. Asking for more would make the consent screen
/// worse without buying a feature.
pub(super) const SCOPES: &str = "read write follow";

/// The scopes the moderation screens need, requested only when the operator asks.
///
/// Both the registration and the token must carry these, and Mastodon refuses an
/// authorization request that asks for more than the app was registered with — which
/// is why the upgrade registers a second app rather than widening the first.
#[cfg_attr(
    not(feature = "admin"),
    allow(
        dead_code,
        reason = "the moderation screens are compiled out of this build"
    )
)]
pub(super) const ADMIN_SCOPES: &str = "read write follow admin:read admin:write";

/// The scopes requested for an OAuth [`pending::Purpose`].
///
/// Keeping this mapping in one place ensures app registration, the authorization
/// redirect, and the code-to-token exchange always agree on the scopes.
pub(super) const fn scopes_for(purpose: pending::Purpose) -> &'static str {
    match purpose {
        pending::Purpose::SignIn => SCOPES,
        pending::Purpose::Admin => ADMIN_SCOPES,
    }
}

/// Where the instance sends the browser back.
///
/// The same string is registered as `redirect_uris` and echoed to `/oauth/token`:
/// Mastodon requires all three to match exactly, so it is defined once, in
/// `shared::router`, where the route table that parses the address bar can reach it too.
pub(crate) const CALLBACK_PATH: &str = crate::shared::router::CALLBACK_PATH;

/// A canonical instance address: the host (with a non-default port) and the scheme its
/// API is reached over.
///
/// The host is the whole identity shared by the session record, the admin grant and the
/// request builder. Keeping the scheme beside it means one parse decides both, rather
/// than the grant accepting a loopback address while the request builder quietly sends
/// it to `https://`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Endpoint {
    /// Canonical `host[:port]`, lower-cased, without a trailing slash or default port.
    pub(crate) host: String,
    /// `http` for a loopback instance, `https` for everything else.
    pub(crate) scheme: &'static str,
}

impl Endpoint {
    /// The absolute URL for `path`, which must begin with `/` (or be empty for the root).
    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}://{}{}", self.scheme, self.host, path)
    }
}

/// Starts the flow: register the client, stash the pending record, leave the page.
///
/// On success the browser is navigating to the instance, so the caller mostly sees
/// the error branch; `Ok(())` is returned right after `location.assign`.
///
/// # Errors
///
/// A message fit for an inline alert: bad host input, no usable page origin, the
/// instance refusing app registration, Web Crypto unavailable, or `sessionStorage`
/// blocked. Nothing is stored and no navigation happens when this returns `Err`.
pub async fn begin(host: &str) -> Result<(), String> {
    begin_with(host, pending::Purpose::SignIn, "", "").await
}

/// Starts the admin grant: same flow, wider scopes, different purpose.
///
/// Nothing about the session changes if the operator refuses; the callback simply has
/// no token to store.
#[cfg_attr(
    not(feature = "admin"),
    allow(
        dead_code,
        reason = "the moderation screens are compiled out of this build"
    )
)]
pub async fn begin_admin(host: &str, account_id: &str, return_to: &str) -> Result<(), String> {
    begin_with(host, pending::Purpose::Admin, account_id, return_to).await
}

/// The shared body of both flows.
async fn begin_with(
    host: &str,
    purpose: pending::Purpose,
    account_id: &str,
    return_to: &str,
) -> Result<(), String> {
    let scopes = scopes_for(purpose);
    let endpoint = normalise_host(host)?;
    let origin = origin()?;
    let redirect_uri = format!("{origin}{CALLBACK_PATH}");
    let app = api::register_app(&endpoint, &origin, &redirect_uri, scopes).await?;

    let verifier = pkce::verifier()?;
    let challenge = pkce::challenge(&verifier).await?;
    let state = pkce::state()?;

    // Persist before redirecting: if this fails the flow cannot be completed, and the
    // user should hear it here rather than after a round trip to the instance.
    pending::save(&pending::Pending {
        host: endpoint.host.clone(),
        client_id: app.client_id.clone(),
        client_secret: app.client_secret.clone(),
        verifier,
        state: state.clone(),
        redirect_uri: redirect_uri.clone(),
        purpose,
        account_id: account_id.to_owned(),
        return_to: return_to.to_owned(),
    })?;

    let url = authorize_url(
        &endpoint,
        &app.client_id,
        &redirect_uri,
        &challenge,
        &state,
        scopes,
    )?;
    window()?
        .location()
        .assign(&url)
        .map_err(|_| "Could not open the instance's authorization page.".to_owned())
}

/// Completes the flow from the callback's query string.
///
/// Returns `(host, access_token)`; the caller hands both to
/// [`crate::state::bridge::sign_in`], which verifies the token against the instance
/// before anything is installed or saved.
///
/// # Errors
///
/// A message fit for an inline alert: the instance refused (`error=access_denied`),
/// the callback is missing its code, there is no pending record in this tab, `state`
/// does not match, or the token exchange was rejected. The pending record is cleared
/// in every case, including these.
pub async fn finish(search: &str) -> Result<Finished, String> {
    // Single use, and cleared before any early return: an authorization code must not
    // be replayable by a reload or a shared URL, and an abandoned attempt must not
    // leave a verifier sitting in storage.
    let record = pending::take();

    let params = web_sys::UrlSearchParams::new_with_str(search)
        .map_err(|_| "The callback URL could not be read.".to_owned())?;

    // Mastodon reports a refusal (the user pressed "Deny") as `error=access_denied`,
    // sometimes with a human-readable `error_description`.
    if let Some(error) = params.get("error") {
        let reason = params.get("error_description").unwrap_or(error);
        return Err(format!("The instance did not authorize the app: {reason}"));
    }

    let code = params
        .get("code")
        .filter(|code| !code.is_empty())
        .ok_or_else(|| "The instance's callback carried no authorization code.".to_owned())?;
    let state = params.get("state").unwrap_or_default();

    let record = record.ok_or_else(|| {
        "This sign-in did not start in this tab, so there is no pending request to compare \
         the instance's answer against. Start again from the sign-in page."
            .to_owned()
    })?;
    // The CSRF check: only an instance that saw our authorization request can echo the
    // `state` this tab generated.
    if state != record.state {
        return Err(
            "The callback's `state` does not match this tab's sign-in, so the answer \
                    was discarded. Start again from the sign-in page."
                .to_owned(),
        );
    }

    let token = api::exchange_code(&record, &code, scopes_for(record.purpose)).await?;
    Ok(Finished {
        host: record.host,
        token,
        purpose: record.purpose,
        account_id: record.account_id,
        return_to: (!record.return_to.is_empty()).then_some(record.return_to),
    })
}

/// What the callback learned, and what it is allowed to do with it.
#[derive(Debug)]
pub struct Finished {
    /// The instance the token belongs to.
    pub host: String,
    /// The access token.
    pub token: String,
    /// Whether this replaces the session or only adds an admin grant.
    pub purpose: Purpose,
    /// The account an admin grant was requested for; empty for a sign-in.
    pub account_id: String,
    /// The in-app path to return to after an admin grant, when one was recorded.
    pub return_to: Option<String>,
}

/// The authorization URL, with `redirect_uri` percent-encoded by the `url` crate.
fn authorize_url(
    endpoint: &Endpoint,
    client_id: &str,
    redirect_uri: &str,
    challenge: &str,
    state: &str,
    scopes: &str,
) -> Result<String, String> {
    let mut url = Url::parse(&endpoint.url("/oauth/authorize"))
        .map_err(|error| format!("Could not build the authorization URL: {error}"))?;
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", scopes)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state);
    Ok(url.into())
}

/// Trims a pasted address down to a canonical [`Endpoint`].
///
/// This is a security boundary, not input polish: the result builds the URLs the code
/// and token travel to, so credentials, a path, `?`, `#` or whitespace are refused
/// rather than interpreted. A bare host is the common case; a full URL is accepted
/// because that is what people paste, and http is allowed only where the sdk allows it
/// — a loopback instance.
pub(crate) fn normalise_host(input: &str) -> Result<Endpoint, String> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err("Enter your instance's domain, for example mastodon.social.".to_owned());
    }
    if raw.contains("://") {
        endpoint_from_url(raw)
    } else {
        endpoint_from_bare_host(raw)
    }
}

/// Parses a bare `host`, `host:port`, `host/path` and reports why it cannot be used.
///
/// The sentinel scheme is `https` only so the `url` crate can find the host and port;
/// the effective scheme is chosen afterwards from the parsed host.
fn endpoint_from_bare_host(raw: &str) -> Result<Endpoint, String> {
    let parsed = Url::parse(&format!("https://{raw}")).map_err(|_| unusable(raw))?;
    reject_credentials_and_path(&parsed, raw)?;

    let scheme = if is_loopback(&parsed) { "http" } else { "https" };
    // Reparse under the effective scheme so the default port is dropped from the
    // canonical host (`localhost:80` becomes `localhost`, not `localhost:80`).
    let host = canonical_host(&parsed).ok_or_else(|| unusable(raw))?;
    let reparsed = Url::parse(&format!("{scheme}://{host}")).map_err(|_| unusable(raw))?;
    let host = canonical_host(&reparsed).ok_or_else(|| unusable(raw))?;
    Ok(Endpoint { host, scheme })
}

/// Parses a full URL, keeping an explicit `https` even for a loopback host.
fn endpoint_from_url(raw: &str) -> Result<Endpoint, String> {
    let url = Url::parse(raw).map_err(|_| unusable(raw))?;
    let scheme = match url.scheme() {
        "https" => "https",
        "http" if is_loopback(&url) => "http",
        "http" => {
            return Err(
                "Plain http is only supported for a local instance; use https:// for a \
                 remote domain."
                    .to_owned(),
            );
        }
        other => {
            return Err(format!(
                "\"{other}://\" addresses are not supported. Enter the instance's domain, \
                 for example mastodon.social."
            ));
        }
    };
    reject_credentials_and_path(&url, raw)?;
    let host = canonical_host(&url).ok_or_else(|| unusable(raw))?;
    Ok(Endpoint { host, scheme })
}

/// Rejects anything that is not an instance root: credentials would end up in every
/// request URL, and a path or query silently points at a different resource.
fn reject_credentials_and_path(url: &Url, raw: &str) -> Result<(), String> {
    if !url.username().is_empty() || url.password().is_some() {
        return Err(unusable(raw));
    }
    if !matches!(url.path(), "" | "/") {
        return Err(format!(
            "\"{raw}\" points at a page, not an instance. Enter just the domain, for \
             example mastodon.social."
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(unusable(raw));
    }
    Ok(())
}

/// The canonical `host[:port]`, with the scheme's default port omitted.
fn canonical_host(url: &Url) -> Option<String> {
    let host = url.host_str()?;
    Some(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    })
}

/// The same hosts the sdk accepts over plain http: the local machine and nothing else.
fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(domain)) => {
            domain == "localhost" || domain.ends_with(".localhost")
        }
        Some(url::Host::Ipv4(address)) => address.is_loopback(),
        Some(url::Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    }
}

fn unusable(raw: &str) -> String {
    format!(
        "\"{raw}\" is not an instance domain. Enter just the domain, for example \
         mastodon.social."
    )
}

/// This page's origin, which the instance needs as the redirect target.
///
/// `file://` pages report the literal string `null`, which cannot be registered and
/// would make the redirect fail after consent — so it is refused up front.
fn origin() -> Result<String, String> {
    let origin = window()?.location().origin().unwrap_or_default();
    if origin.starts_with("https://") || origin.starts_with("http://") {
        Ok(origin)
    } else {
        Err(
            "This page's origin cannot be used as a redirect target; serve the app over \
             http(s)."
                .to_owned(),
        )
    }
}

/// The browser window, or a message the user can act on.
fn window() -> Result<web_sys::Window, String> {
    web_sys::window().ok_or_else(|| "Sign-in needs a browser window.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_purpose_maps_to_admin_scopes() {
        assert_eq!(scopes_for(Purpose::SignIn), SCOPES);
        assert_eq!(scopes_for(Purpose::Admin), ADMIN_SCOPES);
        assert!(scopes_for(Purpose::Admin).contains("admin:read"));
        assert!(scopes_for(Purpose::Admin).contains("admin:write"));
    }

    #[test]
    fn authorize_url_carries_admin_scopes_for_admin_purpose() {
        let endpoint = normalise_host("mastodon.example").unwrap();
        let url = authorize_url(
            &endpoint,
            "client-id",
            "https://nodus.example/oauth/callback",
            "challenge",
            "state",
            scopes_for(Purpose::Admin),
        )
        .unwrap();
        let parsed = Url::parse(&url).unwrap();
        let scope = parsed
            .query_pairs()
            .find(|(key, _)| key == "scope")
            .map(|(_, value)| value.into_owned());
        assert_eq!(scope.as_deref(), Some(ADMIN_SCOPES));
    }

    fn endpoint(host: &str, scheme: &'static str) -> Endpoint {
        Endpoint {
            host: host.to_owned(),
            scheme,
        }
    }

    #[test]
    fn bare_host_is_canonical_https() {
        assert_eq!(
            normalise_host("mastodon.social").unwrap(),
            endpoint("mastodon.social", "https")
        );
        assert_eq!(
            normalise_host("  Mastodon.Social/  ").unwrap(),
            endpoint("mastodon.social", "https")
        );
    }

    #[test]
    fn https_url_loses_its_scheme_and_slash_but_not_its_host() {
        assert_eq!(
            normalise_host("https://mastodon.social/").unwrap(),
            endpoint("mastodon.social", "https")
        );
    }

    #[test]
    fn loopback_defaults_to_http_and_keeps_a_non_default_port() {
        assert_eq!(
            normalise_host("localhost:3000").unwrap(),
            endpoint("localhost:3000", "http")
        );
        assert_eq!(
            normalise_host("http://localhost:3000").unwrap(),
            endpoint("localhost:3000", "http")
        );
        assert_eq!(
            normalise_host("http://127.0.0.1:8080/").unwrap(),
            endpoint("127.0.0.1:8080", "http")
        );
        assert_eq!(
            normalise_host("http://[::1]:8080").unwrap(),
            endpoint("[::1]:8080", "http")
        );
    }

    #[test]
    fn loopback_may_explicitly_ask_for_https() {
        assert_eq!(
            normalise_host("https://localhost:3443").unwrap(),
            endpoint("localhost:3443", "https")
        );
    }

    #[test]
    fn default_ports_are_dropped() {
        assert_eq!(
            normalise_host("https://mastodon.social:443").unwrap(),
            endpoint("mastodon.social", "https")
        );
        assert_eq!(
            normalise_host("localhost:80").unwrap(),
            endpoint("localhost", "http")
        );
    }

    #[test]
    fn remote_plain_http_is_refused() {
        let error = normalise_host("http://mastodon.social").unwrap_err();
        assert!(error.contains("https://"), "{error}");
    }

    #[test]
    fn non_http_schemes_are_refused() {
        assert!(normalise_host("ftp://mastodon.social").is_err());
        assert!(normalise_host("javascript:alert(1)").is_err());
    }

    #[test]
    fn paths_queries_and_credentials_are_refused() {
        assert!(normalise_host("https://mastodon.social/about").is_err());
        assert!(normalise_host("mastodon.social/about").is_err());
        assert!(normalise_host("https://mastodon.social/?query").is_err());
        assert!(normalise_host("https://user:pass@mastodon.social").is_err());
    }

    #[test]
    fn empty_and_malformed_hosts_are_refused() {
        assert!(normalise_host("").is_err());
        assert!(normalise_host("   ").is_err());
        assert!(normalise_host("mastodon.social:notaport").is_err());
    }
}
