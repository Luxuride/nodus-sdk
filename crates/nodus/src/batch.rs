//! Several requests in one round trip.
//!
//! The cost we are removing is not the bytes, it is the round trips: a timeline that
//! needs the statuses, their authors, the favourites and the relationship of each one
//! pays a round trip per shape, and on a mobile connection that is the whole latency
//! budget. A batch is the same reads under one request, answered positionally so a
//! client cannot mismatch answers and questions.
//!
//! The server is expected to run the calls against the same authenticated account, in
//! order, and to return one result per call even when some fail — a batch is not
//! atomic, and pretending otherwise would make one bad cursor discard nine good reads.

use serde::{Deserialize, Serialize};

/// The verbs a batch may contain.
///
/// Deliberately two read verbs and one write: a batch that could post statuses is a
/// batch that can half-succeed in ways the user cannot see.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verb {
    /// Read.
    Get,
    /// Write, with a JSON body.
    Post,
    /// Write, with no body.
    Delete,
}

impl Verb {
    /// The wire spelling, which is the HTTP verb itself.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Delete => "DELETE",
        }
    }
}

/// One call inside a batch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchCall {
    /// The verb.
    pub verb: Verb,
    /// An absolute path on the instance, e.g. `/api/v1/statuses/01J…`.
    pub path: String,
    /// A JSON body for `Post`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
}

impl BatchCall {
    /// A read.
    #[must_use]
    pub fn get(path: impl Into<String>) -> Self {
        Self {
            verb: Verb::Get,
            path: path.into(),
            body: None,
        }
    }

    /// A write with a body.
    #[must_use]
    pub fn post(path: impl Into<String>, body: serde_json::Value) -> Self {
        Self {
            verb: Verb::Post,
            path: path.into(),
            body: Some(body),
        }
    }

    /// A write with no body.
    #[must_use]
    pub fn delete(path: impl Into<String>) -> Self {
        Self {
            verb: Verb::Delete,
            path: path.into(),
            body: None,
        }
    }
}

/// One answer, in the order of the [`BatchCall`] that produced it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchResult {
    /// The status the single call would have answered with.
    pub status: u16,
    /// The body, or `null` when the call had none.
    pub body: serde_json::Value,
}

impl BatchResult {
    /// Whether this call succeeded.
    #[must_use]
    pub const fn is_ok(&self) -> bool {
        self.status >= 200 && self.status < 300
    }
}

/// The answer to a batch.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BatchResponse {
    /// One entry per call, in the same order.
    pub results: Vec<BatchResult>,
}

impl BatchResponse {
    /// The answer at `index`, if the server returned one.
    ///
    /// Returns `Option` rather than indexing because a shorter answer is a server bug:
    /// the caller must be able to treat it as "no data" instead of panicking in a
    /// view that is already on screen.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&BatchResult> {
        self.results.get(index)
    }

    /// How many calls failed.
    #[must_use]
    pub fn failures(&self) -> usize {
        self.results.iter().filter(|result| !result.is_ok()).count()
    }

    /// Whether every call succeeded.
    #[must_use]
    pub fn all_ok(&self) -> bool {
        self.results.iter().all(BatchResult::is_ok)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(path: &str) -> BatchCall {
        BatchCall::get(path)
    }

    #[test]
    fn calls_carry_the_verb_the_builder_names() {
        assert_eq!(call("/a").verb, Verb::Get);
        assert_eq!(BatchCall::delete("/b").verb, Verb::Delete);
        assert_eq!(
            BatchCall::post("/c", serde_json::json!({ "x": 1 })).body,
            Some(serde_json::json!({ "x": 1 }))
        );
    }

    #[test]
    fn a_read_serialises_without_a_body_field() {
        let json = serde_json::to_value(call("/a")).expect("json");
        assert_eq!(json, serde_json::json!({ "verb": "get", "path": "/a" }));
    }

    #[test]
    fn a_short_answer_is_readable_as_missing_rather_than_panicking() {
        let response = BatchResponse {
            results: vec![BatchResult {
                status: 200,
                body: serde_json::json!([]),
            }],
        };
        assert!(response.get(0).is_some());
        assert!(response.get(1).is_none());
    }

    #[test]
    fn failures_are_counted_per_call() {
        let response = BatchResponse {
            results: vec![
                BatchResult {
                    status: 200,
                    body: serde_json::Value::Null,
                },
                BatchResult {
                    status: 404,
                    body: serde_json::json!({ "error": "Record not found" }),
                },
            ],
        };
        assert_eq!(response.failures(), 1);
        assert!(!response.all_ok());
    }

    #[test]
    fn a_batch_round_trips() {
        let response = BatchResponse {
            results: vec![BatchResult {
                status: 200,
                body: serde_json::json!({ "id": "01J" }),
            }],
        };
        let json = serde_json::to_string(&response).expect("json");
        let back: BatchResponse = serde_json::from_str(&json).expect("parse");
        assert_eq!(back, response);
    }
}
