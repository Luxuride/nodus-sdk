//! Filters over the REST transport.

use crate::actions::FilterActions;
use crate::model::{Filter, FilterDraft};
use crate::rest::{RestInstance, Transport};
use crate::{Error, Result};

impl<T: Transport> FilterActions for RestInstance<T> {
    type Error = Error;

    async fn filters(&self) -> Result<Vec<Filter>> {
        let url = self.url("/api/v2/filters")?;
        self.fetch(self.get(url)).await
    }

    async fn create_filter(&self, draft: &FilterDraft) -> Result<Filter> {
        let url = self.url("/api/v2/filters")?;
        let form = draft_form(draft);
        self.fetch(self.post_form(url, &form)).await
    }

    async fn update_filter(&self, id: &str, draft: &FilterDraft) -> Result<Filter> {
        let url = self.url(&format!("/api/v2/filters/{id}"))?;
        let form = draft_form(draft);
        self.fetch(self.put_form(url, &form)).await
    }

    async fn delete_filter(&self, id: &str) -> Result<()> {
        let url = self.url(&format!("/api/v2/filters/{id}"))?;
        // The body of a delete is empty, so it cannot be decoded as the deleted object.
        let _: serde_json::Value = self.fetch(self.delete(url)).await?;
        Ok(())
    }
}

/// The form body the filter endpoints expect.
///
/// Contexts and phrases repeat their key rather than joining on a comma, because that is
/// what the API parses; a phrase containing a comma would otherwise split in two.
fn draft_form(draft: &FilterDraft) -> Vec<(&'static str, String)> {
    let mut form = vec![
        ("title", draft.title.clone()),
        ("filter_action", draft.filter_action.as_str().to_owned()),
        ("whole_word", draft.whole_word.to_string()),
    ];
    for context in &draft.context {
        form.push(("context[]", context.as_str().to_owned()));
    }
    if let Some(expires_in) = &draft.expires_in {
        form.push(("expires_in", expires_in.clone()));
    }
    for keyword in &draft.keywords {
        form.push(("keywords_attributes[][keyword]", keyword.clone()));
    }
    form
}
