//! `/notifications`: what this instance has told this reader, newest first.
//!
//! The page is the client's own because half of what it draws is not Mastodon: a moderation
//! notice carries the action, the reason, the rule, an expiry and, while it can still be
//! appealed, a button. Rows also arrive grouped — twenty favourites on one post are one row
//! with "and 19 others" — and the instance says which rows by `group_key`.

use leptos::prelude::*;
use nodus_pages::pages::moderation::appeals::{force_sentence, lifted_sentence};
use nodus_pages::{PageHeader, Screen};
use nodus_ui::{display_name, relative_age};

use crate::platform::clock;
use crate::state::bridge::notifications::{self, Appeal, Moderation, Notice};

/// One row on the page: the newest notice in its group and how many it stands for.
#[derive(Clone)]
struct Group {
    key: String,
    notice: Notice,
    others: i64,
}

#[component]
pub fn NotificationsView() -> impl IntoView {
    Effect::new(move |_| notifications::ensure_loaded());

    let items = notifications::items();
    let loading = notifications::loading();
    let loaded = notifications::loaded();
    let error = notifications::error();
    let cursor = notifications::cursor();
    let available = notifications::available();
    let unread = notifications::unread();
    let seen = notifications::seen();
    let revision = notifications::revision();

    let groups = Signal::derive(move || {
        let _ = revision.get();
        fold_groups(items.get())
    });

    let empty = move || loaded.get() && items.get().is_empty() && available.get() != Some(false);

    view! {
        <Screen>
            <div class="flex items-start justify-between gap-3">
                <PageHeader
                    title="Notifications"
                    description="Mentions, boosts, favourites, follows and anything a moderator has said to this account."
                />
                <Show when=move || { unread.get() > 0 }>
                    <button
                        class="btn btn-ghost btn-sm w-fit"
                        data-mark-all-read
                        on:click=move |_| notifications::mark_all_read()
                    >
                        "Mark all read"
                    </button>
                </Show>
            </div>

            <Show when=move || available.get() == Some(false)>
                <p class="text-sm opacity-60" data-notice-absent>
                    "This instance does not serve notifications."
                </p>
            </Show>

            <Show when=empty>
                <p class="text-sm opacity-60" data-notice-empty>
                    "This instance has nothing for you yet."
                </p>
            </Show>

            <Show when=move || error.get().is_some()>
                <p class="text-error text-xs" data-notice-error>
                    {move || error.get().unwrap_or_default()}
                </p>
            </Show>

            <ul class="flex flex-col gap-3">
                <For each=move || groups.get() key=|group| group.key.clone() let:group>
                    <NoticeRow group=group seen=seen.clone() />
                </For>
            </ul>

            <Show when=move || cursor.get().is_some()>
                <button
                    class="btn btn-ghost btn-sm w-fit"
                    data-notice-more
                    disabled=move || loading.get()
                    on:click=move |_| notifications::load_more()
                >
                    "Load more"
                </button>
            </Show>
        </Screen>
    }
}

/// Folds the rows the instance grouped, keeping the instance's own order.
///
/// A grouped row already carries `group_count`; an instance that sends one row per event
/// instead (no aggregation) is folded here as well, so the page reads the same either way.
fn fold_groups(rows: Vec<Notice>) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::with_capacity(rows.len());
    for row in rows {
        let key = if row.group_key.is_empty() {
            format!("{}:{}", row.kind, row.id)
        } else {
            row.group_key.clone()
        };
        if let Some(existing) = groups.iter_mut().find(|group| group.key == key) {
            existing.others += 1;
            continue;
        }
        let others = (row.group_count - 1).max(0);
        groups.push(Group {
            key,
            notice: row,
            others,
        });
    }
    groups
}

