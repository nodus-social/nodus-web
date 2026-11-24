//! The browser half of Web Push: one service worker, one `PushManager` subscription, and the
//! requests that keep the browser's subscription and the server's row naming the same
//! endpoint.
//!
//! The order in [`unsubscribe`] is the point of this module. The server row is deleted first
//! and the browser only drops its subscription once the server has answered 204. When the
//! delete fails the browser subscription is kept, so the two sides can never disagree about
//! which of them is authoritative: the server still refuses delivery to the endpoint it
//! removed, or still holds the one the browser kept.
//!
//! Nothing here runs until the reader asks: subscribing registers the worker, which is why
//! registration lives in [`subscribe`] and not at sign-in.

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::JsValue;
use web_sys::{PushSubscription, PushSubscriptionOptionsInit, ServiceWorkerRegistration};

/// Where the worker is served. The SSR host answers it with `Service-Worker-Allowed: /`.
pub const SCRIPT: &str = "/sw.js";

/// The sentence a browser without a push subscription API gets, in place of the card.
pub const UNSUPPORTED: &str =
    "This browser cannot receive push notifications: it has no PushManager or no service worker.";

/// The alert kinds this instance delivers, in the order the card lists them.
///
/// The sender in `crates/api/src/webpush.rs` filters on exactly these four names.
pub const KINDS: [&str; 4] = ["mention", "favourite", "reblog", "follow"];

/// Which kinds to deliver, as the wire spells them.
///
/// Every kind starts on, matching the server's own default for a new subscription. Kinds the
/// object does not mention keep that default, so a row written by a client that only knows
/// some of the kinds still parses here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alerts {
    #[serde(default = "enabled")]
    pub mention: bool,
    #[serde(default = "enabled")]
    pub favourite: bool,
    #[serde(default = "enabled")]
    pub reblog: bool,
    #[serde(default = "enabled")]
    pub follow: bool,
}

const fn enabled() -> bool {
    true
}

impl Default for Alerts {
    fn default() -> Self {
        Self {
            mention: true,
            favourite: true,
            reblog: true,
            follow: true,
        }
    }
}

impl Alerts {
    /// The flag for one wire name. An unknown name is off, never on.
    #[must_use]
    pub fn get(&self, kind: &str) -> bool {
        match kind {
            "mention" => self.mention,
            "favourite" => self.favourite,
            "reblog" => self.reblog,
            "follow" => self.follow,
            _ => false,
        }
    }

    /// Sets one flag by wire name. An unknown name changes nothing.
    pub fn set(&mut self, kind: &str, value: bool) {
        match kind {
            "mention" => self.mention = value,
            "favourite" => self.favourite = value,
            "reblog" => self.reblog = value,
            "follow" => self.follow = value,
            _ => {}
        }
    }
}

/// What `GET /api/v1/push/subscription` answers while subscribed.
#[derive(Debug, Clone, Deserialize)]
pub struct Subscription {
    pub endpoint: String,
    /// base64url of an uncompressed P-256 point: the VAPID public key.
    pub server_key: String,
    pub alerts: Alerts,
}

/// The body `POST` and `PUT` take: Mastodon's `subscription`/`data` nesting, not a flat object.
#[derive(Debug, Serialize)]
struct Request<'a> {
    subscription: Bound<'a>,
    data: Data,
}

#[derive(Debug, Serialize)]
struct Bound<'a> {
    endpoint: &'a str,
    keys: Keys<'a>,
}

#[derive(Debug, Serialize)]
struct Keys<'a> {
    p256dh: &'a str,
    auth: &'a str,
}

#[derive(Debug, Serialize)]
struct Data {
    alerts: Alerts,
}

/// Decodes the VAPID key from base64url into the bytes `applicationServerKey` wants.
///
/// The key arrives base64url, whose alphabet trades `+`/`/` for `-`/`_`. `atob` only knows
/// standard base64: it rejects `-` and `_` outright, so a key containing either (most of
/// them, since a 65-byte point encodes to 87 characters) would fail to decode. Translating
/// the two characters and restoring the stripped padding before decoding is what makes this
/// work.
#[must_use]
pub fn decode_base64url(input: &str) -> Option<Vec<u8>> {
    let input = input.trim().trim_end_matches('=');
    if input.is_empty() {
        return None;
    }
    let standard = input.replace('-', "+").replace('_', "/");
    let padded = match standard.len() % 4 {
        0 => standard,
        2 => format!("{standard}=="),
        3 => format!("{standard}="),
        _ => return None,
    };
    base64::engine::general_purpose::STANDARD.decode(padded).ok()
}

fn window() -> Option<web_sys::Window> {
    web_sys::window()
}

fn has(object: &JsValue, property: &str) -> bool {
    js_sys::Reflect::has(object, &JsValue::from_str(property)).unwrap_or(false)
}

