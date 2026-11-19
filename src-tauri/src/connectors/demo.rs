//! Sample data, to try the app (and take screenshots) without connecting any account.

use super::Connector;
use crate::items::{Kind, NewItem, now};
use async_trait::async_trait;
use serde_json::json;

pub struct Demo;

const MINUTE: i64 = 60;
const HOUR: i64 = 60 * MINUTE;

#[async_trait]
impl Connector for Demo {
    fn source(&self) -> &str {
        "demo"
    }

    async fn sync(&self) -> Result<Vec<NewItem>, String> {
        let now = now();
        // (id, kind, title, body, author, state, seconds from now, duration)
        let samples = [
            (
                "mr-482",
                Kind::MergeRequest,
                "Add retry with backoff to the sync worker",
                "Review requested. CI is green, 3 files changed.",
                "Noor Haddad",
                "review requested",
                -25 * MINUTE,
                None,
            ),
            (
                "mail-1",
                Kind::Mail,
                "Invoice #2041 for November",
                "Your invoice is attached. Payment is due within 30 days.",
                "Billing at Northwind",
                "unread",
                -50 * MINUTE,
                None,
            ),
            (
                "event-1",
                Kind::Event,
                "Weekly sync",
                "Agenda: release checklist, open incidents, planning.",
                "Team calendar",
                "confirmed",
                2 * HOUR,
                Some(30 * MINUTE),
            ),
            (
                "ticket-77",
                Kind::Ticket,
                "Search returns stale results after an edit",
                "Steps to reproduce are in the first comment. Affects the desktop app only.",
                "Imani Okafor",
                "in progress",
                -3 * HOUR,
                None,
            ),
            (
                "alert-1",
                Kind::Alert,
                "api-eu-2 is not responding",
                "No answer on port 443 for the last 4 minutes.",
                "Servers",
                "down",
                -4 * MINUTE,
                None,
            ),
            (
                "mail-2",
                Kind::Mail,
                "Re: API rate limits for the partner integration",
                "Thanks for the details. Could we raise the limit to 600 requests per minute?",
                "Tomás Rivera",
                "unread",
                -5 * HOUR,
                None,
            ),
            (
                "notif-1",
                Kind::Notification,
                "You were mentioned in \"Drop Node 20 support\"",
                "Can you confirm the desktop build no longer needs it?",
                "Kenji Watanabe",
                "unread",
                -7 * HOUR,
                None,
            ),
            (
                "task-1",
                Kind::Task,
                "Write the release notes for 0.4",
                "",
                "",
                "todo",
                -20 * HOUR,
                None,
            ),
            (
                "article-1",
                Kind::Article,
                "SQLite's full-text search, explained with examples",
                "How tokenizers, prefix queries and ranking work together.",
                "Weekly reading",
                "unread",
                -26 * HOUR,
                None,
            ),
        ];
        Ok(samples
            .into_iter()
            .map(|(id, kind, title, body, author, state, offset, duration)| {
                let occurred_at = now + offset;
                NewItem {
                    external_id: id.into(),
                    kind,
                    title: title.into(),
                    body: body.into(),
                    author: (!author.is_empty()).then(|| author.to_owned()),
                    url: None,
                    state: state.into(),
                    occurred_at,
                    ends_at: duration.map(|d: i64| occurred_at + d),
                    // Trackers say how far along a ticket is; the Tasks space groups by it.
                    meta: match kind {
                        Kind::Ticket => json!({ "identifier": "APP-77", "stateType": "started" }),
                        _ => json!({}),
                    },
                }
            })
            .collect())
    }
}
