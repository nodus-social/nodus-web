//! The notification policy editor: one row per kind the API speaks about.
//!
//! The eight kinds are the wire vocabulary of `GET`/`PUT /api/v2/notifications/policy`,
//! not the reader's vocabulary: the labels here are what a row says, and [`PolicyKind::wire`]
//! is what the request says. The page owns no state and performs no request; the caller
//! holds the rows, the save and the refusal.

use leptos::prelude::*;

use crate::components::panel::Panel;

/// What this instance should do with one kind of notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PolicyKind {
    Mention,
    Favourite,
    Reblog,
    Follow,
    FollowRequest,
    Report,
    PendingRequest,
    Status,
}

impl PolicyKind {
    /// Every kind the policy endpoint answers, in the order the backend lists them.
    pub const ALL: [Self; 8] = [
        Self::Mention,
        Self::Favourite,
        Self::Reblog,
        Self::Follow,
        Self::FollowRequest,
        Self::Report,
        Self::PendingRequest,
        Self::Status,
    ];

    /// The name this kind has on the wire.
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            Self::Mention => "mention",
            Self::Favourite => "favourite",
            Self::Reblog => "reblog",
            Self::Follow => "follow",
            Self::FollowRequest => "follow_request",
            Self::Report => "report",
            Self::PendingRequest => "pending_request",
            Self::Status => "status",
        }
    }

    /// The row's label, in the reader's words rather than the endpoint's.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Mention => "Mentions",
            Self::Favourite => "Favourites",
            Self::Reblog => "Boosts",
            Self::Follow => "New followers",
            Self::FollowRequest => "Follow requests",
            Self::Report => "Reports",
            Self::PendingRequest => "Pending follow requests",
            Self::Status => "New posts",
        }
    }

    /// The group the row is filed under.
    #[must_use]
    pub const fn group(self) -> PolicyGroup {
        match self {
            Self::Mention => PolicyGroup::Mentions,
            Self::Favourite | Self::Reblog => PolicyGroup::Boosts,
            Self::Follow | Self::FollowRequest | Self::PendingRequest => PolicyGroup::Follows,
            Self::Report => PolicyGroup::Reports,
            Self::Status => PolicyGroup::Posts,
        }
    }

    /// Whether this instance has a creation site for this kind today.
    ///
    /// The API accepts all eight for Mastodon compatibility, but four of them are never
    /// written: a locked follow becomes `requested` without a notification, a report is an
    /// admin record, and nothing stores a pending request or a status notification. The
    /// rows stay editable, and the group says plainly that no such notification arrives.
    #[must_use]
    pub const fn inert(self) -> bool {
        matches!(
            self,
            Self::FollowRequest | Self::Report | Self::PendingRequest | Self::Status
        )
    }

    #[must_use]
    pub fn from_wire(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.wire() == name)
    }
}

/// What the instance does with a notification of one kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PolicyValue {
    /// Deliver it to the notifications list.
    Accept,
    /// Keep it, but behind the filtered view instead of the ordinary list.
    Filter,
    /// Do not create it at all.
    Drop,
}

impl PolicyValue {
    pub const ALL: [Self; 3] = [Self::Accept, Self::Filter, Self::Drop];

    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Filter => "filter",
            Self::Drop => "drop",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Accept => "Accept",
            Self::Filter => "Filter",
            Self::Drop => "Drop",
        }
    }

    #[must_use]
    pub fn from_wire(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|value| value.wire() == word)
    }
}

/// The headings the eight kinds are filed under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyGroup {
    Mentions,
    Boosts,
    Follows,
    Reports,
    Posts,
}

impl PolicyGroup {
    pub const ALL: [Self; 5] = [
        Self::Mentions,
        Self::Boosts,
        Self::Follows,
        Self::Reports,
        Self::Posts,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Mentions => "Mentions",
            Self::Boosts => "Boosts",
            Self::Follows => "Follows",
            Self::Reports => "Reports",
            Self::Posts => "Posts",
        }
    }

    /// The sentence under a group that holds a kind this instance never sends.
    ///
    /// Deliberately a sentence rather than a disabled control: the setting is accepted and
    /// remembered today, so it is not an error and not a lie to offer it.
    #[must_use]
    pub const fn inert_sentence(self) -> Option<&'static str> {
        match self {
            Self::Mentions | Self::Boosts => None,
            Self::Follows => Some(
                "This instance does not send follow request or pending follow request notifications today. The settings are kept, and become real if this instance starts sending them.",
            ),
            Self::Reports => Some(
                "This instance does not send report notifications today. The setting is kept, and becomes real if this instance starts sending them.",
            ),
            Self::Posts => Some(
                "This instance does not send new post notifications today. The setting is kept, and becomes real if this instance starts sending them.",
            ),
        }
    }
}

