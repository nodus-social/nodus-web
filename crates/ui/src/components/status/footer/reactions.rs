//! The reaction row under a status, for a server that keeps a reaction tally.
//!
//! A reaction is not a verb on a note: it is a *map* the note carries, one entry per key,
//! with a count and — for the signed-in account — at most one entry of its own. So the row
//! draws the map the instance reported, keyed the way the instance spells the key (a
//! Unicode emoji, or a `:shortcode:` for a custom one), and offers the palette beside it.
//!
//! The row is drawn only when a caller brings a tally or an explanation for its absence: a
//! Mastodon status has neither, and the row must not add a line of empty space to it.

use leptos::prelude::*;

use crate::icons::Icon;

/// One note's reactions, as the instance reported them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReactionTally {
    /// One entry per key, in the instance's order.
    pub entries: Vec<(String, u64)>,
    /// The key this session placed, when it placed one; the instance's own report does not
    /// name it, so a freshly loaded note has counts and no `mine`.
    pub mine: Option<String>,
    /// Whether the account has any reaction on the note.
    pub reacted: bool,
}

/// What the picker offers without asking the reader to type.
///
/// Misskey accepts any emoji key, including a custom `:shortcode:` the instance defines; a
/// full picker would have to carry the emoji data the composer already loads. The footer
/// offers the handful a reader reaches for, and the palettes elsewhere stay the way to pick
/// an unusual one.
pub const PALETTE: [&str; 6] = ["❤️", "👍", "😂", "🎉", "👀", "🔥"];

/// The tally and the two controls that change it.
#[component]
pub fn ReactionRow(
    /// The reactions the note carries.
    tally: ReactionTally,
    /// Sets the reader's reaction to a key; absent when the instance serves no reactions.
    ///
    /// Required rather than `#[prop(optional)]`, because the absence is the thing the row
    /// draws differently: an optional prop would make `None` unreachable from the caller.
    on_react: Option<Callback<String>>,
    /// Removes the reader's reaction, whichever key it is.
    on_unreact: Option<Callback<()>>,
    /// Why the row offers no control, when the instance refused one.
    note: Option<String>,
) -> impl IntoView {
    // Taken apart rather than read through the struct: the row draws the tally and never
    // keeps it, so the parts belong to the row now.
    let ReactionTally {
        entries,
        mine,
        reacted,
    } = tally;
    let chips: Vec<AnyView> = entries
        .iter()
        .map(|(key, count)| {
            let mine = mine.as_deref() == Some(key);
            chip(key, *count, mine, on_react, on_unreact)
        })
        .collect();
    let remove = (reacted && on_unreact.is_some()).then(|| {
        view! {
            <button
                class="btn btn-ghost btn-sm h-8 min-h-8 px-2"
                title="Remove your reaction"
                aria-label="Remove your reaction"
                on:click=move |_| {
                    if let Some(on_unreact) = on_unreact {
                        on_unreact.run(());
                    }
                }
            >
                <Icon icon=Icon::Close/>
            </button>
        }
    });
    let picker = on_react.map(|on_react| {
        view! {
            <details class="dropdown">
                <summary class="btn btn-ghost btn-sm h-8 min-h-8 px-2" aria-label="Add a reaction">
                    <Icon icon=Icon::Plus/>
                </summary>
                <ul class="dropdown-content menu z-10 mt-1 flex-row gap-1 rounded-box bg-base-100 p-1 shadow">
                    {PALETTE
                        .iter()
                        .map(|key| {
                            let key = (*key).to_owned();
                            let label = format!("React with {key}");
                            let picked = key.clone();
                            view! {
                                <li>
                                    <button
                                        class="btn btn-ghost btn-sm h-8 min-h-8 px-2"
                                        aria-label=label
                                        on:click=move |_| on_react.run(picked.clone())
                                    >
                                        {key}
                                    </button>
                                </li>
                            }
                        })
                        .collect_view()}
                </ul>
            </details>
        }
    });
    view! {
        <div class="flex w-full flex-wrap items-center gap-1">
            {chips}
            {remove}
            {picker}
            {note.map(|note| view! { <span class="text-xs opacity-60">{note}</span> })}
        </div>
    }
}

