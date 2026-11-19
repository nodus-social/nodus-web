//! The poll controls: two to four options, a duration, and two voting rules.
//!
//! The draft lives in the composer, so every field arrives as the signal that holds it and
//! every edit writes back through the same signal. The one thing the view cannot do is put
//! the caret in a row it just added — that needs the document — so it reports the new index.

use leptos::prelude::*;

/// The most choices a poll may carry; the server refuses a fifth.
pub const MAX_OPTIONS: usize = 4;
/// The fewest choices a poll may carry; the server refuses a one-answer poll.
pub const MIN_OPTIONS: usize = 2;
/// The longest choice the server stores, in characters.
pub const MAX_OPTION_CHARS: usize = 50;
/// A day, the answer most polls mean when the writer does not choose.
pub const DEFAULT_DURATION: u32 = 86_400;

/// How long a poll may stay open, in seconds, with the label the select shows.
///
/// Every entry sits inside the server's window — five minutes to seven days — because a
/// picker that offers a duration the submit will refuse is not a picker.
pub const DURATIONS: [(u32, &str); 7] = [
    (300, "5 minutes"),
    (1_800, "30 minutes"),
    (3_600, "1 hour"),
    (21_600, "6 hours"),
    (86_400, "1 day"),
    (259_200, "3 days"),
    (604_800, "1 week"),
];

/// Whether these choices are a poll the server would accept.
///
/// Two to four non-empty choices, each at most fifty characters: the same rule the
/// instance applies in `validated_poll`, so the composer can refuse before the request.
#[must_use]
pub fn ready(options: &[String]) -> bool {
    (MIN_OPTIONS..=MAX_OPTIONS).contains(&options.len())
        && options.iter().all(|option| {
            let option = option.trim();
            !option.is_empty() && option.chars().count() <= MAX_OPTION_CHARS
        })
}

/// The poll controls, shown only while the poll is part of the post.
#[component]
pub fn PollEditor(
    open: RwSignal<bool>,
    options: RwSignal<Vec<String>>,
    expires_in: RwSignal<u32>,
    multiple: RwSignal<bool>,
    hide_totals: RwSignal<bool>,
    /// Reports the index of the option just appended, so the caller can focus it.
    #[prop(optional_no_strip)]
    on_add: Option<Callback<usize>>,
) -> impl IntoView {
    view! {
        <Show when=move || open.get()>
            <div class="flex flex-col gap-2 rounded-box border border-base-300 p-2">
                <div class="flex items-center justify-between">
                    <span class="text-xs font-semibold uppercase tracking-wide opacity-50">
                        "Poll"
                    </span>
                    <button
                        type="button"
                        class="btn btn-ghost btn-xs"
                        title="Remove the poll"
                        on:click=move |_| {
                            options.set(vec![String::new(), String::new()]);
                            open.set(false);
                        }
                    >
                        "Remove"
                    </button>
                </div>

                <p class="text-xs opacity-60">
                    "Two to four choices, 50 characters each."
                </p>
                <Options options=options/>
                <Show when=move || {
                    MIN_OPTIONS
                        > options
                            .get()
                            .iter()
                            .filter(|option| !option.trim().is_empty())
                            .count()
                }>
                    <span class="text-warning text-xs">
                        "A poll needs at least two filled choices."
                    </span>
                </Show>
                <Show when=move || MAX_OPTIONS == options.get().len()>
                    <span class="text-xs opacity-60">"A poll holds at most four choices."</span>
                </Show>
                <PollSettings
                    options=options
                    expires_in=expires_in
                    multiple=multiple
                    hide_totals=hide_totals
                    on_add=on_add
                />
            </div>
        </Show>
    }
}

