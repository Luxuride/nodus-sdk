//! The rest of a status, and the shapes a "full client" needs beside it.
//!
//! Split from `model/mod.rs` because a status is one type and these are the things
//! that hang off it: media, mentions, tags, polls, edit history. Keeping them apart
//! keeps each file about one question, and lets this file grow as the client grows
//! without the status definition drifting out of view.

use serde::{Deserialize, Serialize};

use super::{Account, CustomEmoji, Status};

/// An account named in the content of a status.
///
/// Separate from [`Account`] on purpose: a mention carries no counts and no
/// avatar, only what a renderer needs to turn `@acct` into a link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mention {
    /// Instance-local id.
    pub id: String,
    /// Local username, without the domain.
    pub username: String,
    /// `user@host`, always fully qualified.
    pub acct: String,
    /// Profile URL.
    #[serde(default)]
    pub url: String,
}

/// A hashtag in the content of a status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    /// The tag itself, without `#`.
    pub name: String,
    /// Permalink to the tag timeline.
    #[serde(default)]
    pub url: String,
    /// Daily usage counts, when the instance reports them.
    #[serde(default)]
    pub history: Vec<TagHistory>,
}

/// One day of usage for a hashtag.
///
/// The counts arrive as strings, which is Mastodon's quirk, not ours: parsing them
/// into numbers here would make a malformed day fail the whole tag payload instead of
/// costing one entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagHistory {
    /// `YYYY-MM-DD`.
    pub day: String,
    /// Accounts that used the tag that day, as a decimal string.
    #[serde(default)]
    pub accounts: String,
    /// Posts that used it, as a decimal string.
    #[serde(default)]
    pub uses: String,
}

/// The kind of an attachment, as the wire names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    /// Still image.
    Image,
    /// Video file.
    Video,
    /// Silent looping video (Mastodon's `gifv`).
    Gifv,
    /// Audio file.
    Audio,
    /// A type this client predates.
    ///
    /// Rendered as a link rather than dropped: an unknown attachment still exists,
    /// and hiding it would misrepresent the post.
    #[serde(other)]
    Unknown,
}

/// One attachment on a status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Media {
    /// Instance-local id.
    pub id: String,
    /// What it is.
    #[serde(rename = "type")]
    pub kind: MediaKind,
    /// The file, when the instance will serve it.
    #[serde(default)]
    pub url: Option<String>,
    /// A smaller copy, for grids.
    #[serde(default)]
    pub preview_url: Option<String>,
    /// Alt text, written by the author.
    #[serde(default)]
    pub description: Option<String>,
    /// Placeholder hash, for rendering before the bytes arrive.
    #[serde(default)]
    pub blurhash: Option<String>,
    /// Width in pixels, when reported.
    #[serde(default)]
    pub meta_width: Option<u32>,
    /// Height in pixels, when reported.
    #[serde(default)]
    pub meta_height: Option<u32>,
}

/// Mastodon's name for [`Media`].
pub use Media as MediaAttachment;

/// A poll attached to a status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Poll {
    /// Poll id.
    pub id: String,
    /// When voting closes.
    #[serde(default)]
    pub expires_at: Option<String>,
    /// Whether voting has closed.
    #[serde(default)]
    pub expired: bool,
    /// Whether several options may be picked.
    #[serde(default)]
    pub multiple: bool,
    /// Total votes.
    #[serde(default)]
    pub votes_count: u64,
    /// Distinct voters, `None` when the instance hides them.
    #[serde(default)]
    pub voters_count: Option<u64>,
    /// Whether the signed-in account voted.
    #[serde(default)]
    pub voted: bool,
    /// Which options the signed-in account picked.
    #[serde(default)]
    pub own_votes: Vec<usize>,
    /// The options.
    #[serde(default)]
    pub options: Vec<PollOption>,
    /// Emoji used in the options.
    #[serde(default)]
    pub emojis: Vec<CustomEmoji>,
}

/// One option of a poll.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollOption {
    /// The label, plain text.
    pub title: String,
    /// Votes for it, `None` while the instance withholds counts.
    #[serde(default)]
    pub votes_count: Option<u64>,
}

