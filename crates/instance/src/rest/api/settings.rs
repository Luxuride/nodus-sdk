//! Preferences and announcements over the REST transport.

use crate::actions::SettingsActions;
use crate::model::{Announcement, Preferences};
use crate::rest::{RestInstance, Transport};
use crate::{Error, Result};

impl<T: Transport> SettingsActions for RestInstance<T> {
    type Error = Error;

    async fn preferences(&self) -> Result<Preferences> {
        let url = self.url("/api/v1/preferences")?;
        self.fetch(self.get(url)).await
    }

    async fn announcements(&self) -> Result<Vec<Announcement>> {
        let url = self.url("/api/v1/announcements")?;
        self.fetch(self.get(url)).await
    }

    async fn dismiss_announcement(&self, id: &str) -> Result<()> {
        let url = self.url(&format!("/api/v1/announcements/{id}/dismiss"))?;
        // Dismissing answers 200 with an empty body; decoding it as an object would
        // make a successful dismissal look like a failure.
        let _: serde_json::Value = self.fetch(self.post(url)).await?;
        Ok(())
    }
}