/// The options, one row each.
#[component]
fn Options(options: RwSignal<Vec<String>>) -> impl IntoView {
    let update = move |index: usize, value: String| {
        options.update(|list| {
            if let Some(slot) = list.get_mut(index) {
                *slot = value;
            }
        });
    };
    let indices = Signal::derive(move || (0..options.get().len()).collect::<Vec<_>>());

    view! {
        <For each=move || indices.get() key=|index| *index let:index>
            <div class="flex flex-col gap-1">
                <div class="flex items-center gap-1">
                    <input
                        class="input input-bordered input-sm w-full"
                        id=move || format!("poll-option-{index}")
                        placeholder=move || format!("Option {}", index + 1)
                        aria-label=move || format!("Poll option {}", index + 1)
                        prop:value=move || options.get().get(index).cloned().unwrap_or_default()
                        on:input=move |event| update(index, event_target_value(&event))
                    />
                    <button
                        type="button"
                        class="btn btn-ghost btn-xs"
                        title=move || {
                            if MIN_OPTIONS == options.get().len() {
                                "A poll needs at least two choices"
                            } else {
                                "Remove this choice"
                            }
                        }
                        disabled=move || MIN_OPTIONS == options.get().len()
                        on:click=move |_| {
                            options.update(|list| {
                                if list.len() > MIN_OPTIONS {
                                    list.remove(index);
                                }
                            });
                        }
                    >
                        "×"
                    </button>
                </div>
                <Show when=move || {
                    options
                        .get()
                        .get(index)
                        .is_some_and(|option| option.trim().is_empty())
                }>
                    <span class="text-warning text-xs">"A choice cannot be empty."</span>
                </Show>
                <Show when=move || {
                    options
                        .get()
                        .get(index)
                        .is_some_and(|option| option.chars().count() > MAX_OPTION_CHARS)
                }>
                    <span class="text-error text-xs">
                        {format!("At most {MAX_OPTION_CHARS} characters per choice.")}
                    </span>
                </Show>
            </div>
        </For>
    }
}

/// How long the poll runs, and who may pick how many answers.
#[component]
fn PollSettings(
    options: RwSignal<Vec<String>>,
    expires_in: RwSignal<u32>,
    multiple: RwSignal<bool>,
    hide_totals: RwSignal<bool>,
    #[prop(optional_no_strip)] on_add: Option<Callback<usize>>,
) -> impl IntoView {
    view! {
        <div class="flex flex-wrap items-center gap-2">
            <button
                type="button"
                class="btn btn-outline btn-xs gap-1 border-dashed"
                title=move || {
                    if MAX_OPTIONS == options.get().len() {
                        "A poll holds at most four choices"
                    } else {
                        "Add another choice"
                    }
                }
                disabled=move || MAX_OPTIONS == options.get().len()
                on:click=move |_| {
                    let added = options.with_untracked(Vec::len);
                    options.update(|list| list.push(String::new()));
                    if let Some(on_add) = on_add {
                        on_add.run(added);
                    }
                }
            >
                "+"
                "Add option"
            </button>

            <select
                class="select select-bordered select-sm w-fit"
                aria-label="How long the poll stays open"
                prop:value=move || {
                    let held = expires_in.get();
                    DURATIONS
                        .iter()
                        .position(|(seconds, _)| *seconds == held)
                        .unwrap_or(0)
                        .to_string()
                }
                on:change=move |event| {
                    let index = event_target_value(&event).parse::<usize>().unwrap_or(0);
                    expires_in.set(DURATIONS.get(index).map_or(86_400, |(seconds, _)| *seconds));
                }
            >
                {DURATIONS
                    .iter()
                    .enumerate()
                    .map(|(index, (_, label))| {
                        view! { <option value=index.to_string()>{*label}</option> }
                    })
                    .collect::<Vec<_>>()}
            </select>

            <label class="label cursor-pointer gap-1 p-0 text-xs">
                <input
                    type="checkbox"
                    class="checkbox checkbox-xs"
                    prop:checked=move || multiple.get()
                    on:change=move |event| multiple.set(event_target_checked(&event))
                />
                "Several answers"
            </label>

            <label class="label cursor-pointer gap-1 p-0 text-xs">
                <input
                    type="checkbox"
                    class="checkbox checkbox-xs"
                    prop:checked=move || hide_totals.get()
                    on:change=move |event| hide_totals.set(event_target_checked(&event))
                />
                "Hide counts until it closes"
            </label>
        </div>
    }
}
