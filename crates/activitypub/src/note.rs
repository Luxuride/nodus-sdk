//! A public note, reduced to what a card draws.

use serde_json::Value;

use crate::Error;
use crate::actor::{Emoji, emojis, link, string, total_items};

/// A note attachment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    /// Object type, e.g. `Image`, `Video`, `Document`.
    pub kind: String,
    /// Full media URL.
    pub url: String,
    /// Preview image, when the origin gives one.
    pub preview_url: Option<String>,
    /// Alt text.
    pub description: Option<String>,
    /// Pixel width, when known.
    pub width: Option<u32>,
    /// Pixel height, when known.
    pub height: Option<u32>,
}

/// A public note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// Canonical object id.
    pub id: String,
    /// Human page URL.
    pub url: String,
    /// HTML body.
    pub content: String,
    /// RFC 3339 publication time.
    pub published: String,
    /// Whether the body or media needs a click.
    pub sensitive: bool,
    /// Content warning / subject.
    pub summary: String,
    /// Media, in display order.
    pub attachments: Vec<Attachment>,
    /// Custom emoji in the body.
    pub emojis: Vec<Emoji>,
    /// Parent note id, when this is a reply.
    pub in_reply_to: Option<String>,
    /// Replies collection URL.
    pub replies: Option<String>,
    /// Reply count when the origin publishes it.
    pub replies_count: u64,
}

impl Note {
    /// Parse a `Note`, or the `object` of a `Create`.
    ///
    /// # Errors
    /// [`Error::Object`] when there is no id.
    pub fn from_json(value: &Value) -> Result<Self, Error> {
        let object = if value.get("type").and_then(Value::as_str) == Some("Create") {
            value
                .get("object")
                .ok_or_else(|| Error::Object("Create has no object".into()))?
        } else {
            value
        };
        let id = string(object.get("id")).ok_or_else(|| Error::Object("note has no id".into()))?;
        let url = link(object.get("url")).unwrap_or_else(|| id.clone());
        Ok(Self {
            id,
            url,
            content: string(object.get("content")).unwrap_or_default(),
            published: string(object.get("published")).unwrap_or_default(),
            sensitive: object
                .get("sensitive")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            summary: string(object.get("summary")).unwrap_or_default(),
            attachments: attachments(object.get("attachment")),
            emojis: emojis(object.get("tag")),
            in_reply_to: link(object.get("inReplyTo")),
            replies: link(object.get("replies")),
            replies_count: total_items(object.get("replies")),
        })
    }

    /// The content warning, if any.
    #[must_use]
    pub fn warning(&self) -> Option<&str> {
        (!self.summary.is_empty()).then_some(self.summary.as_str())
    }
}

fn attachments(value: Option<&Value>) -> Vec<Attachment> {
    let Some(Value::Array(items)) = value else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let url = link(item.get("url"))?;
            Some(Attachment {
                kind: string(item.get("type")).unwrap_or_else(|| "Document".to_owned()),
                url: url.clone(),
                preview_url: item.get("icon").and_then(|icon| link(Some(icon))),
                description: string(item.get("name")),
                width: item
                    .get("width")
                    .and_then(Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok()),
                height: item
                    .get("height")
                    .and_then(Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok()),
            })
        })
        .collect()
}
