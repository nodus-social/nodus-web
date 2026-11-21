//! The reaction tally a card draws, and the two verbs that change it.
//!
//! A shared [`Status`] has a favourite count and no reaction map, and Misskey's tally is a
//! separate document from the note itself — so the counts a card draws are kept here, keyed
//! by note id, rather than carried on the status every provider shares. Keeping them is the
//! only reason a chip keeps its count across a repaint: the card re-renders from a signal,
//! and a tally fetched once is not fetched again.
//!
//! Whether the instance serves reactions at all is learned the same way a browser learns it:
//! by asking. A Misskey-compatible server that answers `404` to `reactions/create` has no
//! reactions, and that is a fact about the server worth remembering — the footer stops
//! offering the picker instead of raising an error on every click.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use leptos::prelude::*;
use nodus_client_core::Error;
use nodus_misskey::Reactions;
use nodus_ui::status_footer::ReactionTally;

/// The tally is the shared row's own type, so the counts the bridge keeps and the counts a
/// card draws are one value with one set of field names rather than two that have to agree.
pub type Tally = ReactionTally;

thread_local! {
    static HELD: RefCell<HashMap<String, Tally>> = RefCell::new(HashMap::new());

    /// Which notes already have a read on its way, so a card that paints twice does not pay
    /// for the same request twice.
    static ASKED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());

    /// `Some(false)` once the instance has said it has no reactions; `None` until asked.
    static SERVES: Cell<Option<bool>> = const { Cell::new(None) };
}

fn revision() -> RwSignal<u64> {
    thread_local! {
        /// A root of its own, for the same reason as the protocol counter: this one is
        /// bumped by a task that can land after the card that started it is gone, and a
        /// counter owned by that card would be disposed by then.
        static ROOT: Owner = Owner::new_root(None);
        static REVISION: RwSignal<u64> = ROOT.with(|root| root.with(|| RwSignal::new(0)));
    }
    REVISION.with(|revision| *revision)
}

fn bump() {
    revision().update(|generation| *generation = generation.wrapping_add(1));
}

/// One note's tally, as a signal a card can draw from.
#[must_use]
pub fn tally(note_id: &str) -> Signal<Option<Tally>> {
    let revision = revision();
    let note_id = note_id.to_owned();
    Signal::derive(move || {
        revision.get();
        HELD.with_borrow(|held| held.get(&note_id).cloned())
    })
}

/// Whether this instance serves reactions, as a signal a card can gate on.
///
/// True until the server says otherwise: the console cannot tell a Misskey-compatible server
/// from a Mastodon one before the first request, and hiding a control that would have worked
/// is worse than offering one that quietly does not.
#[must_use]
pub fn serves() -> Signal<bool> {
    let revision = revision();
    Signal::derive(move || {
        revision.get();
        SERVES.with(Cell::get).unwrap_or(true)
    })
}

/// Asks the instance for a note's tally when the card does not already hold one.
///
/// The status a card draws does not always come from a note read: a shared link is read
/// anonymously, and a verb answers with a status, not with a note payload. The shared model
/// has one favourite count and no room for a reaction map, so the map a card needs is asked
/// for by id — the one thing a screen is *about* is worth a read of its own.
pub fn load(note_id: &str) {
    if HELD.with_borrow(|held| held.contains_key(note_id)) {
        return;
    }
    let fresh = ASKED.with_borrow_mut(|asked| asked.insert(note_id.to_owned()));
    if !fresh {
        return;
    }
    let id = note_id.to_owned();
    leptos::task::spawn_local(async move {
        let Some(bridge) = super::current() else {
            return;
        };
        match bridge.note(&id).await {
            Ok(note) => remember(&id, &note.reactions, note.reacted),
            // The card was drawn from what the reader already had, so a note that cannot be
            // re-read costs the chips and nothing else. It is forgotten rather than
            // remembered as unreadable: a later visit may be answered.
            Err(_) => ASKED.with_borrow_mut(|asked| {
                asked.remove(&id);
            }),
        }
    });
}

/// Records what a note read reported, so the card draws it.
pub(super) fn remember(note_id: &str, reactions: &Reactions, reacted: bool) {
    let entries: Vec<(String, u64)> = reactions
        .iter()
        .map(|(emoji, count)| (emoji.to_owned(), count))
        .collect();
    HELD.with_borrow_mut(|held| {
        let mine = held.get(note_id).and_then(|tally| tally.mine.clone());
        held.insert(
            note_id.to_owned(),
            Tally {
                entries,
                mine,
                reacted,
            },
        );
    });
    bump();
}

/// Adds a reaction, or replaces the one the account already placed.
///
/// Misskey allows one reaction per account, so this is the reader's own reaction changing,
/// not a second one being added.
pub async fn react(note_id: &str, emoji: &str) -> Result<(), String> {
    let Some(bridge) = super::current() else {
        return Err("Not signed in.".to_owned());
    };
    match bridge.react(note_id, emoji).await {
        Ok(reactions) => {
            SERVES.with(|serves| serves.set(Some(true)));
            settle(note_id, &reactions, true, Some(emoji.to_owned()));
            Ok(())
        }
        Err(error) => quiet(error),
    }
}

/// Removes the account's reaction, whichever key it was.
pub async fn unreact(note_id: &str) -> Result<(), String> {
    let Some(bridge) = super::current() else {
        return Err("Not signed in.".to_owned());
    };
    match bridge.unreact(note_id).await {
        Ok(reactions) => {
            SERVES.with(|serves| serves.set(Some(true)));
            settle(note_id, &reactions, false, None);
            Ok(())
        }
        Err(error) => quiet(error),
    }
}

/// Records a successful change.
fn settle(note_id: &str, reactions: &Reactions, reacted: bool, mine: Option<String>) {
    remember(note_id, reactions, reacted);
    HELD.with_borrow_mut(|held| {
        if let Some(tally) = held.get_mut(note_id) {
            tally.mine = mine;
        }
    });
    bump();
}

/// Turns "this server has no reactions" into a quiet absence, and everything else into a
/// message the reader can act on.
fn quiet(error: Error) -> Result<(), String> {
    if let Error::Status { status: 404, .. } = error {
        SERVES.with(|serves| serves.set(Some(false)));
        // The tally the note already reported stays as it is: the note does have reactions,
        // the console just cannot change them here.
        bump();
        return Ok(());
    }
    Err(error.to_string())
}

/// Forgets every tally, for a session that ended.
pub(super) fn forget() {
    HELD.with_borrow_mut(HashMap::clear);
    ASKED.with_borrow_mut(HashSet::clear);
    SERVES.with(|serves| serves.set(None));
    bump();
}
