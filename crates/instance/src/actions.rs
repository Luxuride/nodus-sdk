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
    Account, AdminAccount, AdminMeasure, Announcement, Conversation, DomainAllow, DomainBlock,
    DomainSeverity, Draft, FeaturedTag, Filter, FilterDraft, HistoryEntry, InstanceInfo, Invite,
    List, LocalId, MediaAttachment, ModerationAction, Page, PageParams, Preferences, Relationship,
    Relay, Report, ReportState, Rule, ScheduledStatus, SearchResults, Status,
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
pub trait MediaActions {
    type Error: Debug;

    /// Uploads one attachment and returns what the instance made of it.
    ///
    /// The bytes are passed as a slice because the caller owns the file object: the SDK
    /// never opens a path, web or native.
    fn upload_media(
        &self,
        bytes: &[u8],
        mime: &str,
        description: Option<&str>,
    ) -> impl Future<Output = Result<MediaAttachment, Self::Error>>;
}

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
    fn follow_requests(&self) -> impl Future<Output = Result<Vec<Account>, Self::Error>>;

    /// Accepts a pending follow request.
    fn authorize_follow_request(
        &self,
        id: &LocalId,
    ) -> impl Future<Output = Result<Relationship, Self::Error>>;

    /// Refuses a pending follow request.
    fn reject_follow_request(&self, id: &LocalId) -> impl Future<Output = Result<(), Self::Error>>;

    fn mutes(&self, params: PageParams)
    -> impl Future<Output = Result<Page<Account>, Self::Error>>;

    fn blocks(
        &self,
        params: PageParams,
    ) -> impl Future<Output = Result<Page<Account>, Self::Error>>;

    fn following(
        &self,
        id: &LocalId,
        params: PageParams,
    ) -> impl Future<Output = Result<Page<Account>, Self::Error>>;

    /// Changes the signed-in account's own profile.
    ///
    /// Answers the account as the instance now holds it, so the caller renders the
    /// sanitised result rather than the text it sent.
    fn update_profile(
        &self,
        update: &crate::model::ProfileUpdate,
    ) -> impl Future<Output = Result<Account, Self::Error>>;
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

    /// Accounts the instance suggests, for finding people to follow.
    fn directory(
        &self,
        local_only: bool,
        offset: u32,
    ) -> impl Future<Output = Result<Vec<Account>, Self::Error>>;

    /// Mutes or unmutes a conversation.
    fn mute_conversation(
        &self,
        id: &str,
        muted: bool,
    ) -> impl Future<Output = Result<Conversation, Self::Error>>;

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

    /// The accounts on a list.
    ///
    /// Mastodon returns the whole membership, not a page: a list is a hand-picked set
    /// of people, and the client needs all of them to answer "is this account on it".
    fn list_accounts(
        &self,
        list_id: &str,
    ) -> impl Future<Output = Result<Vec<Account>, Self::Error>>;

    /// A list's timeline.
    ///
    /// Same cursor rules as the other timelines — a full page is what produces the next
    /// id — because it is the same endpoint shape and the same UI below it.
    fn list_timeline(
        &self,
        list_id: &str,
        params: PageParams,
    ) -> impl Future<Output = Result<Page<Status>, Self::Error>>;
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

/// The reader's own filters.
///
/// A filter is not a status action: it changes what a *timeline* shows, and the server
/// already applies it. This surface exists so the client can edit the list and, for a
/// thread assembled from cached statuses, apply the same rule itself.
pub trait FilterActions {
    /// Error type of the transport.
    type Error: Debug;

    /// Every filter this account keeps.
    fn filters(&self) -> impl Future<Output = Result<Vec<Filter>, Self::Error>>;

    /// Create a filter.
    fn create_filter(
        &self,
        draft: &FilterDraft,
    ) -> impl Future<Output = Result<Filter, Self::Error>>;

    /// Replace a filter's settings and phrases.
    fn update_filter(
        &self,
        id: &str,
        draft: &FilterDraft,
    ) -> impl Future<Output = Result<Filter, Self::Error>>;

    /// Delete a filter and its phrases.
    fn delete_filter(&self, id: &str) -> impl Future<Output = Result<(), Self::Error>>;
}

/// Moderation: the report queue, account standing, blocked domains.
///
/// Every call here needs `admin:read` or `admin:write`, so a client must ask before it
/// renders a screen: an ordinary token gets a 403, and a menu that leads to a 403 is
/// worse than a menu that is not there.
pub trait AdminActions {
    /// Error type of the transport.
    type Error: Debug;

    /// Reports waiting for a decision, or already decided.
    fn reports(
        &self,
        state: ReportState,
        limit: u16,
    ) -> impl Future<Output = Result<Vec<Report>, Self::Error>>;

    /// Mark a report as handled.
    fn resolve_report(&self, id: &str) -> impl Future<Output = Result<Report, Self::Error>>;

    /// Reopen a handled report.
    fn reopen_report(&self, id: &str) -> impl Future<Output = Result<Report, Self::Error>>;

    /// Accounts, newest first, optionally only local ones.
    fn admin_accounts(
        &self,
        local: bool,
        limit: u16,
    ) -> impl Future<Output = Result<Vec<AdminAccount>, Self::Error>>;

    /// Apply a decision to one account.
    fn moderate_account(
        &self,
        id: &str,
        action: ModerationAction,
    ) -> impl Future<Output = Result<AdminAccount, Self::Error>>;

    /// Domains the instance refuses.
    fn domain_blocks(
        &self,
        limit: u16,
    ) -> impl Future<Output = Result<Vec<DomainBlock>, Self::Error>>;

    /// Refuse a domain.
    fn create_domain_block(
        &self,
        domain: &str,
        severity: DomainSeverity,
    ) -> impl Future<Output = Result<DomainBlock, Self::Error>>;

