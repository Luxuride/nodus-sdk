//! What a streaming socket says.
//!
//! The transport hands over the raw JSON frame Mastodon sends, and this module turns it
//! into something a state machine can act on. It belongs next to the model because the
//! shape is the instance's, not ours: a Mastodon socket and a Nodus socket must produce
//! the same events, or the SDK would need two reducers and only test one.

use serde_json::Value;

use super::{Conversation, Notification, Status};

/// One event from a streaming socket.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    /// A status was created or changed where the socket can see it.
    Update(Box<Status>),
    /// A status was edited.
    StatusUpdate(Box<Status>),
    /// A status was deleted; only its id arrives.
    Delete(String),
    /// Something happened to this account.
    Notification(Box<Notification>),
    /// A conversation changed.
    Conversation(Box<Conversation>),
    /// The reader's filters changed; every cached status may now be filtered differently.
    FiltersChanged,
    /// A frame this client does not act on.
    ///
    /// Kept rather than dropped so a socket log can say what arrived, and so a new
    /// Mastodon event type does not look like silence.
    Unknown(String),
}

impl StreamEvent {
    /// Parses one socket frame.
    ///
    /// Accepts both shapes the fediverse sends: the server-sent-event body
    /// (`{"event": "update", "payload": "…"}`, where the payload is a JSON *string*) and
    /// the WebSocket body (`{"stream": ["user"], "event": "update", "payload": {…}}`).
    /// Returns `None` for anything that is not an event — a malformed frame is a socket
    /// problem, not a state transition guess.
    #[must_use]
    pub fn parse(frame: &str) -> Option<Self> {
        let frame: Value = serde_json::from_str(frame).ok()?;
        let name = frame.get("event")?.as_str()?;
        let payload = frame.get("payload");
        match name {
            "update" => decode(payload).map(|status| Self::Update(Box::new(status))),
            "status.update" => decode(payload).map(|status| Self::StatusUpdate(Box::new(status))),
            "notification" => decode(payload).map(|note| Self::Notification(Box::new(note))),
            "conversation" => decode(payload).map(|thread| Self::Conversation(Box::new(thread))),
            "delete" => delete_id(payload).map(Self::Delete),
            "filters_changed" => Some(Self::FiltersChanged),
            other => Some(Self::Unknown(other.to_owned())),
        }
    }

    /// The status this event carries, if any.
    #[must_use]
    pub fn status(&self) -> Option<&Status> {
        match self {
            Self::Update(status) | Self::StatusUpdate(status) => Some(status),
            _ => None,
        }
    }

    /// The account this event is about, when it names one.
    ///
    /// A notification about a status belongs to the status's author, and a status event
    /// belongs to its author; both are worth upserting so a screen that renders the event
    /// has a name to show.
    #[must_use]
    pub fn account(&self) -> Option<&super::Account> {
        match self {
            Self::Update(status) | Self::StatusUpdate(status) => Some(&status.account),
            Self::Notification(note) => Some(&note.account),
            Self::Conversation(thread) => thread.accounts.first(),
            Self::Delete(_) | Self::FiltersChanged | Self::Unknown(_) => None,
        }
    }
}

/// Decodes a payload that may arrive as a JSON string or as an object.
fn decode<T: serde::de::DeserializeOwned>(payload: Option<&Value>) -> Option<T> {
    match payload? {
        Value::String(text) => serde_json::from_str(text).ok(),
        object => serde_json::from_value(object.clone()).ok(),
    }
}

/// Reads a delete payload, which is the bare id as a string.
fn delete_id(payload: Option<&Value>) -> Option<String> {
    match payload? {
        Value::String(id) => Some(id.clone()),
        // Some servers wrap it: `{"id": "1"}`.
        object => object.get("id")?.as_str().map(ToOwned::to_owned),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A status payload with only the fields the model requires.
    fn status_json(id: &str) -> String {
        serde_json::json!({
            "id": id,
            "created_at": "2026-01-01T00:00:00.000Z",
            "content": "<p>hi</p>",
            "visibility": "public",
            "account": {
                "id": "9",
                "acct": "someone",
                "display_name": "Someone",
                "note": "",
                "avatar": "https://example.test/a.png",
                "header": "https://example.test/h.png",
                "bot": false,
                "locked": false,
                "followers_count": 0,
                "following_count": 0,
                "statuses_count": 0,
                "last_status_at": null
            },
            "favourites_count": 0,
            "reblogs_count": 0,
            "replies_count": 0,
            "favourited": false,
            "reblogged": false,
            "bookmarked": false
        })
        .to_string()
    }

    #[test]
    fn a_server_sent_frame_carries_a_status_as_a_json_string() {
        let frame = serde_json::json!({
            "event": "update",
            "payload": status_json("1"),
        })
        .to_string();

        let event = StreamEvent::parse(&frame).expect("an update frame parses");
        assert_eq!(event.status().map(|status| status.id.as_str()), Some("1"));
        assert_eq!(
            event.account().map(|account| account.acct.as_str()),
            Some("someone")
        );
    }

    #[test]
    fn a_websocket_frame_carries_the_payload_as_an_object() {
        let frame = serde_json::json!({
            "stream": ["user"],
            "event": "status.update",
            "payload": serde_json::from_str::<Value>(&status_json("2")).expect("valid json"),
        })
        .to_string();

        let event = StreamEvent::parse(&frame).expect("a status.update frame parses");
        assert!(matches!(event, StreamEvent::StatusUpdate(_)));
    }

    #[test]
    fn a_delete_frame_is_just_an_id() {
        let frame = serde_json::json!({"event": "delete", "payload": "42"}).to_string();
        assert_eq!(
            StreamEvent::parse(&frame),
            Some(StreamEvent::Delete("42".to_owned()))
        );
    }

    #[test]
    fn an_unparseable_frame_is_not_an_event() {
        assert!(StreamEvent::parse("not json").is_none());
        assert!(StreamEvent::parse(r#"{"event":"update","payload":"{oops"}"#).is_none());
    }

    #[test]
    fn an_unknown_event_survives_with_its_name() {
        let frame = serde_json::json!({"event": "announcement", "payload": {}}).to_string();
        assert_eq!(
            StreamEvent::parse(&frame),
            Some(StreamEvent::Unknown("announcement".to_owned()))
        );
    }
}
