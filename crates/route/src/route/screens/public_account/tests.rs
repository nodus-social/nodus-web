//! A visitor's profile draws the account, never a row table: rows belong to the signed-in
//! reader, and a page with no session has no reader to attribute them to.

use std::sync::Arc;

use leptos::prelude::*;
use nodus_client_core::model::{Account, ProfileField as SdkField, Status};

use super::{PublicAccountScreen, PublicAccountScreenProps};
use crate::components::CardRenderer;
use crate::route::{Runtime, TimelineKind, TimelineReply};

/// A host with no session, holding an account that answers with rows anyway — the instance
/// sends whatever it sends; this screen decides not to draw them.
struct VisitorRuntime;

impl Runtime for VisitorRuntime {
    fn revision(&self) -> Signal<u64> {
        Signal::derive(|| 0)
    }

    fn now_millis(&self) -> f64 {
        0.0
    }

    fn timeline(&self, _kind: &TimelineKind) -> TimelineReply {
        TimelineReply {
            items: Vec::new(),
            cursor: None,
            held: true,
            unavailable: false,
        }
    }

    fn load_timeline(
        &self,
        _kind: TimelineKind,
        _cursor: Option<String>,
        _done: Callback<Result<(), String>>,
    ) {
    }

    fn public_account(&self, host: &str, id: &str) -> Option<Account> {
        (host == "mastodon.test" && id == "7").then(|| Account {
            id: "7".to_owned(),
            acct: "bob".to_owned(),
            display_name: "Bob".to_owned(),
            note: String::new(),
            avatar: String::new(),
            avatar_meta: None,
            header: String::new(),
            header_meta: None,
            bot: false,
            locked: false,
            followers_count: 0,
            following_count: 0,
            statuses_count: 0,
            last_status_at: None,
            discoverable: None,
            indexable: None,
            noindex: None,
            fields: vec![
                SdkField::new("Blog", "https://example.test").verified_at("2026-01-01T00:00:00Z"),
                SdkField::new("Pronouns", "they/them"),
            ],
        })
    }
}

fn render_visitor_profile() -> String {
    let card: CardRenderer = Arc::new(|_status: Status| ().into_any());
    Owner::new().with(|| {
        PublicAccountScreen(PublicAccountScreenProps {
            runtime: Arc::new(VisitorRuntime),
            host: "mastodon.test".to_owned(),
            id: "7".to_owned(),
            card,
        })
        .to_html()
    })
}

#[test]
fn a_visitors_profile_renders_no_rows() {
    let html = render_visitor_profile();

    assert!(html.contains("Bob"), "{html}");
    assert!(!html.contains("https://example.test"), "{html}");
    assert!(!html.contains("they/them"), "{html}");
    assert!(!html.contains("Verified link"), "{html}");
}
