//! What a moderator sees and can do.
//!
//! These types are `admin:read`/`admin:write` territory: they carry private data — a
//! report's free-text comment, an account's email, the private note on a domain block —
//! so nothing here is used by a public screen, and every call that returns them is
//! expected to fail for a token that is not a moderator's.

use serde::{Deserialize, Serialize};

use super::{Account, Status};

/// A report, as the queue returns it.
///
/// No `Eq`: the statuses it quotes carry floats, and a report is compared only in tests.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub id: String,
    #[serde(default)]
    pub action_taken: bool,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub target_account: Option<Account>,
    #[serde(default)]
    pub statuses: Vec<Status>,
}

/// Which reports to ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReportState {
    /// Waiting for a moderator.
    #[default]
    Open,
    /// Already acted on.
    Resolved,
}

impl ReportState {
    /// The query value the API expects.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Resolved => "resolved",
        }
    }
}

/// An account as a moderator sees it: the public account plus the private fields.
///
/// The flags are separate booleans because the wire has separate booleans and each one
/// means something different to a moderator; folding them into a state enum would make
/// a payload with two flags set unrepresentable, and such payloads do occur.
#[expect(
    clippy::struct_excessive_bools,
    reason = "the wire has independent flags and a DTO must mirror it"
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdminAccount {
    pub id: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub ip: Option<String>,
    #[serde(default)]
    pub confirmed: bool,
    #[serde(default)]
    pub silenced: bool,
    #[serde(default)]
    pub suspended: bool,
    #[serde(default)]
    pub disabled: bool,
    /// Missing means the server has no approval concept at all (before Mastodon 4.0),
    /// which reads as approved — defaulting to `false` would show every account as
    /// pending on those servers.
    #[serde(default = "approved_by_default")]
    pub approved: bool,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub account: Option<Account>,
}

impl AdminAccount {
    /// What this account is, in one word, for a table cell.
    pub fn standing(&self) -> &'static str {
        if self.suspended {
            "suspended"
        } else if self.silenced {
            "silenced"
        } else if self.disabled {
            "disabled"
        } else if !self.approved {
            "pending"
        } else if self.confirmed {
            "confirmed"
        } else {
            "unconfirmed"
        }
    }
}

/// The default for a field an older server does not send.
const fn approved_by_default() -> bool {
    true
}

/// What a moderator decided about an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModerationAction {
    /// Limit the account to its followers.
    Silence,
    /// Stop the account from being seen or heard at all.
    Suspend,
    /// Refuse the account entirely.
    Disable,
    /// Undo silence, suspension or disabling.
    Enable,
    /// Accept a pending account.
    Approve,
    /// Reject a pending account.
    Reject,
}

impl ModerationAction {
    /// The action name the API expects.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Silence => "silence",
            Self::Suspend => "suspend",
            Self::Disable => "disable",
            Self::Enable => "enable",
            Self::Approve => "approve",
            Self::Reject => "reject",
        }
    }

    /// Every action, for a menu.
    pub const ALL: [Self; 6] = [
        Self::Silence,
        Self::Suspend,
        Self::Disable,
        Self::Enable,
        Self::Approve,
        Self::Reject,
    ];
}

/// How much of a domain to refuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DomainSeverity {
    /// Refuse everything, including existing posts.
    #[default]
    Suspend,
    /// Refuse the domain's public posts from timelines, keep existing ones.
    Silence,
    /// Keep the block on record without refusing anything.
    Noop,
}

impl DomainSeverity {
    /// The severity name the API expects.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Suspend => "suspend",
            Self::Silence => "silence",
            Self::Noop => "noop",
        }
    }
}

/// A blocked domain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainBlock {
    pub id: String,
    pub domain: String,
    #[serde(default)]
    pub severity: DomainSeverity,
    #[serde(default)]
    pub reject_media: bool,
    #[serde(default)]
    pub reject_reports: bool,
    #[serde(default)]
    pub obfuscate: bool,
    #[serde(default)]
    pub public_comment: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_account_without_a_domain_is_local() {
        let account: AdminAccount = serde_json::from_value(serde_json::json!({
            "id": "1",
            "username": "ada",
            "confirmed": true,
            "approved": true,
        }))
        .unwrap();
        assert_eq!(account.domain, None);
        assert_eq!(account.standing(), "confirmed");
    }

    #[test]
    fn standing_reports_the_most_severe_flag() {
        let mut account: AdminAccount = serde_json::from_value(serde_json::json!({
            "id": "1",
            "username": "ada",
        }))
        .unwrap();
        assert_eq!(account.standing(), "unconfirmed");
        account.suspended = true;
        assert_eq!(account.standing(), "suspended");
    }

    #[test]
    fn a_report_without_statuses_still_parses() {
        let report: Report = serde_json::from_value(serde_json::json!({
            "id": "9",
            "comment": "spam",
        }))
        .unwrap();
        assert_eq!(report.statuses, Vec::new());
        assert!(!report.action_taken);
    }
}

/// One line of the admin dashboard: a count per day, with the previous period beside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminMeasure {
    pub key: String,
    /// The API sends totals as strings, so they are kept that way rather than parsed.
    #[serde(default)]
    pub total: Option<String>,
    #[serde(default)]
    pub previous_total: Option<String>,
    #[serde(default)]
    pub data: Vec<MeasurePoint>,
}

/// One day of a measure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeasurePoint {
    #[serde(default)]
    pub date: String,
    /// A **string** on the wire, like every other count Mastodon sends: the dashboard
    /// answers `{"date":"2026-11-01","value":"0"}`, and a `u64` here rejects the whole
    /// payload with `invalid type: string "0", expected u64`.
    #[serde(default)]
    pub value: String,
}

/// A domain whose federation this instance allows even when it blocks federation
/// generally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainAllow {
    pub id: String,
    #[serde(default)]
    pub domain: String,
}
