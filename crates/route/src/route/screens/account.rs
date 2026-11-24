//! One account: who they are, whether we follow them, and what they posted.

use std::sync::Arc;

use leptos::prelude::*;
use nodus_client_core::model::{Account, ProfileField as SdkField, Relationship};
use nodus_pages::pages::settings::form::ProfileField;

use crate::chrome::{ButtonTabList, LoadMore};
use crate::components::CardRenderer;
use crate::pages::people::Kind;

use super::super::{Action, ActionResult, Runtime, TimelineKind};
use super::people::PeopleList;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Posts,
    Followers,
    Following,
}

impl Tab {
    const ALL: [Self; 3] = [Self::Posts, Self::Followers, Self::Following];

    fn label(self) -> &'static str {
        match self {
            Self::Posts => "Posts",
            Self::Followers => "Followers",
            Self::Following => "Following",
        }
    }

    fn of(label: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|which| which.label() == label)
            .unwrap_or(Self::Posts)
    }
}

/// A profile page.
#[component]
#[expect(
    clippy::too_many_lines,
    reason = "a route screen owns its layout and its reads"
)]
pub fn AccountScreen(runtime: Arc<dyn Runtime>, id: String, card: CardRenderer) -> impl IntoView {
    let runtime = StoredValue::new(runtime);
    let card = StoredValue::new(card);
    let account = RwSignal::new(runtime.get_value().account(&id));
    let relationship = RwSignal::new(runtime.get_value().relationship(&id));
    let tab = RwSignal::new(Tab::Posts);
    let (loading, set_loading) = signal(false);
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let alive = crate::Alive::new();

    let kind = StoredValue::new(TimelineKind::Account(id.clone()));
    let snapshot = Signal::derive(move || {
        runtime.get_value().revision().get();
        runtime.get_value().timeline(&kind.get_value())
    });
    let statuses = Signal::derive(move || snapshot.get().items);
    let cursor = Signal::derive(move || snapshot.get().cursor);

    let load_posts = Callback::new(move |after: Option<String>| {
        set_loading.set(true);
        let done = Callback::new(move |result: Result<(), String>| {
            alive.if_alive(|| {
                if let Err(message) = result {
                    error.set(Some(message));
                }
                set_loading.set(false);
            });
        });
        runtime
            .get_value()
            .load_timeline(kind.get_value(), after, done);
    });

    let load_id = id.clone();
    Effect::new(move |_| {
        if account.get_untracked().is_none() || relationship.get_untracked().is_none() {
            let account_done = Callback::new(move |result: Result<Account, String>| {
                alive.if_alive(|| match result {
                    Ok(found) => account.set(Some(found)),
                    Err(message) => {
                        if account.get_untracked().is_none() {
                            error.set(Some(message));
                        }
                    }
                });
            });
            let relationship_done =
                Callback::new(move |result: Result<Option<Relationship>, String>| {
                    alive.if_alive(|| match result {
                        Ok(found) => relationship.set(found),
                        // A profile whose relationship could not be read has no verbs worth
                        // drawing: three dead buttons read as "this account cannot be acted
                        // on", which is a claim, and the wrong one. The reason is shown
                        // instead, and the header keeps only what is true.
                        Err(message) => {
                            error.set(Some(message));
                        }
                    });
                });
            runtime
                .get_value()
                .load_profile(load_id.clone(), account_done, relationship_done);
        }
        let held = snapshot.get_untracked();
        if !held.held && !held.unavailable {
            load_posts.run(None);
        }
    });

    let tabs: ChildrenFn = Arc::new(move || {
        view! {
            <ButtonTabList
                labels=Tab::ALL.iter().map(|tab| tab.label().to_owned()).collect()
                active=Signal::derive(move || tab.get().label().to_owned())
                on_select=Callback::new(move |label: String| tab.set(Tab::of(&label)))
            />
        }
        .into_any()
    });

    let page_id = StoredValue::new(id);
    let after: ChildrenFn = Arc::new(move || {
        view! {
            <Show when=move || tab.get() == Tab::Posts>
                <Show when=move || cursor.get().is_some()>
                    <LoadMore
                        loading=Signal::derive(move || loading.get())
                        on_more=Callback::new(move |()| load_posts.run(cursor.get_untracked()))
                    />
                </Show>
            </Show>
            <Show when=move || tab.get() == Tab::Followers>
                <PeopleList
                    runtime=runtime.get_value()
                    id=page_id.get_value()
                    kind=Kind::Followers
                />
            </Show>
            <Show when=move || tab.get() == Tab::Following>
                <PeopleList
                    runtime=runtime.get_value()
                    id=page_id.get_value()
                    kind=Kind::Following
                />
            </Show>
            <Show when=move || error.get().is_some()>
                <crate::Note class="text-error text-xs">
                    {move || nodus_ui::capabilities::explain(error.get())}
                </crate::Note>
            </Show>
        }
        .into_any()
    });

    // The rows ride on the account document, so the reader's own profile reads them from
    // the account already in hand rather than asking the instance a second question. They
    // stay the reader's: this screen has only ever drawn the signed-in reader's rows, and
    // another account's are not it to grow. A visitor's page carries none at all.
    // A cold load can finish rendering before the stored session is restored, so ownership
    // is derived from the revision signal rather than read once at construction. A profile
    // opened by a visitor never grows the reader's controls when a later session arrives
    // for a different account.
    let own_profile = Signal::derive(move || {
        runtime.get_value().revision().get();
        runtime
            .get_value()
            .session()
            .is_some_and(|session| session.account_id == page_id.get_value())
    });
    let fields = Signal::derive(move || {
        if !own_profile.get() {
            return Vec::new();
        }
        account
            .get()
            .map(|profile| profile_rows(&profile.fields))
            .unwrap_or_default()
    });
    // The controls exist only on the reader's own profile, and only when the running paint
    // can write. The account the instance answers with replaces the one this screen holds,
    // so a new picture paints without a second read.
    let available_editor = runtime.get_value().profile_editor();

    let age_of = StoredValue::new(Callback::new(move |created_at: String| {
        crate::relative_age(&created_at, runtime.get_value().now_millis())
    }));

    let on_avatar = Callback::new(move |url: String| {
        runtime.get_value().action(
            Action::OpenMedia {
                urls: vec![url],
                index: 0,
            },
            Callback::new(|_| ()),
        );
    });

    view! {
        {move || account.get().map(|profile| {
            let id = profile.id.clone();
            let editor = own_profile
                .get()
                .then_some(available_editor)
                .flatten()
                .map(|editor| nodus_pages::profile::ProfileEditor {
                    on_saved: Callback::new(move |fresh: Account| account.set(Some(fresh))),
                    ..editor
                });
            view! {
                <crate::profile::ProfilePage
                    account=profile
                    relationship=relationship.get()
                    statuses=statuses
                    show_statuses=tab.get() == Tab::Posts
                    // Nobody follows, mutes or blocks themselves: the instance's answer
                    // about a relationship with one's own account is not an offer to act.
                    show_verbs=relationship.get().is_some() && !own_profile.get()
                    tabs=tabs.clone()
                    after=after.clone()
                    card=card.get_value()
                    age_of=age_of.get_value()
                    on_follow=Callback::new({
                        let id = id.clone();
                        move |(_, on): (String, bool)| {
                            toggle(
                                &runtime.get_value(),
                                Action::Follow { id: id.clone(), on },
                                relationship,
                                busy,
                                error,
                                alive,
                            );
                        }
                    })
                    on_mute=Callback::new({
                        let id = id.clone();
                        move |(_, on): (String, bool)| {
                            toggle(
                                &runtime.get_value(),
                                Action::Mute { id: id.clone(), on },
                                relationship,
                                busy,
                                error,
                                alive,
                            );
                        }
                    })
                    on_block=Callback::new({
                        let id = id.clone();
                        move |(_, on): (String, bool)| {
                            toggle(
                                &runtime.get_value(),
                                Action::Block { id: id.clone(), on },
                                relationship,
                                busy,
                                error,
                                alive,
                            );
                        }
                    })
                    fields=fields
                    editor=editor
                    banners=runtime.get_value().profile_banners()
                    on_avatar=on_avatar
                />
            }
        })}
    }
}