#[component]
fn NoticeRow(#[prop(into)] group: Group, seen: Signal<std::collections::HashSet<String>>) -> impl IntoView {
    let notice = group.notice.clone();
    let id = notice.id.clone();
    let unread = {
        let id = id.clone();
        Signal::derive(move || !seen.get().contains(&id))
    };
    let account = notice.account.clone();
    let who = display_name(&account.display_name, &account.acct);
    let profile = format!("/accounts/{}", account.id);
    let avatar = account.avatar.clone();
    let age = relative_age(&notice.created_at, clock::now_ms() as f64);
    let created_at = notice.created_at.clone();
    let verb = phrase(&notice, group.others);
    let kind = notice.kind.clone();
    let status = notice.status.clone();
    let note = notice.note.clone();
    let moderation = notice.moderation.clone();
    let avatar_id = notice.id.clone();
    let post_id = notice.id.clone();
    let profile_href = profile.clone();

    view! {
        <li
            class="card border border-base-300 bg-base-100 shadow-sm"
            data-notice-id=notice.id.clone()
            data-notice-kind=kind
        >
            <div class="card-body gap-2 p-4">
                <div class="flex items-center gap-2">
                    <a class="avatar" href=profile_href on:click=move |_| notifications::mark_seen(&avatar_id)>
                        <div class="w-8 rounded-full bg-base-300">
                            <img
                                width="40"
                                height="40"
                                decoding="async"
                                src=avatar
                                alt=""
                                referrerpolicy="no-referrer"
                            />
                        </div>
                    </a>
                    <p class="min-w-0 flex-1 truncate text-sm">
                        <a class="font-semibold hover:underline" href=profile.clone()>
                            {who}
                        </a>
                        <span class="opacity-70">{format!(" {verb}")}</span>
                    </p>
                    <Show when=move || unread.get()>
                        <span class="badge badge-primary badge-xs" data-unread-dot></span>
                    </Show>
                    <time class="text-xs opacity-50" datetime=created_at>{age}</time>
                </div>

                {note.map(|note| view! { <p class="text-sm opacity-80" data-notice-note>{note}</p> })}

                {moderation.map(|moderation| {
                    view! { <ModerationBox notice_id=notice.id.clone() moderation=moderation /> }
                })}

                {status.map(|status| {
                    let href = format!("/status/{}", status.id);
                    view! {
                        <a
                            class="link link-hover text-xs opacity-70"
                            href=href
                            on:click=move |_| notifications::mark_seen(&post_id)
                        >
                            "View post"
                        </a>
                    }
                })}
            </div>
        </li>
    }
}

/// What happened, in words, with the group folded in front of it.
fn phrase(notice: &Notice, others: i64) -> String {
    let verb = match notice.kind.as_str() {
        "follow" => "followed you",
        "follow_request" => "asked to follow you",
        "reblog" => "boosted your post",
        "favourite" => "favourited your post",
        "mention" => "mentioned you",
        "poll" => "A poll you voted in ended",
        "status" => "posted",
        "moderation" => "A moderator wrote to this account",
        "appeal" => "answered an appeal",
        _ => "did something this client has no word for",
    };
    if others > 0 {
        format!("{verb} — and {others} others")
    } else {
        verb.to_owned()
    }
}