    /// Stop refusing a domain.
    fn delete_domain_block(&self, id: &str) -> impl Future<Output = Result<(), Self::Error>>;
    /// Announcements every account sees, including unpublished ones.
    fn admin_announcements(&self) -> impl Future<Output = Result<Vec<Announcement>, Self::Error>>;
    /// Publishes one. Empty `starts_at`/`ends_at` mean "now" and "never ends".
    fn create_announcement(
        &self,
        content: &str,
        starts_at: Option<&str>,
        ends_at: Option<&str>,
    ) -> impl Future<Output = Result<Announcement, Self::Error>>;
    fn delete_announcement(&self, id: &str) -> impl Future<Output = Result<(), Self::Error>>;
    /// Dashboard counts for the given keys (`new_users`, `interactions`, …) between two
    /// dates, both `YYYY-MM-DD`.
    fn measures(
        &self,
        keys: &[&str],
        start_at: &str,
        end_at: &str,
    ) -> impl Future<Output = Result<Vec<AdminMeasure>, Self::Error>>;
    /// Relays this instance follows.
    fn relays(&self) -> impl Future<Output = Result<Vec<Relay>, Self::Error>>;
    fn add_relay(&self, inbox_url: &str) -> impl Future<Output = Result<Relay, Self::Error>>;
    fn remove_relay(&self, id: &str) -> impl Future<Output = Result<(), Self::Error>>;
    /// Domains allowed through a general block.
    fn domain_allows(
        &self,
        limit: u16,
    ) -> impl Future<Output = Result<Vec<DomainAllow>, Self::Error>>;
    fn create_domain_allow(
        &self,
        domain: &str,
    ) -> impl Future<Output = Result<DomainAllow, Self::Error>>;
    fn delete_domain_allow(&self, id: &str) -> impl Future<Output = Result<(), Self::Error>>;
}

/// What the account asked for, and what the instance wants said.
///
/// Preferences belong to the account rather than to the browser: a reader who always
/// posts unlisted should not have to set that in every client they sign into.
pub trait SettingsActions {
    /// Error type of the transport.
    type Error: Debug;

    /// The account's posting and reading defaults.
    fn preferences(&self) -> impl Future<Output = Result<Preferences, Self::Error>>;

    /// Announcements the instance is showing.
    fn announcements(&self) -> impl Future<Output = Result<Vec<Announcement>, Self::Error>>;

    /// Marks an announcement as read, so it stops appearing.
    fn dismiss_announcement(&self, id: &str) -> impl Future<Output = Result<(), Self::Error>>;
}

/// Invites and the approval queue: who may join this instance.
pub trait RegistrationActions {
    type Error: Debug;

    /// The invites this account may hand out.
    fn invites(&self, limit: u16) -> impl Future<Output = Result<Vec<Invite>, Self::Error>>;
    /// Creates one. `max_uses` of `None` means unlimited; `expires_in` is in seconds.
    fn create_invite(
        &self,
        max_uses: Option<u32>,
        expires_in: Option<u64>,
        autofollow: bool,
    ) -> impl Future<Output = Result<Invite, Self::Error>>;
    fn revoke_invite(&self, id: &str) -> impl Future<Output = Result<(), Self::Error>>;

    /// Accounts waiting for a moderator's decision.
    fn pending_accounts(
        &self,
        limit: u16,
    ) -> impl Future<Output = Result<Vec<AdminAccount>, Self::Error>>;
    fn approve_account(&self, id: &str) -> impl Future<Output = Result<AdminAccount, Self::Error>>;
    fn reject_account(&self, id: &str) -> impl Future<Output = Result<AdminAccount, Self::Error>>;
}

/// What a server says about itself.
///
/// Readable without a token: a name, a version and a contact address are what someone
/// deciding whether to sign in needs first.
pub trait InstanceMeta {
    type Error: Debug;

    fn instance_info(&self) -> impl Future<Output = Result<InstanceInfo, Self::Error>>;
}

/// Featured tags, and the instance's rules.
///
/// The rules are read without a token: they are what someone about to join is asked to
/// accept, so requiring a session to see them would be backwards.
pub trait HighlightActions {
    /// Error type of the transport.
    type Error: Debug;

    /// The tags an account has put on its profile.
    fn featured_tags(
        &self,
        id: &LocalId,
    ) -> impl Future<Output = Result<Vec<FeaturedTag>, Self::Error>>;

    /// Features a tag on the authenticated account's profile.
    fn feature_tag(&self, name: &str) -> impl Future<Output = Result<FeaturedTag, Self::Error>>;

    /// Removes a tag from the profile.
    fn unfeature_tag(&self, id: &str) -> impl Future<Output = Result<(), Self::Error>>;

    /// The instance's code of conduct.
    fn rules(&self) -> impl Future<Output = Result<Vec<Rule>, Self::Error>>;
}

/// Posts the instance will publish later.
///
/// Scheduling is the same call as posting with a time attached, which is why it takes a
/// [`Draft`] rather than its own shape: the content is identical, only the moment differs.
pub trait ScheduleActions {
    /// Error type of the transport.
    type Error: Debug;

    /// Everything waiting to be published, soonest first.
    fn scheduled(&self) -> impl Future<Output = Result<Vec<ScheduledStatus>, Self::Error>>;

    /// Queues a draft for `at` (RFC 3339).
    fn schedule(
        &self,
        draft: &Draft,
        at: &str,
    ) -> impl Future<Output = Result<ScheduledStatus, Self::Error>>;

    /// Cancels a queued post.
    fn cancel_scheduled(&self, id: &str) -> impl Future<Output = Result<(), Self::Error>>;
}
