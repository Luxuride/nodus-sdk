//! Scheduled posts over the REST transport.

use crate::actions::ScheduleActions;
use crate::model::{Draft, ScheduledStatus};
use crate::rest::{RestInstance, Transport};
use crate::{Error, Result};

impl<T: Transport> ScheduleActions for RestInstance<T> {
    type Error = Error;

    async fn scheduled(&self) -> Result<Vec<ScheduledStatus>> {
        let url = self.url("/api/v1/scheduled_statuses")?;
        self.fetch(self.get(url)).await
    }

    async fn schedule(&self, draft: &Draft, at: &str) -> Result<ScheduledStatus> {
        let url = self.url("/api/v1/statuses")?;
        let mut form = vec![
            ("status", draft.text.clone()),
            ("visibility", draft.visibility.as_str().to_owned()),
        ];
        if let Some(parent) = draft.in_reply_to_id.as_ref() {
            form.push(("in_reply_to_id", parent.clone()));
        }
        for media in &draft.media_ids {
            form.push(("media_ids[]", media.clone()));
        }
        if !draft.spoiler_text.is_empty() {
            form.push(("spoiler_text", draft.spoiler_text.clone()));
        }
        if draft.sensitive {
            form.push(("sensitive", "true".to_owned()));
        }
        form.push(("scheduled_at", at.to_owned()));
        self.fetch(self.post_form(url, &form)).await
    }

    async fn cancel_scheduled(&self, id: &str) -> Result<()> {
        let url = self.url(&format!("/api/v1/scheduled_statuses/{id}"))?;
        // The answer is an empty body; decoding it as an object would make a successful
        // cancellation look like a failure.
        let _: serde_json::Value = self.fetch(self.delete(url)).await?;
        Ok(())
    }
}
