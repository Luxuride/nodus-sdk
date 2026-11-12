//! Public reads over HTTPS. No signing, no delivery.

use serde_json::Value;
use url::Url;

use crate::actor::link;
use crate::{Actor, Collection, CollectionPage, Error, Note};

/// A read-only ActivityPub client.
#[derive(Debug, Clone)]
pub struct ActivityPub {
    client: reqwest::Client,
}

impl Default for ActivityPub {
    fn default() -> Self {
        Self::new()
    }
}

impl ActivityPub {
    /// A client with a short timeout and an ActivityPub accept header.
    #[must_use]
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent("nodus-activitypub/0.1")
            .build()
            .unwrap_or_default();
        Self { client }
    }

    async fn get(&self, url: &str) -> Result<Value, Error> {
        check(url)?;
        let response = self
            .client
            .get(url)
            .header("accept", "application/activity+json, application/ld+json")
            .send()
            .await
            .map_err(|error| Error::Transport(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(Error::Status {
                status: status.as_u16(),
                url: url.to_owned(),
            });
        }
        response
            .json()
            .await
            .map_err(|error| Error::Transport(error.to_string()))
    }

    /// Resolve `user@host` to its actor URL.
    ///
    /// # Errors
    /// [`Error::Handle`] for a malformed handle, [`Error::NoActor`] when WebFinger has no
    /// ActivityPub link.
    pub async fn webfinger(&self, acct: &str) -> Result<String, Error> {
        let (user, host) = acct
            .split_once('@')
            .filter(|(user, host)| !user.is_empty() && !host.is_empty())
            .ok_or_else(|| Error::Handle(acct.to_owned()))?;
        let resource = format!("acct:{user}@{host}");
        let url = format!(
            "https://{host}/.well-known/webfinger?resource={}",
            urlencoding::encode(&resource)
        );
        let value = self.get(&url).await?;
        value
            .get("links")
            .and_then(Value::as_array)
            .and_then(|links| {
                links.iter().find(|link| {
                    link.get("rel").and_then(Value::as_str) == Some("self")
                        && link
                            .get("type")
                            .and_then(Value::as_str)
                            .is_some_and(|kind| kind.contains("activity"))
                })
            })
            .and_then(|link| link.get("href"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| Error::NoActor(acct.to_owned()))
    }

    /// Read an actor by handle.
    ///
    /// # Errors
    /// Propagates WebFinger and transport errors.
    pub async fn actor(&self, acct: &str) -> Result<Actor, Error> {
        let url = self.webfinger(acct).await?;
        self.actor_url(&url).await
    }

    /// Read an actor by actor URL.
    ///
    /// # Errors
    /// Propagates transport and object errors.
    pub async fn actor_url(&self, url: &str) -> Result<Actor, Error> {
        Actor::from_json(&self.get(url).await?)
    }

    /// Read a collection header.
    ///
    /// # Errors
    /// Propagates transport and object errors.
    pub async fn collection(&self, url: &str) -> Result<Collection, Error> {
        Collection::from_json(&self.get(url).await?)
    }

    /// Read a collection page.
    ///
    /// # Errors
    /// Propagates transport and object errors.
    pub async fn page(&self, url: &str) -> Result<CollectionPage, Error> {
        CollectionPage::from_json(&self.get(url).await?)
    }

    /// Read the first page of an actor's outbox.
    ///
    /// # Errors
    /// [`Error::Object`] when the actor publishes no outbox.
    pub async fn outbox(&self, actor: &Actor) -> Result<CollectionPage, Error> {
        let url = actor
            .outbox
            .as_deref()
            .ok_or_else(|| Error::Object("actor has no outbox".to_owned()))?;
        self.first_page(url).await
    }

    /// Read the first page of a collection, following `first` when needed.
    ///
    /// # Errors
    /// Propagates transport and object errors.
    pub async fn first_page(&self, url: &str) -> Result<CollectionPage, Error> {
        let value = self.get(url).await?;
        if value.get("orderedItems").is_some() || value.get("items").is_some() {
            return CollectionPage::from_json(&value);
        }
        let collection = Collection::from_json(&value)?;
        match collection.first {
            Some(first) => self.page(&first).await,
            None => Ok(CollectionPage {
                items: Vec::new(),
                next: None,
                total_items: collection.total_items,
            }),
        }
    }

    /// Read a note by URL.
    ///
    /// # Errors
    /// Propagates transport and object errors.
    pub async fn note(&self, url: &str) -> Result<Note, Error> {
        Note::from_json(&self.get(url).await?)
    }

    /// Read the reply collection of a note, one page.
    ///
    /// # Errors
    /// [`Error::Object`] when the note has no replies collection.
    pub async fn replies(&self, note: &Note) -> Result<CollectionPage, Error> {
        let url = note
            .replies
            .as_deref()
            .ok_or_else(|| Error::Object("note has no replies collection".to_owned()))?;
        self.first_page(url).await
    }
}

fn check(url: &str) -> Result<(), Error> {
    let parsed = Url::parse(url).map_err(|_| Error::Url(url.to_owned()))?;
    if parsed.scheme() != "https" || parsed.host_str().is_none() {
        return Err(Error::Url(url.to_owned()));
    }
    Ok(())
}

/// The URL a note object points at, when callers only have a page.
#[must_use]
pub fn note_url(value: &Value) -> Option<String> {
    link(value.get("url")).or_else(|| link(value.get("id")))
}
