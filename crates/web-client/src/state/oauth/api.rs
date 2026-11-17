//! The two requests the flow makes to the instance's own API: app registration and
//! the code-to-token exchange.
//!
//! Both are form-encoded `POST`s through `gloo-net` (already in the graph via the
//! transports), so the browser's `fetch` and CORS rules apply unchanged. JSON is
//! parsed with the browser's own parser: `serde_json` is not a dependency of this
//! crate, and adding it for two scalars would be a larger change than the feature.

use gloo_net::http::{Headers, Request};
use wasm_bindgen::JsValue;

use super::Endpoint;
use super::pending::Pending;

/// Name the instance shows on its "authorized applications" page.
const CLIENT_NAME: &str = "nodus";

/// How much of a non-JSON error body is quoted back to the user. Enough to recognise
/// a reverse-proxy error page, short enough for an inline alert.
const MAX_ERROR_CHARS: usize = 200;

/// What app registration hands back and the exchange needs.
pub(super) struct App {
    pub(super) client_id: String,
    /// Not a secret in any useful sense — it is bound to a public client whose codes
    /// are proven with PKCE, and the instance issues it to anyone who asks. It is here
    /// because Mastodon's `/oauth/token` demands client authentication: sending only
    /// `client_id` answers `401 invalid_client`, which is what broke sign-in before.
    pub(super) client_secret: String,
}

/// Registers the app with `endpoint` and returns its credentials.
pub(super) async fn register_app(
    endpoint: &Endpoint,
    origin: &str,
    redirect_uri: &str,
    scopes: &str,
) -> Result<App, String> {
    let url = endpoint.url("/api/v1/apps");
    let json = post_form(
        &url,
        &[
            ("client_name", CLIENT_NAME),
            ("redirect_uris", redirect_uri),
            ("scopes", scopes),
            ("website", origin),
        ],
    )
    .await?;
    let client_id = string_field(&json, "client_id")
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            format!("{url} returned no client id. The instance may have app registration disabled.")
        })?;
    let client_secret = string_field(&json, "client_secret").unwrap_or_default();
    Ok(App {
        client_id,
        client_secret,
    })
}

/// Exchanges an authorization code for an access token.
///
/// The verifier proves this browser is the one that started the flow; `redirect_uri`
/// must repeat the registered value byte-for-byte, which is why it is stored rather
/// than rebuilt. `scopes` must match the scopes requested in app registration and
/// authorization (e.g. `ADMIN_SCOPES` for an admin grant).
pub(super) async fn exchange_code(
    record: &Pending,
    code: &str,
    scopes: &str,
) -> Result<String, String> {
    let endpoint = super::normalise_host(&record.host)?;
    let url = endpoint.url("/oauth/token");
    let form = token_exchange_form(record, code, scopes);
    let json = post_form(&url, &form).await?;
    string_field(&json, "access_token")
        .filter(|token| !token.is_empty())
        .ok_or_else(|| format!("{url} returned no access token."))
}

/// Builds the form-encoded parameters for `POST /oauth/token`.
pub(super) fn token_exchange_form<'a>(
    record: &'a Pending,
    code: &'a str,
    scopes: &'a str,
) -> [(&'static str, &'a str); 7] {
    [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("client_id", record.client_id.as_str()),
        ("client_secret", record.client_secret.as_str()),
        ("redirect_uri", record.redirect_uri.as_str()),
        ("code_verifier", record.verifier.as_str()),
        ("scope", scopes),
    ]
}

/// `POST`s a form-encoded body and returns the parsed JSON response.
///
/// A non-2xx response becomes an `Err` carrying the instance's own reason, which is
/// the only part of a failed exchange a user can act on.
async fn post_form(url: &str, form: &[(&str, &str)]) -> Result<JsValue, String> {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(form.iter().copied())
        .finish();
    let headers = Headers::new();
    headers.set("Content-Type", "application/x-www-form-urlencoded");

    let request = Request::post(url)
        .headers(headers)
        .body(body)
        .map_err(|error| format!("Could not build the request to {url}: {error}"))?;
    let response = request
        .send()
        .await
        .map_err(|error| format!("Could not reach {url}: {error}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| format!("Reading the response from {url} failed: {error}"))?;

    let json = js_sys::JSON::parse(&text).unwrap_or(JsValue::UNDEFINED);
    if !(200..300).contains(&status) {
        // Mastodon answers a refused exchange with
        // `{"error": "invalid_grant", "error_description": "…"}`; a proxy in front of
        // it may answer with HTML, which is still more useful than a bare status.
        let reason = string_field(&json, "error_description")
            .or_else(|| string_field(&json, "error"))
            .unwrap_or_else(|| text.chars().take(MAX_ERROR_CHARS).collect());
        return Err(format!("{url} responded {status}: {reason}"));
    }
    if json.is_undefined() {
        return Err(format!("{url} returned a body that is not JSON."));
    }
    Ok(json)
}

/// A string property of a JSON object, if it is there and is a string.
fn string_field(value: &JsValue, name: &str) -> Option<String> {
    js_sys::Reflect::get(value, &JsValue::from_str(name))
        .ok()?
        .as_string()
}

#[cfg(test)]
mod tests {
    use super::super::pending::{Pending, Purpose};
    use super::super::{ADMIN_SCOPES, SCOPES, scopes_for};
    use super::token_exchange_form;

    fn sample_pending(purpose: Purpose) -> Pending {
        Pending {
            host: "mastodon.example".to_owned(),
            client_id: "client-id".to_owned(),
            client_secret: "client-secret".to_owned(),
            verifier: "verifier-value".to_owned(),
            state: "state-value".to_owned(),
            redirect_uri: "https://nodus.example/oauth/callback".to_owned(),
            purpose,
            account_id: if purpose == Purpose::Admin {
                "account-1".to_owned()
            } else {
                String::new()
            },
            return_to: if purpose == Purpose::Admin {
                "/moderation/reports".to_owned()
            } else {
                String::new()
            },
        }
    }

    #[test]
    fn admin_exchange_form_requests_admin_scopes() {
        let pending = sample_pending(Purpose::Admin);
        let form = token_exchange_form(&pending, "auth-code", scopes_for(pending.purpose));
        assert!(form.contains(&("scope", ADMIN_SCOPES)));
    }

    #[test]
    fn sign_in_exchange_form_requests_standard_scopes() {
        let pending = sample_pending(Purpose::SignIn);
        let form = token_exchange_form(&pending, "auth-code", scopes_for(pending.purpose));
        assert!(form.contains(&("scope", SCOPES)));
    }
}
