//! The verbs a full client needs, grouped by what they act on.
//!
//! [`InstanceApi`](crate::api::InstanceApi) is the *read* surface the SDK state machine
//! drives: timelines, one status, notifications. Everything a user does — favouriting,
//! boosting, voting, following, blocking — is an action, and actions are not state: the
//! caller decides when to run one and what to do with the answer. Keeping them out of
//! `InstanceApi` means a fake instance in a test implements three reads, not thirty
//! methods, and it keeps the trait a description of what the SDK needs rather than a
//! description of Mastodon.
//!
//! Every method returns the object the instance returns, not `()`, because the client
//! renders the answer: an optimistic update followed by a silent failure is worse than
//! not updating at all.
//!
//! Follow, unfollow and delete already exist on
//! [`InstanceApi`](crate::api::InstanceApi) — they predate this split because the
//! recommender's sync worker needs them — so they are deliberately absent here. Two
//! traits declaring one method would make every call site ambiguous.

use core::fmt::Debug;
use core::future::Future;

use crate::model::{
    Account, Conversation, Draft, HistoryEntry, List, LocalId, Page, PageParams, Relationship,
    SearchResults, Status,
};

/// Things done to a status.
pub trait StatusActions {
    /// Error type of the transport.
    type Error: Debug;

    /// Favourites a status, returning it as the instance now reports it.
    fn favourite(&self, id: &str) -> impl Future<Output = Result<Status, Self::Error>>;

    /// Removes the signed-in account's favourite.
    fn unfavourite(&self, id: &str) -> impl Future<Output = Result<Status, Self::Error>>;

    /// Boosts a status.
    fn reblog(&self, id: &str) -> impl Future<Output = Result<Status, Self::Error>>;

    /// Removes the boost.
    fn unreblog(&self, id: &str) -> impl Future<Output = Result<Status, Self::Error>>;

    /// Bookmarks a status; private to the account, so no other instance learns it.
    fn bookmark(&self, id: &str) -> impl Future<Output = Result<Status, Self::Error>>;

    /// Removes the bookmark.
    fn unbookmark(&self, id: &str) -> impl Future<Output = Result<Status, Self::Error>>;

    /// Pins a status to the account's profile.
    fn pin(&self, id: &str) -> impl Future<Output = Result<Status, Self::Error>>;

    /// Unpins it.
    fn unpin(&self, id: &str) -> impl Future<Output = Result<Status, Self::Error>>;

    /// Edits a status, keeping its id and adding a revision.
    fn edit_status(
        &self,
        id: &str,
        draft: &Draft,
    ) -> impl Future<Output = Result<Status, Self::Error>>;

    /// Votes on an attached poll.
    fn vote(
        &self,
        poll_id: &str,
        choices: &[usize],
    ) -> impl Future<Output = Result<Status, Self::Error>>;

    /// The edit history, oldest first.
    fn history(&self, id: &str) -> impl Future<Output = Result<Vec<HistoryEntry>, Self::Error>>;

    /// Accounts that boosted the status.
    fn reblogged_by(
        &self,
        id: &str,
        params: PageParams,
    ) -> impl Future<Output = Result<Page<Account>, Self::Error>>;

    /// Accounts that favourited the status.
    fn favourited_by(
        &self,
        id: &str,
        params: PageParams,
    ) -> impl Future<Output = Result<Page<Account>, Self::Error>>;
}

/// Things done to an account.
pub trait AccountActions {
    /// Error type of the transport.
    type Error: Debug;

    /// Blocks: no follows in either direction, no mentions.
    fn block(&self, id: &LocalId) -> impl Future<Output = Result<Relationship, Self::Error>>;

    /// Lifts the block.
    fn unblock(&self, id: &LocalId) -> impl Future<Output = Result<Relationship, Self::Error>>;

    /// Mutes: their posts leave every timeline, they are not told.
    fn mute(&self, id: &LocalId) -> impl Future<Output = Result<Relationship, Self::Error>>;

    /// Lifts the mute.
    fn unmute(&self, id: &LocalId) -> impl Future<Output = Result<Relationship, Self::Error>>;

    /// Relationships for several accounts in one call.
    ///
    /// Returns them in the order asked; an instance may answer fewer, which the caller
    /// reads as "unknown" rather than as "not following".
    fn relationships(
        &self,
        ids: &[LocalId],
    ) -> impl Future<Output = Result<Vec<Relationship>, Self::Error>>;

    /// Who follows `id`.
    fn followers(
        &self,
        id: &LocalId,
        params: PageParams,
    ) -> impl Future<Output = Result<Page<Account>, Self::Error>>;

    /// Who `id` follows.
    fn following(
        &self,
        id: &LocalId,
        params: PageParams,
    ) -> impl Future<Output = Result<Page<Account>, Self::Error>>;
}

/// The collections a client lists: favourites, bookmarks, conversations, lists.
pub trait Browse {
    /// Error type of the transport.
    type Error: Debug;

    /// Statuses the account favourited, newest first.
    fn favourites(
        &self,
        params: PageParams,
    ) -> impl Future<Output = Result<Page<Status>, Self::Error>>;

    /// Statuses the account bookmarked, newest first.
    fn bookmarks(
        &self,
        params: PageParams,
    ) -> impl Future<Output = Result<Page<Status>, Self::Error>>;

    /// Direct-message threads, newest first.
    fn conversations(
        &self,
        params: PageParams,
    ) -> impl Future<Output = Result<Page<Conversation>, Self::Error>>;

    /// Marks a conversation as read.
    fn mark_conversation_read(&self, id: &str) -> impl Future<Output = Result<(), Self::Error>>;

    /// The account's lists.
    fn lists(&self) -> impl Future<Output = Result<Vec<List>, Self::Error>>;

    /// Creates a list.
    fn create_list(&self, title: &str) -> impl Future<Output = Result<List, Self::Error>>;

    /// Renames a list.
    fn rename_list(&self, id: &str, title: &str)
    -> impl Future<Output = Result<List, Self::Error>>;

    /// Deletes a list; its members stay followed.
    fn delete_list(&self, id: &str) -> impl Future<Output = Result<(), Self::Error>>;

    /// Adds accounts to a list.
    fn add_to_list(
        &self,
        list_id: &str,
        accounts: &[LocalId],
    ) -> impl Future<Output = Result<(), Self::Error>>;

    /// Removes accounts from a list.
    fn remove_from_list(
        &self,
        list_id: &str,
        accounts: &[LocalId],
    ) -> impl Future<Output = Result<(), Self::Error>>;
}

/// Searching the instance and, when `resolve` is set, the fediverse.
pub trait Search {
    /// Error type of the transport.
    type Error: Debug;

    /// Searches accounts, statuses and hashtags.
    ///
    /// `resolve` is the difference between "what this instance already knows" and
    /// "go and fetch it, which may take seconds and may find nothing": a search-as-you-
    /// type box wants the former, a deliberate lookup the latter.
    fn search(
        &self,
        query: &str,
        resolve: bool,
    ) -> impl Future<Output = Result<SearchResults, Self::Error>>;
}