/// One reaction: its key as the instance spells it, and its count.
///
/// The key is the label because it *is* the emoji; a count of one is still shown, so the row
/// reads the same whether a reaction is the reader's own or a stranger's.
fn chip(
    key: &str,
    count: u64,
    mine: bool,
    on_react: Option<Callback<String>>,
    on_unreact: Option<Callback<()>>,
) -> AnyView {
    let label = format!("{key} reaction, {count}");
    let class = if mine {
        "btn btn-ghost btn-sm btn-active h-8 min-h-8 px-2 tabular-nums"
    } else {
        "btn btn-ghost btn-sm h-8 min-h-8 px-2 tabular-nums"
    };
    // Built twice rather than once: a view is moved into the tree it is placed in, so the
    // button and the span each need their own.
    let body = || {
        view! {
            <span>{key.to_owned()}</span>
            <span class="ml-1 text-xs">{count}</span>
        }
    };
    // A chip that is already the reader's own is the way to take it back: pressing it again
    // says "not this one", which is the only thing a second press of the same emoji could
    // mean — the instance would otherwise just re-place the reaction it already holds.
    if mine {
        if let Some(on_unreact) = on_unreact {
            return view! {
                <button
                    class=class
                    title="Remove your reaction"
                    aria-label=label
                    on:click=move |_| on_unreact.run(())
                >
                    {body()}
                </button>
            }
            .into_any();
        }
    } else if let Some(on_react) = on_react {
        let pressed = key.to_owned();
        return view! {
            <button
                class=class
                title="React with this"
                aria-label=label
                on:click=move |_| on_react.run(pressed.clone())
            >
                {body()}
            </button>
        }
        .into_any();
    }
    view! {
        <span class=format!("{class} cursor-default pointer-events-none") aria-label=label>
            {body()}
        </span>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use leptos::prelude::{Callback, Owner, RenderHtml as _};

    use super::{ReactionRow, ReactionRowProps, ReactionTally};

    /// The map one note reported, with the reader's own key in it.
    fn tally() -> ReactionTally {
        ReactionTally {
            entries: vec![("❤️".to_owned(), 3), (":blob:".to_owned(), 1)],
            mine: Some("❤️".to_owned()),
            reacted: true,
        }
    }

    fn render(on_react: bool, note: Option<&str>) -> String {
        Owner::new().with(|| {
            ReactionRow(ReactionRowProps {
                tally: tally(),
                on_react: on_react.then(|| Callback::new(|_: String| ())),
                on_unreact: on_react.then(|| Callback::new(|(): ()| ())),
                note: note.map(str::to_owned),
            })
            .to_html()
        })
    }

    #[test]
    fn reaction_row_draws_each_key_the_way_the_instance_spells_it_with_its_count() {
        let rendered = render(false, None);
        assert!(rendered.contains("❤️"), "{rendered}");
        assert!(rendered.contains(":blob:"), "{rendered}");
        assert!(rendered.contains(">3<"), "{rendered}");
        assert!(rendered.contains(">1<"), "{rendered}");
        // Read-only here, so a chip is not a button and says so: there is nothing to press.
        assert!(!rendered.contains("<button"), "{rendered}");
        assert!(rendered.contains("pointer-events-none"), "{rendered}");
    }

    #[test]
    fn reaction_row_offers_a_palette_and_the_way_back_when_the_instance_can_react() {
        let rendered = render(true, None);
        assert!(rendered.contains("Add a reaction"), "{rendered}");
        assert!(rendered.contains("React with ❤️"), "{rendered}");
        assert!(rendered.contains("Remove your reaction"), "{rendered}");
        // The reader's own key is the button that takes it back, not one that re-places it.
        assert!(rendered.contains("btn-active"), "{rendered}");
    }

    #[test]
    fn reaction_row_explains_an_unsupported_instance_instead_of_offering_a_control() {
        let rendered = render(false, Some("This instance does not serve reactions."));
        assert!(
            rendered.contains("This instance does not serve reactions."),
            "{rendered}"
        );
        assert!(!rendered.contains("Add a reaction"), "{rendered}");
        assert!(!rendered.contains("Remove your reaction"), "{rendered}");
    }
}
