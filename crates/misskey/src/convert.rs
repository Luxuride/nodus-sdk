//! The wire mapping. Misskey's shapes stay in this file.

use nodus_client_core::model::{
    Account, CredentialSource, CustomEmoji, LocalId, MediaAttachment, MediaKind, Mention,
    Notification, NotificationKind, Poll, PollOption, Relationship, Status, Tag, Visibility,
};
use serde_json::Value;

/// Map a Misskey user to the account shape the views render.
pub fn account(value: &Value) -> Option<Account> {
    let username = value.get("username")?.as_str()?;
    let host = value.get("host").and_then(Value::as_str);
    let acct = host.map_or_else(|| username.to_owned(), |host| format!("{username}@{host}"));
    let emojis: Vec<CustomEmoji> = value
        .get("emojis")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .filter_map(|(shortcode, url)| {
                    url.as_str().map(|url| CustomEmoji {
                        shortcode: shortcode.clone(),
                        url: url.to_owned(),
                        static_url: None,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Account {
        id: string(value, "id")?,
        acct,
        display_name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        note: text_html(
            value
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            &emojis,
        ),
        avatar: value
            .get("avatarUrl")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        header: value
            .get("bannerUrl")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        bot: value.get("isBot").and_then(Value::as_bool).unwrap_or(false),
        locked: value
            .get("isLocked")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        followers_count: number(value, "followersCount"),
        following_count: number(value, "followingCount"),
        statuses_count: number(value, "notesCount"),
        last_status_at: None,
        discoverable: value.get("isExplorable").and_then(Value::as_bool),
        indexable: None,
        noindex: None,
    })
}

/// Map a Misskey note to the status shape the views render.
pub fn status(value: Value) -> Option<Status> {
    let account = account(value.get("user")?)?;
    let emojis = emojis(&value);
    let files: Vec<MediaAttachment> = value
        .get("files")
        .and_then(Value::as_array)
        .map(|files| files.iter().filter_map(media).collect())
        .unwrap_or_default();
    let renote = value.get("renote").cloned().and_then(status);
    let text = value
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let cw = value.get("cw").and_then(Value::as_str).unwrap_or_default();
    let reactions = value.get("reactions").and_then(Value::as_object);
    let favourites = reactions.map_or(0, |map| map.values().filter_map(Value::as_u64).sum::<u64>());
    Some(Status {
        id: string(&value, "id")?,
        created_at: string(&value, "createdAt").unwrap_or_default(),
        content: text_html(text, &emojis),
        content_format: Some("mfm".to_owned()),
        content_source: Some(text.to_owned()),
        visibility: visibility_in(value.get("visibility").and_then(Value::as_str)),
        account,
        favourites_count: favourites,
        reblogs_count: number(&value, "renoteCount"),
        replies_count: number(&value, "repliesCount"),
        favourited: false,
        reblogged: false,
        bookmarked: false,
        reblog: renote.map(Box::new),
        in_reply_to_id: string(&value, "replyId"),
        emojis,
        url: value.get("url").and_then(Value::as_str).map(str::to_owned),
        sensitive: files.iter().any(|media| media.description.is_some()),
        spoiler_text: cw.to_owned(),
        language: None,
        media_attachments: files,
        mentions: Vec::<Mention>::new(),
        tags: tags(text),
        poll: value.get("poll").cloned().and_then(poll),
        edited_at: None,
        pinned: None,
    })
}

pub fn notification(value: Value) -> Option<Notification> {
    let account = account(value.get("user")?)?;
    let kind = match value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "follow" => NotificationKind::Follow,
        "receiveFollowRequest" => NotificationKind::FollowRequest,
        "renote" | "quote" => NotificationKind::Reblog,
        "reaction" | "like" => NotificationKind::Favourite,
        "mention" | "reply" => NotificationKind::Mention,
        "pollEnded" => NotificationKind::Poll,
        "note" => NotificationKind::Status,
        _ => NotificationKind::Unknown,
    };
    Some(Notification {
        id: string(&value, "id")?,
        kind,
        created_at: string(&value, "createdAt").unwrap_or_default(),
        account,
        status: value.get("note").cloned().and_then(status),
    })
}

pub fn relationship(value: &Value, id: &LocalId) -> Option<Relationship> {
    Some(Relationship {
        id: id.clone(),
        following: boolean(value, "isFollowing"),
        requested: boolean(value, "isFollowRequested"),
        followed_by: boolean(value, "isFollowed"),
        blocking: boolean(value, "isBlocking"),
        blocked_by: boolean(value, "isBlocked"),
        muting: boolean(value, "isMuted"),
        muting_notifications: boolean(value, "isRenoteMuted"),
        domain_blocking: false,
        endorsed: false,
    })
}

pub fn source(value: &Value) -> CredentialSource {
    let visibility = value
        .pointer("/clientData/defaultNoteVisibility")
        .and_then(Value::as_str)
        .or_else(|| value.get("defaultNoteVisibility").and_then(Value::as_str));
    CredentialSource {
        privacy: visibility.map(str::to_owned),
        sensitive: None,
        language: None,
    }
}

/// Misskey sends MFM, not HTML. Escape it, keep line breaks, and paint custom emoji.
#[must_use]
pub fn note_html(text: &str, emojis: &[CustomEmoji]) -> String {
    text_html(text, emojis)
}

fn text_html(text: &str, emojis: &[CustomEmoji]) -> String {
    let mut html = String::with_capacity(text.len() + 16);
    for token in text.split_whitespace() {
        if !html.is_empty() {
            html.push(' ');
        }
        let shortcode = token.trim_matches(':');
        if token.starts_with(':') && token.ends_with(':') {
            if let Some(emoji) = emojis.iter().find(|emoji| emoji.shortcode == shortcode) {
                html.push_str(&format!(
                    "<img class=\"emoji\" alt=\":{shortcode}:\" title=\":{shortcode}:\" src=\"{}\">",
                    escape(&emoji.url)
                ));
                continue;
            }
        }
        html.push_str(&escape(token));
    }
    html.replace('\n', "<br>")
}

fn media(value: &Value) -> Option<MediaAttachment> {
    let kind = match value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "image" => MediaKind::Image,
        "video" => MediaKind::Video,
        "audio" => MediaKind::Audio,
        "gifv" => MediaKind::Gifv,
        _ => MediaKind::Unknown,
    };
    let properties = value.get("properties");
    Some(MediaAttachment {
        id: string(value, "id")?,
        kind,
        url: value.get("url").and_then(Value::as_str).map(str::to_owned),
        preview_url: value
            .get("thumbnailUrl")
            .and_then(Value::as_str)
            .map(str::to_owned),
        description: value
            .get("comment")
            .and_then(Value::as_str)
            .map(str::to_owned),
        blurhash: None,
        meta_width: properties
            .and_then(|properties| properties.get("width"))
            .and_then(Value::as_u64)
            .map(|width| width as u32),
        meta_height: properties
            .and_then(|properties| properties.get("height"))
            .and_then(Value::as_u64)
            .map(|height| height as u32),
        sensitive: value
            .get("sensitive")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        processing_state: None,
        processing_progress: None,
    })
}

fn poll(value: Value) -> Option<Poll> {
    let choices = value.get("choices")?.as_array()?;
    let options: Vec<PollOption> = choices
        .iter()
        .filter_map(|choice| {
            Some(PollOption {
                title: choice.get("text")?.as_str()?.to_owned(),
                votes_count: choice.get("votes").and_then(Value::as_u64),
            })
        })
        .collect();
    let votes_count = options
        .iter()
        .filter_map(|option| option.votes_count)
        .sum::<u64>();
    Some(Poll {
        id: String::new(),
        expires_at: value
            .get("expiresAt")
            .and_then(Value::as_str)
            .map(str::to_owned),
        expired: value
            .get("expiresAt")
            .and_then(Value::as_str)
            .is_some_and(|at| at < "1970-01-01"),
        multiple: value
            .get("multiple")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        votes_count,
        voters_count: None,
        voted: false,
        own_votes: Vec::new(),
        options,
        emojis: Vec::new(),
    })
}

pub fn visibility_out(visibility: Visibility) -> &'static str {
    match visibility {
        Visibility::Public => "public",
        Visibility::Unlisted => "home",
        Visibility::Private => "followers",
        Visibility::Direct => "specified",
    }
}

