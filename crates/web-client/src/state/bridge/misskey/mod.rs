//! The Misskey verbs the console's cards call, in the shape the cards already speak.
//!
//! A newtype rather than a second client: `MisskeyInstance` already implements the read
//! surface, but `StatusActions` and the SDK type are both foreign to this crate, so the
//! implementation has to live on a type of ours (the orphan rule). The two places Misskey's
//! wire and the shared model disagree are corrected here and nowhere else — which "like" the
//! star means, and a tally `Status` has no field for — because every other provider shares
//! that model and must not see Misskey's reading of it.
//!
//! One difference is left standing on purpose: a Misskey status reports the *reaction* total
//! in `favourites_count`, since that is the number Misskey's own note carries. The star's
//! count is therefore not a favourite count on a Misskey instance. Correcting it would mean
//! a second request per card and a model change shared by every provider, so the console
//! leaves the number as the instance reports it and the star as the verb it runs.

use nodus_client_core::model::Status;
use nodus_client_core::rest::Transport;
use nodus_client_core::{Instance, InstanceApi, Result};
use nodus_mastodon::GlooTransport;
use nodus_misskey::{MisskeyInstance, Reactions};

pub mod actions;
pub mod reactions;
pub mod read;

#[cfg(test)]
mod tests;

/// One instance's Misskey verbs, for the browser's transport.
pub type BrowserMisskey = MisskeyBridge<GlooTransport>;

/// One note as the card draws it.
#[derive(Debug, Clone)]
pub struct Note {
    /// The note in the shared shape, with the star this console draws.
    pub status: Status,
    /// Every reaction on the note, keyed the way Misskey spells them.
    pub reactions: Reactions,
    /// Whether the account has already reacted at all.
    ///
    /// Misskey reports it as `myReaction`, which names the key as well; the tally carries
    /// counts only, so whether a reaction exists is read on its own — removing one does not
    /// need to know which key it was.
    pub reacted: bool,
}

/// An instance's note verbs.
#[derive(Debug, Clone)]
pub struct MisskeyBridge<T: Transport> {
    inner: MisskeyInstance<T>,
}

impl<T: Transport> MisskeyBridge<T> {
    /// One instance, one token, one transport.
    pub fn with_transport(instance: &Instance, token: &str, transport: T) -> Self {
        Self {
            inner: MisskeyInstance::with_transport(instance, token, transport),
        }
    }

    /// The Misskey client this bridge corrects.
    fn inner(&self) -> &MisskeyInstance<T> {
        &self.inner
    }

    /// One note, its tally, and whether the reader already reacted.
    ///
    /// Two reads for the one note a screen is *about*, and none at all for the rows around
    /// it: a timeline's notes arrive with their own `reactions` inside the payload the read
    /// already fetched, so a tally is only fetched here, where the console holds an id and
    /// no payload.
    pub async fn note(&self, id: &str) -> Result<Note> {
        let raw = self.inner.status(id).await?;
        // `convert::status` reads Misskey's `myReaction` into `favourited` and its
        // `isFavorited` into `bookmarked`. Both channels are independent on Misskey, so the
        // reaction is taken before [`starred`] folds the favourite into the star.
        let reacted = raw.favourited;
        let reactions = self.inner.reactions(id).await?;
        Ok(Note {
            status: starred(raw),
            reactions,
            reacted,
        })
    }

    /// Sets the account's reaction, returning the tally as the instance now reports it.
    ///
    /// Misskey keeps one reaction per account, so a second call replaces the first.
    pub async fn react(&self, id: &str, emoji: &str) -> Result<Reactions> {
        self.inner.react(id, emoji).await
    }

    /// Removes whatever reaction the account placed.
    ///
    /// The key is not sent: Misskey's delete names only the note, so a caller does not have
    /// to remember which reaction the reader chose, and the SDK's second argument is left
    /// empty at the one place where the wire is built.
    pub async fn unreact(&self, id: &str) -> Result<Reactions> {
        self.inner.unreact(id, "").await
    }
}

/// Re-reads the two "I like this" channels into the one flag the console's star draws.
///
/// Misskey keeps a private favourite (`isFavorited`) beside a public reaction (`myReaction`),
/// while the shared status has one favourite flag and one bookmark flag. The conversion
/// fills `favourited` from the reaction and `bookmarked` from the favourite; the console's
/// star is the favourite *verb*, so the favourite becomes the star and the bookmark is left
/// empty, because Misskey has no bookmark verb to draw.
fn starred(mut status: Status) -> Status {
    status.favourited = status.bookmarked;
    status.bookmarked = false;
    status
}

thread_local! {
    /// The bridge for the session's instance.
    ///
    /// Beside the SDK handle rather than inside it because the actions have to be taken
    /// from a transport, and the SDK's state machine is the Mastodon one: the bridge is the
    /// second path to the same screens, not a state store of its own.
    static INSTALLED: std::cell::RefCell<Option<BrowserMisskey>> = const {
        std::cell::RefCell::new(None)
    };
}

/// Installs the bridge for a session's instance.
pub(super) fn install(instance: &Instance, token: &str) {
    // A different account may be on a different server, with its own tallies and its own
    // answer about whether it serves reactions at all.
    reactions::forget();
    let bridge = MisskeyBridge::with_transport(instance, token, GlooTransport::new());
    INSTALLED.with_borrow_mut(|slot| *slot = Some(bridge));
}

/// Drops the bridge, for a session that ended.
pub(super) fn clear() {
    INSTALLED.with_borrow_mut(|slot| *slot = None);
}

/// Records a read note's tally, for the cards that draw it.
pub(super) fn remember(note_id: &str, note: &Note) {
    reactions::remember(note_id, &note.reactions, note.reacted);
}

/// Drops everything this instance's bridge remembered.
pub(super) fn forget() {
    clear();
    reactions::forget();
}

/// The installed bridge, if the session has one.
pub(super) fn current() -> Option<BrowserMisskey> {
    INSTALLED.with_borrow(std::clone::Clone::clone)
}
