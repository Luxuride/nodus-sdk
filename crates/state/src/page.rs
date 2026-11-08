//! Pages that are not timelines: notifications, messages, saved posts, people, search.
//!
//! A timeline is a list of ids into the entity store, because a status must look the same
//! in every screen. Everything else here is a page of finished objects: a notification, a
//! conversation or a search result has no second copy to stay in step with.

use std::collections::HashSet;

use nodus_instance::model::{
    Account, Conversation, FeaturedTag, Filter, Notification, ScheduledStatus, SearchResults,
    Status,
};
use serde::{Deserialize, Serialize};

/// Which page a stored entry belongs to.
///
/// The account and the query are part of the key: "the followers of A" and "the followers
/// of B" are different pages, and a search for one word says nothing about another.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PageKind {
    Notifications,
    Conversations,
    Favourites,
    Bookmarks,
    Followers(String),
    Following(String),
    Mutes,
    Blocks,
    Scheduled,
    Search(String),
    /// The viewer's keyword filters.
    Filters,
    /// Hashtags pinned to the viewer's profile.
    FeaturedTags,
    /// The suggestion list, keyed by whether it is limited to this instance.
    ///
    /// The instance pages this list by offset rather than by id, so its cursor is a
    /// number written as text — see `Sdk::ensure_page`.
    Directory(bool),
}

/// The contents of one page.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub enum PageItems {
    #[default]
    Empty,
    Statuses(Vec<Status>),
    Notifications(Vec<Notification>),
    Accounts(Vec<Account>),
    Conversations(Vec<Conversation>),
    Scheduled(Vec<ScheduledStatus>),
    Filters(Vec<Filter>),
    FeaturedTags(Vec<FeaturedTag>),
    Search(Box<SearchResults>),
}

impl PageItems {
    /// The ids this page holds, in order. Search results have none: they are replaced whole.
    fn ids(&self) -> Vec<String> {
        match self {
            Self::Empty | Self::Search(_) => Vec::new(),
            Self::Statuses(items) => items.iter().map(|item| item.id.clone()).collect(),
            Self::Notifications(items) => items.iter().map(|item| item.id.clone()).collect(),
            Self::Accounts(items) => items.iter().map(|item| item.id.clone()).collect(),
            Self::Conversations(items) => items.iter().map(|item| item.id.clone()).collect(),
            Self::Scheduled(items) => items.iter().map(|item| item.id.clone()).collect(),
            Self::Filters(items) => items.iter().map(|item| item.id.clone()).collect(),
            Self::FeaturedTags(items) => items.iter().map(|item| item.id.clone()).collect(),
        }
    }

    /// Whether this page speaks the same language as `other`.
    #[must_use]
    pub fn same_shape(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

/// One stored page: its items, the cursor for older ones, and when it was fetched.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PageEntry {
    pub items: PageItems,
    pub cursor: Option<String>,
    pub fetched_at: Option<i64>,
}

impl PageEntry {
    /// Folds an older page into this one, keeping display order.
    ///
    /// Ids already held are dropped: an instance that shifts between requests would
    /// otherwise show the same notification twice, and a keyed list treats a duplicate
    /// key as a panic rather than a blemish.
    pub fn append(&mut self, older: Self) {
        if !self.items.same_shape(&older.items) {
            return;
        }
        let known: HashSet<String> = self.items.ids().into_iter().collect();
        let older_ids = older.items.ids();
        let keep: Vec<bool> = older_ids.iter().map(|id| !known.contains(id)).collect();

        match (&mut self.items, older.items) {
            (PageItems::Statuses(mine), PageItems::Statuses(mut theirs)) => {
                let mut kept = Vec::new();
                for (index, item) in theirs.drain(..).enumerate() {
                    if keep.get(index).copied().unwrap_or(false) {
                        kept.push(item);
                    }
                }
                mine.extend(kept);
            }
            (PageItems::Notifications(mine), PageItems::Notifications(mut theirs)) => {
                let mut kept = Vec::new();
                for (index, item) in theirs.drain(..).enumerate() {
                    if keep.get(index).copied().unwrap_or(false) {
                        kept.push(item);
                    }
                }
                mine.extend(kept);
            }
            (PageItems::Accounts(mine), PageItems::Accounts(mut theirs)) => {
                let mut kept = Vec::new();
                for (index, item) in theirs.drain(..).enumerate() {
                    if keep.get(index).copied().unwrap_or(false) {
                        kept.push(item);
                    }
                }
                mine.extend(kept);
            }
            (PageItems::Conversations(mine), PageItems::Conversations(mut theirs)) => {
                let mut kept = Vec::new();
                for (index, item) in theirs.drain(..).enumerate() {
                    if keep.get(index).copied().unwrap_or(false) {
                        kept.push(item);
                    }
                }
                mine.extend(kept);
            }
            (PageItems::Scheduled(mine), PageItems::Scheduled(mut theirs)) => {
                let mut kept = Vec::new();
                for (index, item) in theirs.drain(..).enumerate() {
                    if keep.get(index).copied().unwrap_or(false) {
                        kept.push(item);
                    }
                }
                mine.extend(kept);
            }
            _ => {}
        }
        if older.cursor.is_some() {
            self.cursor = older.cursor;
        }
    }

    /// Whether this page is younger than `ttl_ms`, measured at `now`.
    #[must_use]
    pub fn is_fresh(&self, now: i64, ttl_ms: i64) -> bool {
        self.fetched_at
            .is_some_and(|fetched| now.saturating_sub(fetched) < ttl_ms)
    }
}