/// Whether this browser can subscribe at all: it needs both interfaces, and `serviceWorker`
/// only exists in a secure context.
#[must_use]
pub fn supported() -> bool {
    let Some(window) = window() else {
        return false;
    };
    has(window.as_ref(), "PushManager") && has(window.navigator().as_ref(), "serviceWorker")
}

fn container() -> Result<web_sys::ServiceWorkerContainer, String> {
    let window = window().ok_or_else(|| String::from(UNSUPPORTED))?;
    if !has(window.navigator().as_ref(), "serviceWorker") {
        return Err(String::from(UNSUPPORTED));
    }
    Ok(window.navigator().service_worker())
}

fn js_error(error: &JsValue) -> String {
    error
        .as_string()
        .or_else(|| {
            error
                .dyn_ref::<js_sys::Error>()
                .map(|error| String::from(error.message()))
        })
        .unwrap_or_else(|| String::from("the browser refused the push subscription"))
}

async fn resolved(promise: js_sys::Promise) -> Result<JsValue, String> {
    wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(|error| js_error(&error))
}

/// The existing registration, without creating one.
async fn registered() -> Result<Option<ServiceWorkerRegistration>, String> {
    let answer = resolved(container()?.get_registration()).await?;
    if answer.is_null() || answer.is_undefined() {
        return Ok(None);
    }
    answer
        .dyn_into::<ServiceWorkerRegistration>()
        .map(Some)
        .map_err(|_| String::from("the browser answered with something other than a registration"))
}

/// The subscription the browser currently holds, if any.
pub async fn subscribed() -> bool {
    if !supported() {
        return false;
    }
    let Ok(Some(registration)) = registered().await else {
        return false;
    };
    let Ok(manager) = registration.push_manager() else {
        return false;
    };
    let Ok(promise) = manager.get_subscription() else {
        return false;
    };
    let Ok(answer) = resolved(promise).await else {
        return false;
    };
    !answer.is_null() && !answer.is_undefined()
}

fn keys(subscription: &PushSubscription) -> Result<(String, String), String> {
    let key = |name| -> Result<String, String> {
        let buffer = subscription
            .get_key(name)
            .map_err(|error| js_error(&error))?
            .ok_or_else(|| String::from("the browser's subscription is missing an encryption key"))?;
        Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(js_sys::Uint8Array::new(&buffer).to_vec()))
    };
    Ok((
        key(web_sys::PushEncryptionKeyName::P256dh)?,
        key(web_sys::PushEncryptionKeyName::Auth)?,
    ))
}

fn push_url() -> Result<url::Url, String> {
    super::instance_transport()
        .ok_or_else(|| String::from("No active instance connection"))?
        .url("/api/v1/push/subscription")
        .map_err(|error| error.to_string())
}

