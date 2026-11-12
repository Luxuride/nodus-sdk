//! Featured tags and instance rules over the REST transport.

use crate::actions::HighlightActions;
use crate::model::{FeaturedTag, LocalId, Rule};
use crate::rest::{RestInstance, Transport};
use crate::{Error, Result};

impl<T: Transport> HighlightActions for RestInstance<T> {
    type Error = Error;

    async fn featured_tags(&self, id: &LocalId) -> Result<Vec<FeaturedTag>> {
        let url = self.url(&format!("/api/v1/accounts/{}/featured_tags", id.0))?;
        self.fetch(self.get(url)).await
    }

    async fn feature_tag(&self, name: &str) -> Result<FeaturedTag> {
        let url = self.url("/api/v1/featured_tags")?;
        self.fetch(self.post_form(url, &[("name", name.trim_start_matches('#').to_owned())]))
            .await
    }

    async fn unfeature_tag(&self, id: &str) -> Result<()> {
        let url = self.url(&format!("/api/v1/featured_tags/{id}"))?;
        // The answer is an empty body, so decoding it as an object would turn a
        // successful removal into a parse failure.
        let _: serde_json::Value = self.fetch(self.delete(url)).await?;
        Ok(())
    }

    async fn rules(&self) -> Result<Vec<Rule>> {
        let url = self.url("/api/v1/instance/rules")?;
        self.fetch(self.get(url)).await
    }
}