/// One editable row: the kind, its current value, and the state of the save in flight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyRow {
    pub kind: PolicyKind,
    pub value: PolicyValue,
    /// A request is in flight for this row.
    pub saving: bool,
    /// The instance refused the last change; the sentence is its own.
    pub error: Option<String>,
}

impl PolicyRow {
    /// Every row at its default, for a caller that has not read the policy yet.
    #[must_use]
    pub fn defaults() -> Vec<Self> {
        PolicyKind::ALL
            .into_iter()
            .map(|kind| Self {
                kind,
                value: PolicyValue::Accept,
                saving: false,
                error: None,
            })
            .collect()
    }

    /// The one-key body a change sends to `PUT /api/v2/notifications/policy`.
    ///
    /// Kept here beside the wire names so the page and the request cannot drift apart.
    #[must_use]
    pub fn change_body(kind: PolicyKind, value: PolicyValue) -> String {
        format!("{{\"{}\":\"{}\"}}", kind.wire(), value.wire())
    }
}

/// The editable policy, painted from rows the caller owns.
#[component]
pub fn NotificationPolicyCard(
    #[prop(into)] rows: Signal<Vec<PolicyRow>>,
    #[prop(into)] saved: Signal<bool>,
    #[prop(optional_no_strip)] on_change: Option<Callback<(PolicyKind, PolicyValue)>>,
) -> impl IntoView {
    let groups = PolicyGroup::ALL
        .into_iter()
        .map(|group| {
            let group_rows = Signal::derive(move || {
                rows.get()
                    .into_iter()
                    .filter(|row| row.kind.group() == group)
                    .collect::<Vec<_>>()
            });
            view! {
                <section class="flex flex-col gap-2" data-policy-group=group.label()>
                    <h3 class="text-sm font-semibold">{group.label()}</h3>
                    {move || {
                        group_rows
                            .get()
                            .into_iter()
                            .map(|row| row_view(&row, on_change).into_any())
                            .collect_view()
                    }}
                    {group
                        .inert_sentence()
                        .map(|sentence| {
                            view! {
                                <p class="text-xs opacity-70" data-inert-policy="">
                                    {sentence}
                                </p>
                            }
                        })}
                </section>
            }
        })
        .collect_view();
    view! {
        <Panel
            title="Notification policy"
            description="Filter keeps a notification behind the filtered view instead of discarding it."
        >
            {groups}
            <Show when=move || saved.get()>
                <p class="text-sm text-success" role="status">
                    "Saved."
                </p>
            </Show>
        </Panel>
    }
}

