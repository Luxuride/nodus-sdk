//! An ActivityPub actor, reduced to the fields a reader can see.

use serde_json::Value;

use crate::Error;

/// An actor's avatar or header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// The image URL.
    pub url: String,
}

/// A custom emoji declared by an actor or a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Emoji {
    /// `:shortcode:`.
    pub shortcode: String,
    /// Image URL.
    pub url: String,
}

/// The public profile behind an actor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    /// Canonical actor id.
    pub id: String,
    /// ActivityPub type, usually `Person`.
    pub kind: String,
    /// Local username.
    pub username: String,
    /// Display name.
    pub name: String,
    /// Profile HTML.
    pub summary: String,
    /// Profile page URL.
    pub url: String,
    /// Avatar.
    pub avatar: Option<String>,
    /// Header image.
    pub header: Option<String>,
    /// Followers collection URL.
    pub followers: Option<String>,
    /// Following collection URL.
    pub following: Option<String>,
    /// Outbox collection URL.
    pub outbox: Option<String>,
}

impl Actor {
    /// Read the fields this crate understands.
    ///
    /// # Errors
    /// [`Error::Object`] when the JSON is not an actor with an id.
    pub fn from_json(value: &Value) -> Result<Self, Error> {
        let id = string(value.get("id")).ok_or_else(|| Error::Object("actor has no id".into()))?;
        let username = string(value.get("preferredUsername"))
            .or_else(|| value.get("name").and_then(Value::as_str).map(str::to_owned))
            .unwrap_or_default();
        let url = link(value.get("url")).unwrap_or_else(|| id.clone());
        let avatar = value.get("icon").and_then(|value| link(Some(value)));
        let header = value.get("image").and_then(|value| link(Some(value)));
        Ok(Self {
            id,
            kind: string(value.get("type")).unwrap_or_else(|| "Person".to_owned()),
            username,
            name: string(value.get("name")).unwrap_or_default(),
            summary: string(value.get("summary")).unwrap_or_default(),
            url,
            avatar,
            header,
            followers: link(value.get("followers")),
            following: link(value.get("following")),
            outbox: link(value.get("outbox")),
        })
    }

    /// The `user@host` handle, from the actor URL.
    #[must_use]
    pub fn acct(&self, host: &str) -> String {
        format!("{}@{host}", self.username)
    }
}

pub(crate) fn string(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => Some(text.clone()),
        Value::Object(map) => map.get("href").and_then(Value::as_str).map(str::to_owned),
        _ => None,
    }
}

pub(crate) fn link(value: Option<&Value>) -> Option<String> {
    let value = value?;
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => items.first().and_then(|item| link(Some(item))),
        Value::Object(map) => map
            .get("url")
            .or_else(|| map.get("href"))
            .and_then(|inner| link(Some(inner)))
            .or_else(|| map.get("id").and_then(Value::as_str).map(str::to_owned)),
        _ => None,
    }
}

pub(crate) fn total_items(value: Option<&Value>) -> u64 {
    let Some(Value::Object(map)) = value else {
        return 0;
    };
    map.get("totalItems")
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

pub(crate) fn emojis(value: Option<&Value>) -> Vec<Emoji> {
    let Some(Value::Array(items)) = value else {
        return Vec::new();
    };
    items
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("Emoji"))
        .filter_map(|item| {
            let shortcode = item.get("name").and_then(Value::as_str)?.to_owned();
            let url = item
                .get("icon")
                .and_then(|icon| link(Some(icon)))
                .or_else(|| {
                    item.get("icon")
                        .and_then(|icon| icon.get("url"))
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })?;
            Some(Emoji { shortcode, url })
        })
        .collect()
}
