//! Ordered and unordered ActivityPub collections.

use serde_json::Value;

use crate::Error;
use crate::actor::{link, string, total_items};

/// A collection's header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collection {
    /// `OrderedCollection` or `Collection`.
    pub id: String,
    /// Number of members when the origin publishes it.
    pub total_items: u64,
    /// First page URL.
    pub first: Option<String>,
}

impl Collection {
    /// Parse a collection header.
    ///
    /// # Errors
    /// [`Error::Object`] when there is no id.
    pub fn from_json(value: &Value) -> Result<Self, Error> {
        let id =
            string(value.get("id")).ok_or_else(|| Error::Object("collection has no id".into()))?;
        Ok(Self {
            id,
            total_items: total_items(Some(value)),
            first: link(value.get("first")),
        })
    }
}

/// One page of a collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionPage {
    /// Raw members. Callers know whether they are actors, notes or URLs.
    pub items: Vec<Value>,
    /// Next page URL, when the origin publishes one.
    pub next: Option<String>,
    /// Total across the whole collection, when present.
    pub total_items: u64,
}

impl CollectionPage {
    /// Parse a collection page, or a collection that carries items inline.
    ///
    /// # Errors
    /// [`Error::Object`] when neither `orderedItems` nor `items` is present.
    pub fn from_json(value: &Value) -> Result<Self, Error> {
        let items = value
            .get("orderedItems")
            .or_else(|| value.get("items"))
            .and_then(Value::as_array)
            .ok_or_else(|| Error::Object("collection page has no items".into()))?
            .clone();
        Ok(Self {
            items,
            next: link(value.get("next")),
            total_items: total_items(Some(value)),
        })
    }
}