fn row_view(
    row: &PolicyRow,
    on_change: Option<Callback<(PolicyKind, PolicyValue)>>,
) -> impl IntoView {
    let kind = row.kind;
    let current = row.value;
    let saving = row.saving;
    let name = format!("{} notification setting", kind.label());
    let error = row.error.clone();
    view! {
        <div
            class="flex flex-col gap-1 rounded-box bg-base-200 px-3 py-2"
            data-policy-kind=kind.wire()
        >
            <div class="flex flex-wrap items-center justify-between gap-2">
                <span class="text-sm">{kind.label()}</span>
                <select
                    class="select select-sm"
                    aria-label=name
                    prop:value=current.wire()
                    disabled=on_change.is_none() || saving
                    on:change=move |event| {
                        let picked = event_target_value(&event);
                        if let (Some(on_change), Some(value)) =
                            (on_change, PolicyValue::from_wire(&picked))
                        {
                            on_change.run((kind, value));
                        }
                    }
                >
                    {PolicyValue::ALL
                        .into_iter()
                        .map(|value| {
                            view! {
                                <option value=value.wire() selected=current == value>
                                    {value.label()}
                                </option>
                            }
                        })
                        .collect_view()}
                </select>
            </div>
            <Show when=move || saving>
                <p class="text-xs opacity-70" role="status">
                    "Saving…"
                </p>
            </Show>
            {error
                .map(|message| {
                    view! {
                        <p class="text-xs text-error" role="alert">
                            {message}
                        </p>
                    }
                })}
        </div>
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    fn painted(rows: Vec<PolicyRow>) -> String {
        view! {
            <NotificationPolicyCard
                rows=Signal::stored(rows)
                saved=Signal::stored(false)
                on_change=Some(Callback::new(|_| {}))
            />
        }
        .to_html()
    }

    /// The editor speaks the endpoint's eight kinds and shows the value it read for each.
    #[test]
    fn every_kind_has_a_row_under_its_group() {
        let html = painted(PolicyRow::defaults());
        for kind in PolicyKind::ALL {
            assert!(html.contains(kind.label()), "missing {kind:?}: {html}");
            assert!(
                html.contains(&format!("data-policy-kind=\"{}\"", kind.wire())),
                "missing row for {kind:?}: {html}"
            );
            assert!(
                html.contains(&format!(
                    "aria-label=\"{} notification setting\"",
                    kind.label()
                )),
                "row {kind:?} has no accessible name: {html}"
            );
        }
        for group in PolicyGroup::ALL {
            assert!(
                html.contains(&format!("data-policy-group=\"{}\"", group.label())),
                "missing group {group:?}: {html}"
            );
        }
    }

    /// The four kinds with no creation site say so in their group, and stay editable.
    #[test]
    fn the_four_inert_kinds_say_this_instance_does_not_send_them() {
        let html = painted(PolicyRow::defaults());
        for group in [
            PolicyGroup::Follows,
            PolicyGroup::Reports,
            PolicyGroup::Posts,
        ] {
            let sentence = group.inert_sentence().expect("group is inert");
            assert!(html.contains(sentence), "missing {group:?}: {html}");
        }
        // The control still works: the setting is accepted and becomes real if a creation
        // site is ever added, so an inert row is not a disabled one.
        assert!(
            !html.contains("disabled"),
            "an inert row must stay editable: {html}"
        );
    }

    /// The current value is the selected option, so the control shows what the GET said.
    #[test]
    fn a_row_shows_the_value_it_read() {
        let rows = PolicyRow::defaults()
            .into_iter()
            .map(|row| {
                if row.kind == PolicyKind::Favourite {
                    PolicyRow {
                        value: PolicyValue::Drop,
                        ..row
                    }
                } else {
                    row
                }
            })
            .collect();
        let html = painted(rows);
        assert!(
            html.contains(r#"<option value="drop" selected="">Drop</option>"#)
                || html.contains(r#"<option value="drop" selected>Drop</option>"#),
            "{html}"
        );
    }

    /// A refusal belongs to the row that asked, not to a toast at the edge of the screen.
    #[test]
    fn a_refusal_is_shown_in_the_row_it_is_about() {
        let rows = PolicyRow::defaults()
            .into_iter()
            .map(|row| {
                if row.kind == PolicyKind::Favourite {
                    PolicyRow {
                        error: Some(
                            "unknown policy for favourite: sometimes; allowed values are accept, filter and drop"
                                .to_owned(),
                        ),
                        ..row
                    }
                } else {
                    row
                }
            })
            .collect();
        let html = painted(rows);
        assert!(html.contains("role=\"alert\""), "{html}");
        assert!(
            html.contains("unknown policy for favourite: sometimes; allowed values are accept, filter and drop"),
            "{html}"
        );
    }

    /// One change is one key: the API takes a partial body, and the row sends only itself.
    #[test]
    fn one_change_posts_only_the_changed_kind() {
        assert_eq!(
            PolicyRow::change_body(PolicyKind::Favourite, PolicyValue::Drop),
            r#"{"favourite":"drop"}"#
        );
    }

    /// The wire names are the backend's list, so a rename on either side fails here.
    #[test]
    fn the_wire_vocabulary_round_trips() {
        let names = PolicyKind::ALL
            .into_iter()
            .map(PolicyKind::wire)
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            [
                "mention",
                "favourite",
                "reblog",
                "follow",
                "follow_request",
                "report",
                "pending_request",
                "status",
            ]
        );
        for kind in PolicyKind::ALL {
            assert_eq!(PolicyKind::from_wire(kind.wire()), Some(kind));
        }
        for value in PolicyValue::ALL {
            assert_eq!(PolicyValue::from_wire(value.wire()), Some(value));
        }
    }
}
