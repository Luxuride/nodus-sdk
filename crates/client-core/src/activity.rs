//! Conversion from canonical `ActivityPub` objects into the shared client model.
//!
//! The `ActivityPub` SDK owns the protocol shapes; this module is the one place
//! that turns them into what every client renders. A Mastodon or Misskey
//! adapter only has to produce the same [`Actor`]/[`Post`] values.

use crate::model::{Actor, Emoji, Media, MediaKind, Post, Visibility};

fn kind(value: &str) -> MediaKind {
    match value.to_ascii_lowercase().as_str() {
        "image" => MediaKind::Image,
        "video" => MediaKind::Video,
        "gifv" | "gif" => MediaKind::Gifv,
        "audio" => MediaKind::Audio,
        _ => MediaKind::Unknown,
    }
}

/// Build the shared actor model from an `ActivityPub` actor.
#[must_use]
pub fn actor(actor: &nodus_activitypub_sdk::actor::Actor, host: &str) -> Actor {
    Actor {
        id: actor.id.clone(),
        acct: actor.acct(host),
        display_name: actor.name.clone(),
        note: actor.summary.clone(),
        avatar: actor.avatar.clone().unwrap_or_default(),
        header: actor.header.clone().unwrap_or_default(),
        bot: actor.kind.eq_ignore_ascii_case("Service")
            || actor.kind.eq_ignore_ascii_case("Application"),
        locked: false,
        followers_count: 0,
        following_count: 0,
        statuses_count: 0,
        last_status_at: None,
        discoverable: None,
        indexable: None,
        noindex: None,
    }
}

/// Build the shared post model from a canonical note and its author.
#[must_use]
pub fn post(note: &nodus_activitypub_sdk::note::Note, actor: &Actor) -> Post {
    Post {
        id: note.id.clone(),
        created_at: note.published.clone(),
        content: note.content.clone(),
        visibility: Visibility::Public,
        account: actor.clone(),
        favourites_count: 0,
        reblogs_count: 0,
        replies_count: note.replies_count,
        favourited: false,
        reblogged: false,
        bookmarked: false,
        reblog: None,
        in_reply_to_id: note.in_reply_to.clone(),
        emojis: note
            .emojis
            .iter()
            .map(|emoji| Emoji {
                shortcode: emoji.shortcode.clone(),
                url: emoji.url.clone(),
                static_url: None,
            })
            .collect(),
        url: Some(note.url.clone()),
        sensitive: note.sensitive,
        spoiler_text: note.summary.clone(),
        media_attachments: note
            .attachments
            .iter()
            .map(|media| Media {
                id: media.url.clone(),
                kind: kind(&media.kind),
                url: Some(media.url.clone()),
                preview_url: media.preview_url.clone(),
                description: media.description.clone(),
                blurhash: None,
                meta_width: media.width,
                meta_height: media.height,
            })
            .collect(),
        mentions: Vec::new(),
        tags: Vec::new(),
        poll: None,
        edited_at: None,
        pinned: None,
    }
}
