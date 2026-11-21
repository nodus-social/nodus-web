//! The status verbs, for a server that has only some of them.
//!
//! Two verbs exist on Misskey — the private favourite and the public reaction — and the
//! console's star runs the favourite. Everything else the shared action trait offers
//! (boosting, bookmarking, pinning, editing, polls, edit history) has no Misskey endpoint at
//! all, so it answers [`Error::Unsupported`] with the verb named. A card that receives that
//! reads as "this instance cannot do that", which is what it is; a transport error would
//! claim the request failed, and invite a retry that can never succeed.

use core::future::Future;

use nodus_client_core::actions::StatusActions;
use nodus_client_core::model::{Account, Draft, HistoryEntry, Page, PageParams, Poll, Status};
use nodus_client_core::rest::Transport;
use nodus_client_core::{Error, InstanceApi, Result};

use super::{MisskeyBridge, starred};

/// The typed answer for a verb this API does not have.
///
/// `verb` completes the SDK's sentence — "{verb} is not supported by this instance API" —
/// so it is written the way a reader would say it.
fn unsupported<T>(verb: &'static str) -> Result<T> {
    Err(Error::Unsupported(verb))
}

impl<T: Transport> StatusActions for MisskeyBridge<T> {
    type Error = Error;

    /// The private favourite, which is what the console's star means.
    async fn favourite(&self, id: &str) -> Result<Status> {
        self.inner().favourite(id).await?;
        self.inner().status(id).await.map(starred)
    }

    async fn unfavourite(&self, id: &str) -> Result<Status> {
        self.inner().unfavourite(id).await?;
        self.inner().status(id).await.map(starred)
    }

    fn reblog(&self, _id: &str) -> impl Future<Output = Result<Status>> {
        core::future::ready(unsupported("boosting"))
    }

    fn unreblog(&self, _id: &str) -> impl Future<Output = Result<Status>> {
        core::future::ready(unsupported("undoing a boost"))
    }

    fn bookmark(&self, _id: &str) -> impl Future<Output = Result<Status>> {
        core::future::ready(unsupported("bookmarking"))
    }

    fn unbookmark(&self, _id: &str) -> impl Future<Output = Result<Status>> {
        core::future::ready(unsupported("removing a bookmark"))
    }

    fn pin(&self, _id: &str) -> impl Future<Output = Result<Status>> {
        core::future::ready(unsupported("pinning"))
    }

    fn unpin(&self, _id: &str) -> impl Future<Output = Result<Status>> {
        core::future::ready(unsupported("unpinning"))
    }

    fn edit_status(&self, _id: &str, _draft: &Draft) -> impl Future<Output = Result<Status>> {
        core::future::ready(unsupported("editing a note"))
    }

    fn vote(&self, _poll_id: &str, _choices: &[usize]) -> impl Future<Output = Result<Poll>> {
        core::future::ready(unsupported("voting"))
    }

    fn history(&self, _id: &str) -> impl Future<Output = Result<Vec<HistoryEntry>>> {
        core::future::ready(unsupported("the edit history"))
    }

    fn reblogged_by(
        &self,
        _id: &str,
        _params: PageParams,
    ) -> impl Future<Output = Result<Page<Account>>> {
        core::future::ready(unsupported("listing boosts"))
    }

    fn favourited_by(
        &self,
        _id: &str,
        _params: PageParams,
    ) -> impl Future<Output = Result<Page<Account>>> {
        core::future::ready(unsupported("listing favourites"))
    }
}