/// One revision of a status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// The content as it read at the time.
    #[serde(default)]
    pub content: String,
    /// The content warning as it read at the time.
    #[serde(default)]
    pub spoiler_text: String,
    /// Whether it was marked sensitive then.
    #[serde(default)]
    pub sensitive: bool,
    /// When that revision was written.
    pub created_at: String,
    /// Who edited it.
    #[serde(default)]
    pub account: Option<Account>,
}

/// A list, Mastodon's way of narrowing the home timeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct List {
    /// List id.
    pub id: String,
    /// The name the owner gave it.
    pub title: String,
    /// `followed`, `list` or `none` — who may reply.
    #[serde(default)]
    pub replies_policy: String,
    /// Whether members' posts appear in the home timeline at all.
    #[serde(default)]
    pub exclusive: Option<bool>,
}

/// A direct-message thread.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conversation {
    /// Conversation id, used for reading it.
    pub id: String,
    /// Whether anything in it is unread.
    #[serde(default)]
    pub unread: bool,
    /// Everyone in the thread.
    #[serde(default)]
    pub accounts: Vec<Account>,
    /// The newest status, which is what a list row shows.
    #[serde(default)]
    pub last_status: Option<Status>,
}

/// What `/api/v2/search` answers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SearchResults {
    /// Matching accounts.
    #[serde(default)]
    pub accounts: Vec<Account>,
    /// Matching statuses.
    #[serde(default)]
    pub statuses: Vec<Status>,
    /// Matching hashtags.
    #[serde(default)]
    pub hashtags: Vec<Tag>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_attachment_kind_survives_parsing() {
        let json = serde_json::json!({ "id": "1", "type": "hologram" });
        let media: MediaAttachment = serde_json::from_value(json).expect("media");
        assert_eq!(media.kind, MediaKind::Unknown);
    }

    #[test]
    fn a_status_without_any_of_the_new_fields_still_parses() {
        let json = serde_json::json!({
            "id": "1",
            "created_at": "2024-01-01T00:00:00Z",
            "visibility": "public",
            "account": {
                "id": "9", "acct": "a@b", "display_name": "A",
                "note": "", "avatar": "", "header": "",
                "bot": false, "locked": false,
                "followers_count": 0, "following_count": 0, "statuses_count": 0
            }
        });
        let status: Status = serde_json::from_value(json).expect("status");
        assert_eq!(status.media_attachments, Vec::<MediaAttachment>::new());
        assert!(status.poll.is_none());
        assert_eq!(status.spoiler_text, "");
    }

    #[test]
    fn poll_votes_round_trip_through_the_own_votes_list() {
        let poll = Poll {
            id: "1".to_owned(),
            expires_at: None,
            expired: false,
            multiple: true,
            votes_count: 3,
            voters_count: Some(2),
            voted: true,
            own_votes: vec![0, 2],
            options: vec![
                PollOption {
                    title: "a".to_owned(),
                    votes_count: Some(1),
                },
                PollOption {
                    title: "b".to_owned(),
                    votes_count: None,
                },
            ],
            emojis: Vec::new(),
        };
        let json = serde_json::to_string(&poll).expect("json");
        let back: Poll = serde_json::from_str(&json).expect("parse");
        assert_eq!(back, poll);
    }
}

/// What an account may change about its own profile.
///
/// Every field is optional and omitted fields are left alone: the wire treats an absent
/// key as "unchanged", and a client that always sent the whole profile would overwrite a
/// bio typed in another tab.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileUpdate {
    /// The display name; an empty string is a real value, not a missing one.
    pub display_name: Option<String>,
    /// The bio, as plain text or limited HTML the instance sanitises.
    pub note: Option<String>,
    /// Whether new follows need approval.
    pub locked: Option<bool>,
    /// Whether the account is marked as automated.
    pub bot: Option<bool>,
    /// Whether the account may appear in discovery.
    pub discoverable: Option<bool>,
}

impl ProfileUpdate {
    /// An update that changes nothing, to build on.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the display name.
    #[must_use]
    pub fn display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }

    /// Sets the bio.
    #[must_use]
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// Requires approval for new follows.
    #[must_use]
    pub const fn locked(mut self, locked: bool) -> Self {
        self.locked = Some(locked);
        self
    }

    /// Whether the result says anything at all.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.display_name.is_none()
            && self.note.is_none()
            && self.locked.is_none()
            && self.bot.is_none()
            && self.discoverable.is_none()
    }
}
