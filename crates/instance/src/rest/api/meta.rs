//! What the server says about itself, over the REST transport.

use crate::actions::InstanceMeta;
use crate::model::{CustomEmoji, InstanceInfo};
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
}
