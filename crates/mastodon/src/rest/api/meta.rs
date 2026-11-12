//! What the server says about itself, over the REST transport.

use crate::actions::InstanceMeta;
use crate::model::{CustomEmoji, InstanceInfo, Language};
use crate::rest::{RestInstance, Transport};
use crate::{Error, Result};

impl<T: Transport> InstanceMeta for RestInstance<T> {
    type Error = Error;

    async fn instance_info(&self) -> Result<InstanceInfo> {
        let url = self.url("/api/v2/instance")?;
        self.fetch(self.get(url)).await
    }

    async fn custom_emojis(&self) -> Result<Vec<CustomEmoji>> {
        let url = self.url("/api/v1/custom_emojis")?;
        self.fetch(self.get(url)).await
    }

    async fn languages(&self) -> Result<Vec<Language>> {
        let url = self.url("/api/v1/instance/languages")?;
        let languages: Vec<Language> = self.fetch(self.get(url)).await?;
        // Unnamed entries are useless in a picker, and an instance that reports an empty
        // name is better served by its code than by a blank row.
        Ok(languages
            .into_iter()
            .map(|language| Language {
                name: if language.name.is_empty() {
                    language.code.clone()
                } else {
                    language.name
                },
                ..language
            })
            .collect())
    }
}
