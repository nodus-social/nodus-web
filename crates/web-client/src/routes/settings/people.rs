//! The viewer's own mute and block lists.
//!
//! Both are the shared people list: the SDK holds the page, the row knows how to open an
//! account, and the only thing this screen decides is which page to read and which verb
//! an undo runs. The server paints the same list through `PeopleList`, so a reader who
//! arrives before hydration sees the same rows.

use leptos::prelude::*;
use leptos::task::spawn_local;
use nodus_client_sdk::Command;
use nodus_client_sdk::page::{PageItems, PageKind};
use nodus_pages::pages::people::{Kind, PeoplePage};

use crate::state::alive::Alive;
use crate::state::bridge::{actions, dispatch, pages, revision};

use super::refusal;

/// Which of the two lists this is. Kept separate from [`Kind`] because only these two
/// have no account id in their page key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    /// Accounts the viewer muted.
    Muted,
    /// Accounts the viewer blocked.
    Blocked,
}

impl Which {
    fn page(self) -> PageKind {
        match self {
            Self::Muted => PageKind::Mutes,
            Self::Blocked => PageKind::Blocks,
        }
    }

    fn kind(self) -> Kind {
        match self {
            Self::Muted => Kind::Mutes,
            Self::Blocked => Kind::Blocks,
        }
    }
}

/// One of the viewer's account lists, read from the SDK page store.
#[component]
pub fn SettingsPeople(which: Which) -> impl IntoView {
    let alive = Alive::new();
    let revision = revision();
    let key = StoredValue::new(which.page());
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal(None::<String>);

    // The store is read on every revision bump, so an undo's dropped row disappears
    // without this screen keeping a second copy of the list.
    let rows = Signal::derive(move || {
        revision.get();
        pages::items(&key.get_value(), |items| match items {
            PageItems::Accounts(list) => Some(list),
            _ => None,
        })
        .map(|(list, _)| list)
        .unwrap_or_default()
    });
    let cursor = Signal::derive(move || {
        revision.get();
        pages::items(&key.get_value(), |items| match items {
            PageItems::Accounts(list) => Some(list),
            _ => None,
        })
        .and_then(|(_, cursor)| cursor)
    });

    let read = Callback::new(move |after: Option<String>| {
        set_loading.set(true);
        let alive = alive;
        let key = key.get_value();
        spawn_local(async move {
            let outcome = match after {
                Some(after) => pages::ensure_after(key, Some(after)).await,
                None => pages::ensure(key).await,
            };
            alive.if_alive(|| {
                if let Err(message) = outcome {
                    set_error.set(Some(refusal::sentence(&message)));
                }
                set_loading.set(false);
            });
        });
    });

    Effect::new(move |_| read.run(None));

    let undo = Callback::new(move |id: String| {
        set_loading.set(true);
        let alive = alive;
        let key = key.get_value();
        spawn_local(async move {
            let outcome = match which {
                Which::Muted => actions::mute(&id, false).await.map(|_| ()),
                Which::Blocked => actions::block(&id, false).await.map(|_| ()),
            };
            alive.if_alive(|| {
                match outcome {
                    Ok(()) => {
                        // The list is the SDK's: dropping the account there is what
                        // makes the row go away, rather than hiding it in a local copy
                        // that the next refresh would bring back.
                        dispatch(Command::PageAccountDropped {
                            kind: key,
                            account: id,
                        });
                    }
                    Err(message) => set_error.set(Some(refusal::sentence(&message))),
                }
                set_loading.set(false);
            });
        });
    });

    let more = Callback::new(move |after: String| read.run(Some(after)));

    view! {
        // An empty list and a list still on its way look the same to `PeoplePage`, which
        // hides its empty line while loading. Say the read is happening here so the panel
        // is never blank, and leave the empty line to the answer that really is empty.
        <Show when=move || loading.get() && rows.get().is_empty()>
            <p class="text-sm opacity-60">
                {match which {
                    Which::Muted => "Reading the mute list…",
                    Which::Blocked => "Reading the block list…",
                }}
            </p>
        </Show>
        <PeoplePage
            kind=which.kind()
            accounts=rows
            loading=Signal::derive(move || loading.get())
            error=Signal::derive(move || error.get())
            cursor=cursor
            on_undo=undo
            on_more=more
        />
    }
}

#[cfg(test)]
mod tests {
    use super::Which;
    use nodus_client_sdk::page::PageKind;
    use nodus_pages::pages::people::Kind;

    #[test]
    fn each_list_reads_its_own_page_and_draws_its_own_verb() {
        assert_eq!(Which::Muted.page(), PageKind::Mutes);
        assert_eq!(Which::Muted.kind(), Kind::Mutes);
        assert_eq!(Which::Blocked.page(), PageKind::Blocks);
        assert_eq!(Which::Blocked.kind(), Kind::Blocks);
    }
}