fn visibility_in(value: Option<&str>) -> Visibility {
    match value.unwrap_or("public") {
        "home" => Visibility::Unlisted,
        "followers" => Visibility::Private,
        "specified" => Visibility::Direct,
        _ => Visibility::Public,
    }
}

fn emojis(value: &Value) -> Vec<CustomEmoji> {
    value
        .get("emojis")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .filter_map(|(shortcode, url)| {
                    url.as_str().map(|url| CustomEmoji {
                        shortcode: shortcode.clone(),
                        url: url.to_owned(),
                        static_url: None,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn tags(text: &str) -> Vec<Tag> {
    text.split_whitespace()
        .filter(|word| word.starts_with('#') && word.len() > 1)
        .map(|word| Tag {
            name: word.trim_start_matches('#').to_owned(),
            url: String::new(),
            history: Vec::new(),
        })
        .collect()
}

fn string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn number(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn boolean(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_misskey_note_becomes_a_status_with_media_and_emoji() {
        let note = serde_json::json!({
            "id": "9abc",
            "createdAt": "2026-11-12T03:00:00.000Z",
            "text": "hello :blob: world",
            "cw": null,
            "visibility": "home",
            "user": {
                "id": "u1",
                "username": "alice",
                "name": "Alice",
                "host": "misskey.example",
                "avatarUrl": "https://misskey.example/a.png",
                "followersCount": 3,
                "followingCount": 4,
                "notesCount": 5
            },
            "files": [{
                "id": "f1",
                "type": "image",
                "url": "https://misskey.example/f.png",
                "thumbnailUrl": "https://misskey.example/t.png",
                "comment": "alt",
                "properties": { "width": 800, "height": 600 }
            }],
            "emojis": { "blob": "https://misskey.example/blob.png" },
            "reactions": { "like": 2, ":blob:": 1 },
            "renoteCount": 1,
            "repliesCount": 2,
            "url": "https://misskey.example/notes/9abc"
        });
        let status = status(note).expect("status");
        assert_eq!(status.account.acct, "alice@misskey.example");
        assert_eq!(status.visibility, Visibility::Unlisted);
        assert_eq!(status.favourites_count, 3);
        assert_eq!(status.media_attachments[0].meta_width, Some(800));
        assert!(status.content.contains("<img class=\"emoji\""));
        assert!(status.content.contains(":blob:"));
    }

    #[test]
    fn a_local_misskey_user_has_a_bare_acct() {
        let user = serde_json::json!({
            "id": "u1",
            "username": "alice",
            "host": null
        });
        assert_eq!(account(&user).expect("account").acct, "alice");
    }

    #[test]
    fn misskey_visibility_names_map_back_to_mastodon_names() {
        assert_eq!(visibility_out(Visibility::Public), "public");
        assert_eq!(visibility_out(Visibility::Unlisted), "home");
        assert_eq!(visibility_out(Visibility::Private), "followers");
        assert_eq!(visibility_out(Visibility::Direct), "specified");
    }
}
