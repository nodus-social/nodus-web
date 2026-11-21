//! The read surface, delegated.
//!
//! Misskey's client already implements [`InstanceApi`]; the bridge only has to correct the
//! two readings the shared model gets wrong for Misskey — which "like" the star shows, and
//! the reaction flag that travels in the same field. Everything else is passed through, so
//! the pages never learn which client answered.

use core::future::Future;

use nodus_client_core::model::{
    Account, Context, CredentialAccount, Draft, LocalId, Notification, Page, PageParams,
    Relationship, Status, StreamKind,
};
use nodus_client_core::rest::Transport;
use nodus_client_core::{Error, InstanceApi, Result};

use super::{MisskeyBridge, starred};

impl<T: Transport> InstanceApi for MisskeyBridge<T> {
    type Error = Error;

    fn verify_credentials(&self) -> impl Future<Output = Result<Account>> {
        self.inner().verify_credentials()
    }

    fn credential_account(&self) -> impl Future<Output = Result<CredentialAccount>> {
        self.inner().credential_account()
    }

    fn timeline(
        &self,
        kind: StreamKind,
        params: PageParams,
    ) -> impl Future<Output = Result<Page<Status>>> {
        self.inner().timeline(kind, params)
    }

    fn tag_timeline(
        &self,
        tag: &str,
        params: PageParams,
    ) -> impl Future<Output = Result<Page<Status>>> {
        self.inner().tag_timeline(tag, params)
    }

    fn list_timeline(
        &self,
        list_id: &str,
        params: PageParams,
    ) -> impl Future<Output = Result<Page<Status>>> {
        self.inner().list_timeline(list_id, params)
    }

    /// The star follows the favourite here too, so a note opened from a thread draws the
    /// same way as the card it was opened from.
    async fn status(&self, id: &str) -> Result<Status> {
        self.inner().status(id).await.map(starred)
    }

    async fn status_context(&self, id: &str) -> Result<Context> {
        let mut context = self.inner().status_context(id).await?;
        for status in context
            .ancestors
            .iter_mut()
            .chain(context.descendants.iter_mut())
        {
            *status = starred(status.clone());
        }
        Ok(context)
    }

    fn notifications(
        &self,
        params: PageParams,
    ) -> impl Future<Output = Result<Page<Notification>>> {
        self.inner().notifications(params)
    }

    fn account(&self, id: &LocalId) -> impl Future<Output = Result<Account>> {
        self.inner().account(id)
    }

    fn account_lookup(&self, acct: &str) -> impl Future<Output = Result<Account>> {
        self.inner().account_lookup(acct)
    }

    fn account_followers(
        &self,
        id: &LocalId,
        params: PageParams,
    ) -> impl Future<Output = Result<Page<Account>>> {
        self.inner().account_followers(id, params)
    }

    fn account_following(
        &self,
        id: &LocalId,
        params: PageParams,
    ) -> impl Future<Output = Result<Page<Account>>> {
        self.inner().account_following(id, params)
    }

    fn account_statuses(
        &self,
        id: &LocalId,
        params: PageParams,
    ) -> impl Future<Output = Result<Page<Status>>> {
        self.inner().account_statuses(id, params)
    }

    fn relationship(&self, id: &LocalId) -> impl Future<Output = Result<Relationship>> {
        self.inner().relationship(id)
    }

    fn follow(&self, id: &LocalId) -> impl Future<Output = Result<Relationship>> {
        self.inner().follow(id)
    }

    fn unfollow(&self, id: &LocalId) -> impl Future<Output = Result<Relationship>> {
        self.inner().unfollow(id)
    }

    fn post_status(&self, draft: &Draft) -> impl Future<Output = Result<Status>> {
        self.inner().post_status(draft)
    }

    fn delete_status(&self, id: &str) -> impl Future<Output = Result<()>> {
        self.inner().delete_status(id)
    }
}