/// The moderation notice: what was done, why, by which rule, until when, and the appeal.
#[component]
fn ModerationBox(notice_id: String, moderation: Moderation) -> impl IntoView {
    let action = moderation.action.clone();
    let reason = moderation.reason.clone();
    let rule = moderation.rule.clone();
    let expires = moderation.expires_at.as_deref().map(date_only);
    let decided_by = moderation.decided_by.clone();
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    // `For` keys the row, so this component is built once from the snapshot the group
    // carried. The appeal, though, is written by this browser after the POST succeeds: a
    // copy would leave "Appeal" on screen until a reload. Read the notice's moderation
    // payload from the store instead, so the waiting line arrives with the write.
    let row_id = notice_id.clone();
    let live = Signal::derive(move || {
        notifications::items()
            .get()
            .iter()
            .find(|row| row.id == row_id)
            .and_then(|row| row.moderation.clone())
    });
    let appeal = Signal::derive(move || live.get().and_then(|moderation| moderation.appeal));
    let can_appeal = Signal::derive(move || live.get().is_some_and(|moderation| moderation.can_appeal));
    let decided = Signal::derive(move || appeal.get().filter(Appeal::decided));
    let pending = Signal::derive(move || {
        appeal
            .get()
            .is_some_and(|appeal| !Appeal::decided(&appeal))
    });

    let send = Callback::new({
        let id = notice_id.clone();
        move |()| {
            if busy.get_untracked() {
                return;
            }
            busy.set(true);
            error.set(None);
            let done = Callback::new(move |result: Result<(), String>| {
                busy.set(false);
                if let Err(message) = result {
                    error.set(Some(message));
                }
            });
            notifications::submit_appeal(
                id.clone(),
                "I would like this restriction reviewed.".to_owned(),
                done,
            );
        }
    });

    view! {
        <div class="rounded-box bg-base-200/60 p-3 text-sm" data-moderation-action=action.clone()>
            <p class="font-medium">{action_sentence(&action)}</p>
            {reason.map(|reason| view! { <p class="opacity-80" data-moderation-reason>{format!("Reason: {reason}")}</p> })}
            {rule.map(|rule| view! { <p class="opacity-80" data-moderation-rule>{format!("Rule: {rule}")}</p> })}
            {expires.map(|date| view! { <p class="opacity-80" data-moderation-expires>{format!("Expires on {date}.")}</p> })}

            {move || decided.get().map(|appeal| {
                let decision = if appeal.status == "accepted" {
                    lifted_sentence(&appeal.action).to_owned()
                } else {
                    let force = force_sentence(&appeal.action);
                    format!("Rejected: {}", lowercase_first(force))
                };
                let who = appeal.decided_by.clone().or(decided_by.clone());
                let when = appeal.decided_at.as_deref().map(date_only);
                let line = decided_line(who.as_deref(), when.as_deref());
                let note = appeal.decision_note.clone();
                view! {
                    <div class="mt-2 flex flex-col gap-1" data-appeal-decided>
                        <p class="font-medium">{decision}</p>
                        <p class="opacity-80" data-appeal-decider>{line}</p>
                        {note.map(|note| view! { <p class="opacity-70">{note}</p> })}
                    </div>
                }
            })}

            {move || pending.get().then(|| {
                view! {
                    <p class="mt-2 opacity-80" data-appeal-waiting>
                        "Your appeal is waiting for a decision"
                    </p>
                }
            })}

            {move || {
                let send = send.clone();
                can_appeal.get().then(|| {
                    view! {
                        <button
                            class="btn btn-sm btn-primary mt-2 w-fit"
                            data-appeal
                            disabled=move || busy.get()
                            on:click=move |_| send.run(())
                        >
                            "Appeal"
                        </button>
                    }
                })
            }}

            <Show when=move || error.get().is_some()>
                <p class="text-error mt-2 text-xs">{move || error.get().unwrap_or_default()}</p>
            </Show>
        </div>
    }
}

/// What was done, in a sentence a person who was not at the keyboard understands.
fn action_sentence(action: &str) -> &'static str {
    match action {
        "suspend" => "The account was suspended.",
        "silence" => "The account was silenced.",
        "reject" => "The registration was rejected.",
        "warn" => "A moderator warned this account.",
        "unsilence" => "The silence was lifted.",
        "unsuspend" => "The suspension was lifted.",
        "approve" => "The registration was approved.",
        _ => "A decision was taken about this account.",
    }
}

fn decided_line(who: Option<&str>, when: Option<&str>) -> String {
    match (who, when) {
        (Some(who), Some(date)) => format!("Decided by {who} on {date}."),
        (Some(who), None) => format!("Decided by {who}."),
        (None, Some(date)) => format!("Decided on {date}."),
        (None, None) => "Decided.".to_owned(),
    }
}

/// The date part of an RFC 3339 timestamp, which is what the admin screen shows too.
fn date_only(stamp: &str) -> String {
    stamp.get(..10).unwrap_or(stamp).to_owned()
}

fn lowercase_first(sentence: &str) -> String {
    let mut characters = sentence.chars();
    match characters.next() {
        Some(first) => first.to_lowercase().chain(characters).collect(),
        None => String::new(),
    }
}