/// The rows an account carries, in the shape the profile page draws them.
fn profile_rows(fields: &[SdkField]) -> Vec<ProfileField> {
    fields
        .iter()
        .map(|field| ProfileField {
            name: field.name.clone(),
            value: field.value.clone(),
            verified_at: field.verified_at.clone(),
        })
        .collect()
}

fn toggle(
    runtime: &Arc<dyn Runtime>,
    action: Action,
    relationship: RwSignal<Option<Relationship>>,
    busy: RwSignal<bool>,
    error: RwSignal<Option<String>>,
    alive: crate::Alive,
) {
    if busy.get_untracked() || relationship.get_untracked().is_none() {
        return;
    }
    busy.set(true);
    error.set(None);
    let done = Callback::new(move |result: Result<ActionResult, String>| {
        alive.if_alive(|| {
            match result {
                Ok(ActionResult::Relationship(found)) => relationship.set(Some(found)),
                Ok(_) => {}
                Err(message) => error.set(Some(message)),
            }
            busy.set(false);
        });
    });
    runtime.action(action, done);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use leptos::prelude::{Callback, IntoAny as _, Owner, RenderHtml as _, Signal};
    use nodus_client_core::model::{Account, Relationship, Status};
    use nodus_ui::session_style::BannerRegionKind;

    use super::{AccountScreen, AccountScreenProps};
    use crate::components::CardRenderer;
    use crate::route::{Runtime, SessionView, TimelineKind, TimelineReply};
    use nodus_client_core::model::ProfileField as SdkField;
    use nodus_pages::pages::settings::form::ProfileImage;
    use nodus_pages::profile::ProfileEditor;

    fn test_editor(message: Option<Result<String, String>>) -> ProfileEditor {
        ProfileEditor {
            pick_picture: Callback::new(|(_kind, _done): (ProfileImage, Callback<Account>)| {}),
            edit_banner: Callback::new(|_region: BannerRegionKind| {}),
            busy: Signal::derive(|| None),
            message: Signal::derive(move || message.clone()),
            left_banner: Signal::derive(|| false),
            right_banner: Signal::derive(|| false),
            on_saved: Callback::new(|_account: Account| {}),
        }
    }

    struct HeldAccountRuntime;

    impl Runtime for HeldAccountRuntime {
        fn revision(&self) -> Signal<u64> {
            Signal::derive(|| 0)
        }

        fn now_millis(&self) -> f64 {
            0.0
        }

        fn timeline(&self, _kind: &TimelineKind) -> TimelineReply {
            TimelineReply {
                items: Vec::new(),
                cursor: Some("next-cursor".to_owned()),
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

        fn account(&self, id: &str) -> Option<Account> {
            (id == "42").then(|| Account {
                id: "42".to_owned(),
                acct: "alice@example.org".to_owned(),
                display_name: "Alice".to_owned(),
                note: String::new(),
                avatar: String::new(),
                avatar_meta: None,
                header: String::new(),
                header_meta: None,
                bot: false,
                locked: false,
                followers_count: 1,
                following_count: 2,
                statuses_count: 3,
                last_status_at: None,
                discoverable: None,
                indexable: None,
                noindex: None,
                fields: Vec::new(),
            })
        }

        fn profile_editor(&self) -> Option<ProfileEditor> {
            // Deliberately present even without a session: the screen, not the runtime, owns
            // the "this is my profile" gate.
            Some(test_editor(None))
        }
    }

    #[test]
    fn held_account_with_cursor_renders_idle_load_more() {
        let card: CardRenderer = Arc::new(|_status: Status| ().into_any());
        let html = Owner::new().with(|| {
            AccountScreen(AccountScreenProps {
                runtime: Arc::new(HeldAccountRuntime),
                id: "42".to_owned(),
                card,
            })
            .to_html()
        });
        let load_more_pos = html.find(">Load more</button>").expect("Load more button");
        let button_start = html[..load_more_pos].rfind("<button").expect("button tag");
        let load_more_tag = &html[button_start..load_more_pos];
        assert!(
            !load_more_tag.contains("disabled"),
            "Load more button must not be disabled when timeline is held: {load_more_tag}"
        );
    }

    /// The signed-in reader of the stub instance, holding the rows given to it.
    struct OwnProfileRuntime {
        fields: Vec<SdkField>,
        message: Option<Result<String, String>>,
        relationship: Option<Relationship>,
        /// The account the session belongs to; `42` is the profile the screen opens.
        viewer: &'static str,
    }

    impl Runtime for OwnProfileRuntime {
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

        fn session(&self) -> Option<SessionView> {
            Some(SessionView {
                host: "stub.instance".to_owned(),
                account_id: self.viewer.to_owned(),
                acct: "alice".to_owned(),
            })
        }

        fn relationship(&self, id: &str) -> Option<Relationship> {
            (id == "42").then(|| self.relationship.clone()).flatten()
        }

        fn profile_editor(&self) -> Option<ProfileEditor> {
            Some(test_editor(self.message.clone()))
        }

        fn account(&self, id: &str) -> Option<Account> {
            (id == "42").then(|| Account {
                id: "42".to_owned(),
                acct: "alice".to_owned(),
                display_name: "Alice".to_owned(),
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
                fields: self.fields.clone(),
            })
        }
    }

    fn render_own_profile(fields: Vec<SdkField>) -> String {
        render_own_profile_with(fields, None)
    }

    fn render_own_profile_with(
        fields: Vec<SdkField>,
        relationship: Option<Relationship>,
    ) -> String {
        let card: CardRenderer = Arc::new(|_status: Status| ().into_any());
        Owner::new().with(|| {
            AccountScreen(AccountScreenProps {
                runtime: Arc::new(OwnProfileRuntime {
                    fields,
                    message: None,
                    relationship,
                    viewer: "42",
                }),
                id: "42".to_owned(),
                card,
            })
            .to_html()
        })
    }

    /// A relationship the instance reports without any flag set: the state in which every
    /// verb would offer to follow, mute or block.
    fn plain_relationship() -> Relationship {
        Relationship {
            id: nodus_client_core::model::LocalId("42".to_owned()),
            following: false,
            requested: false,
            followed_by: false,
            blocking: false,
            blocked_by: false,
            muting: false,
            muting_notifications: false,
            domain_blocking: false,
            endorsed: false,
        }
    }

    #[test]
    fn own_profile_shows_the_rows_with_the_instances_verification() {
        let html = render_own_profile(vec![
            SdkField::new("Blog", "https://example.test").verified_at("2026-01-01T00:00:00Z"),
            SdkField::new("Pronouns", "she/her"),
        ]);

        assert!(html.contains("https://example.test"), "{html}");
        assert!(html.contains("she/her"), "{html}");
        assert!(
            html.contains(r#"aria-label="Verified link""#),
            "a server-authored verified_at must draw the tick: {html}"
        );
        assert_eq!(
            html.matches(r#"aria-label="Verified link""#).count(),
            1,
            "only the verified row is ticked"
        );
    }

    #[test]
    fn own_profile_offers_the_picture_the_header_and_both_side_banners() {
        let html = render_own_profile(Vec::new());
        for label in [
            "Change profile picture",
            "Change header image",
            "Add a left side banner",
            "Add a right side banner",
        ] {
            assert!(html.contains(label), "{label} missing: {html}");
        }
    }

    #[test]
    fn another_accounts_profile_offers_no_media_controls_even_if_the_runtime_can_write() {
        let card: CardRenderer = Arc::new(|_status: Status| ().into_any());
        let html = Owner::new().with(|| {
            AccountScreen(AccountScreenProps {
                runtime: Arc::new(HeldAccountRuntime),
                id: "42".to_owned(),
                card,
            })
            .to_html()
        });
        assert!(!html.contains("Change profile picture"), "{html}");
        assert!(!html.contains("Change header image"), "{html}");
        assert!(!html.contains("side banner"), "{html}");
    }

    #[test]
    fn a_refused_picture_save_paints_its_message_in_place() {
        let message = Some(Err("The instance refused that picture.".to_owned()));
        let card: CardRenderer = Arc::new(|_status: Status| ().into_any());
        let html = Owner::new().with(|| {
            AccountScreen(AccountScreenProps {
                runtime: Arc::new(OwnProfileRuntime {
                    fields: Vec::new(),
                    message,
                    relationship: None,
                    viewer: "42",
                }),
                id: "42".to_owned(),
                card,
            })
            .to_html()
        });
        assert!(html.contains(r#"role="alert""#), "{html}");
        assert!(
            html.contains("The instance refused that picture."),
            "{html}"
        );
    }

    #[test]
    fn the_owner_sees_no_relationship_verbs_on_their_own_profile() {
        // The instance answers a relationship for one's own account like any other, and
        // every flag is false, so an ungated header would offer Follow, Mute and Block.
        let html = render_own_profile_with(Vec::new(), Some(plain_relationship()));
        for label in ["Follow", "Mute", "Block"] {
            assert!(
                !html.contains(&format!(">{label}</button>")),
                "the owner must not be offered {label} on themselves: {html}"
            );
        }
    }

    #[test]
    fn another_accounts_profile_still_offers_its_verbs() {
        // The same relationship, viewed by someone who is not that account: the gate hides
        // the owner's own verbs and nothing else.
        let card: CardRenderer = Arc::new(|_status: Status| ().into_any());
        let html = Owner::new().with(|| {
            AccountScreen(AccountScreenProps {
                runtime: Arc::new(OwnProfileRuntime {
                    fields: Vec::new(),
                    message: None,
                    relationship: Some(plain_relationship()),
                    viewer: "7",
                }),
                id: "42".to_owned(),
                card,
            })
            .to_html()
        });
        for label in ["Follow", "Mute", "Block"] {
            assert!(
                html.contains(&format!(">{label}</button>")),
                "a signed-in reader must be offered {label}: {html}"
            );
        }
    }

    #[test]
    fn clearing_every_row_leaves_the_profile_row_free() {
        let html = render_own_profile(Vec::new());

        assert!(!html.contains("Verified link"), "{html}");
        assert!(!html.contains("https://example.test"), "{html}");
    }
}