/// Reads the server's row, or `None` when this account has no subscription.
pub async fn read() -> Result<Option<Subscription>, String> {
    let transport =
        super::instance_transport().ok_or_else(|| String::from("No active instance connection"))?;
    match transport
        .fetch::<Subscription>(transport.get(push_url()?))
        .await
    {
        Ok(subscription) => Ok(Some(subscription)),
        Err(nodus_mastodon::Error::Status { status: 404, .. }) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

/// Registers the worker, subscribes, and records the subscription on the server.
///
/// The server is asked for its key first: a subscription built against the wrong key is
/// useless, and the sentence saying push is off belongs here rather than after the browser
/// has already been handed a subscription nobody can deliver to.
pub async fn subscribe(alerts: Alerts) -> Result<(), String> {
    let transport =
        super::instance_transport().ok_or_else(|| String::from("No active instance connection"))?;
    let known = read()
        .await?
        .ok_or_else(|| String::from("This instance does not offer push notifications"))?;
    let key = decode_base64url(&known.server_key)
        .ok_or_else(|| String::from("The instance sent a server key that is not base64url"))?;

    let registration = resolved(container()?.register(SCRIPT))
        .await?
        .dyn_into::<ServiceWorkerRegistration>()
        .map_err(|_| String::from("the browser answered with something other than a registration"))?;
    let manager = registration
        .push_manager()
        .map_err(|error| js_error(&error))?;

    let options = PushSubscriptionOptionsInit::new();
    options.set_user_visible_only(true);
    let key: JsValue = js_sys::Uint8Array::from(key.as_slice()).into();
    options.set_application_server_key(&key);

    let subscription = resolved(
        manager
            .subscribe_with_options(&options)
            .map_err(|error| js_error(&error))?,
    )
    .await?
    .dyn_into::<PushSubscription>()
    .map_err(|_| String::from("the browser answered with something other than a subscription"))?;
    let (p256dh, auth) = keys(&subscription)?;
    let body = Request {
        subscription: Bound {
            endpoint: &subscription.endpoint(),
            keys: Keys {
                p256dh: &p256dh,
                auth: &auth,
            },
        },
        data: Data { alerts },
    };
    let body = serde_json::to_vec(&body).map_err(|error| error.to_string())?;
    let answer = transport
        .send_empty(transport.post_body(push_url()?, "application/json", body))
        .await;
    if let Err(error) = answer {
        // The server refused the row the browser now holds: undo it, so the reader can retry
        // from a state both sides agree on.
        let _ = subscription.unsubscribe();
        return Err(refusal(&error));
    }
    Ok(())
}

/// Records new alert flags. Only `data.alerts` is sent: the server keeps every kind it was
/// not told about.
pub async fn update(alerts: Alerts) -> Result<(), String> {
    let transport =
        super::instance_transport().ok_or_else(|| String::from("No active instance connection"))?;
    let body = serde_json::json!({ "data": { "alerts": alerts } });
    let body = serde_json::to_vec(&body).map_err(|error| error.to_string())?;
    transport
        .send_empty(transport.put_body(push_url()?, body))
        .await
        .map_err(|error| refusal(&error))
}

/// Deletes the server's row first, and only then drops the browser's subscription.
///
/// A failed delete stops here and leaves the browser subscribed. Unsubscribing first would
/// leave a row the server still pushes to with no browser behind it, and no way for this
/// device to name the endpoint that should go.
pub async fn unsubscribe() -> Result<(), String> {
    let transport =
        super::instance_transport().ok_or_else(|| String::from("No active instance connection"))?;
    let Some(registration) = registered().await? else {
        return Ok(());
    };
    let Ok(manager) = registration.push_manager() else {
        return Ok(());
    };
    let promise = manager.get_subscription().map_err(|error| js_error(&error))?;
    let held = resolved(promise).await?;
    if held.is_null() || held.is_undefined() {
        return Ok(());
    }
    let subscription = held
        .dyn_into::<PushSubscription>()
        .map_err(|_| String::from("the browser answered with something other than a subscription"))?;

    // A DELETE with a body: the server removes the row it is told about, so the endpoint
    // travels as the same `subscription[endpoint]` field the other verbs take.
    transport
        .send_empty(transport.delete_form(push_url()?, &[("subscription[endpoint]", subscription.endpoint())]))
        .await
        .map_err(|error| refusal(&error))?;

    let promise = subscription.unsubscribe().map_err(|error| js_error(&error))?;
    resolved(promise).await.map(|_| ())
}

/// The server's own sentence when it refused, since these refusals are printed as written
/// (a 501 names the variable an operator must set).
fn refusal(error: &nodus_mastodon::Error) -> String {
    match error {
        nodus_mastodon::Error::Status { message, .. } if !message.trim().is_empty() => {
            message.clone()
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_decodes_the_url_safe_alphabet() {
        assert_eq!(decode_base64url("SGVsbG8").as_deref(), Some(&b"Hello"[..]));
        assert_eq!(decode_base64url("SGVsbG8h").as_deref(), Some(&b"Hello!"[..]));
        assert_eq!(decode_base64url("_-8").as_deref(), Some(&[255, 239][..]));
        assert_eq!(decode_base64url("SGVsbG8=").as_deref(), Some(&b"Hello"[..]));
    }

    #[test]
    fn base64url_refuses_what_is_not_base64() {
        assert_eq!(decode_base64url(""), None);
        assert_eq!(decode_base64url("S"), None);
        assert_eq!(decode_base64url("not base64!"), None);
    }

    #[test]
    fn alerts_round_trip_by_wire_name() {
        let mut alerts = Alerts::default();
        assert_eq!(alerts.get("reblog"), true);
        alerts.set("reblog", false);
        assert!(!alerts.get("reblog"));
        assert!(alerts.get("mention"));
        alerts.set("poll", true);
        assert!(!alerts.get("poll"));
    }

    #[test]
    fn alerts_keep_the_default_for_a_kind_a_row_omits() {
        let parsed: Result<Alerts, _> = serde_json::from_str(r#"{"favourite": false}"#);
        let alerts = parsed.unwrap_or_default();
        assert!(!alerts.favourite);
        assert!(alerts.mention);
        assert!(alerts.reblog);
        assert!(alerts.follow);
    }

    #[test]
    fn alerts_serialize_as_the_four_names_the_sender_filters_on() {
        let value = serde_json::to_value(Alerts::default()).unwrap_or_default();
        assert_eq!(value["mention"], true);
        assert_eq!(value["favourite"], true);
        assert_eq!(value["reblog"], true);
        assert_eq!(value["follow"], true);
        assert_eq!(value.as_object().map(serde_json::Map::len), Some(4));
    }
}
