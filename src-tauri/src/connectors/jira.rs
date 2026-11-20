//! Jira Cloud, as a tracker: the issues assigned to the user that are not done.

use super::{Connector, REQUEST_LIMIT, excerpt, parse_day, unreachable};
use crate::{
    http::client,
    items::{Kind, NewItem},
};
use async_trait::async_trait;
use chrono::DateTime;
use serde::Deserialize;
use serde_json::{Value, json};

/// What is asked for. Jira only searches within a restriction: here, the user's own issues.
const JQL: &str = "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC";
/// Without this list, the search only returns the ids.
const FIELDS: &str = "summary,description,status,priority,project,reporter,updated,duedate";

/// Where the site is, and who signs in.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Options {
    /// Such as `https://your-site.atlassian.net`.
    pub url: String,
    /// The account the API token belongs to.
    pub email: String,
}

pub struct Jira {
    source: String,
    /// Address of the site, without a trailing slash. `None` when what was typed is not one.
    base: Option<String>,
    email: String,
    token: Option<String>,
}

impl Jira {
    pub fn new(source: String, options: Options, token: Option<String>) -> Self {
        let url = options.url.trim().trim_end_matches('/');
        Self {
            source,
            // The token travels with every request: never in the clear.
            base: url.starts_with("https://").then(|| url.to_owned()),
            email: options.email.trim().to_owned(),
            token,
        }
    }
}

#[async_trait]
impl Connector for Jira {
    fn source(&self) -> &str {
        &self.source
    }

    async fn sync(&self) -> Result<Vec<NewItem>, String> {
        let base = self.base.as_deref().ok_or(
            "Give the address of your Jira site, such as https://your-site.atlassian.net.",
        )?;
        let token = self
            .token
            .as_deref()
            .ok_or("Add the API token of your account.")?;
        let address = reqwest::Url::parse_with_params(
            &format!("{base}/rest/api/2/search/jql"),
            [("jql", JQL), ("fields", FIELDS), ("maxResults", "50")],
        )
        .map_err(|error| error.to_string())?;
        let response = client()
            .get(address)
            .timeout(REQUEST_LIMIT)
            .basic_auth(&self.email, Some(token))
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(unreachable)?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err("Jira refused this email address and API token.".into());
        }
        let answer: Value = response.json().await.unwrap_or_default();
        if !status.is_success() {
            return Err(match answer["errorMessages"][0].as_str() {
                Some(message) => format!("Jira: {message} ({status})"),
                None => format!("Jira answered {status}"),
            });
        }
        Ok(tickets(base, &answer))
    }
}

/// Unix seconds of a Jira timestamp such as `2026-11-19T09:30:00.000+0100`: its offset has
/// no colon, unlike the timestamps of the other services.
fn timestamp(text: &str) -> Option<i64> {
    let moment = DateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f%z").ok()?;
    Some(moment.timestamp())
}

/// 1 urgent to 4 low, like the other trackers. Jira lets each site rename its priorities:
/// the usual names are read, any other gives no priority.
fn priority(name: &str) -> Option<u8> {
    match name.to_lowercase().as_str() {
        "highest" | "blocker" | "critical" => Some(1),
        "high" | "major" => Some(2),
        "medium" => Some(3),
        "low" | "lowest" | "minor" | "trivial" => Some(4),
        _ => None,
    }
}

fn tickets(base: &str, answer: &Value) -> Vec<NewItem> {
    let issues = answer["issues"].as_array().into_iter().flatten();
    issues
        .filter_map(|issue| {
            let key = issue["key"].as_str()?;
            let fields = &issue["fields"];
            // "indeterminate" is Jira's word for work that has started.
            let started = fields["status"]["statusCategory"]["key"] == "indeterminate";
            Some(NewItem {
                external_id: issue["id"].as_str()?.into(),
                kind: Kind::Ticket,
                title: fields["summary"].as_str().unwrap_or_default().into(),
                // A plain text here; a site that answers with a structured document gives none.
                body: excerpt(fields["description"].as_str().unwrap_or_default(), 500),
                author: fields["reporter"]["displayName"]
                    .as_str()
                    .map(str::to_owned),
                url: Some(format!("{base}/browse/{key}")),
                state: fields["status"]["name"].as_str().unwrap_or_default().into(),
                occurred_at: fields["updated"].as_str().and_then(timestamp).unwrap_or(0),
                ends_at: fields["duedate"].as_str().and_then(parse_day),
                meta: json!({
                    "identifier": key,
                    "stateType": if started { "started" } else { "unstarted" },
                    "priority": fields["priority"]["name"].as_str().and_then(priority),
                    "project": fields["project"]["name"],
                }),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::parse_time;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{basic_auth, method, path, query_param},
    };

    const EMAIL: &str = "me@example.com";

    fn answer() -> Value {
        json!({ "isLast": true, "issues": [
            { "id": "10001", "key": "APP-12", "fields": {
                "summary": "Search returns stale results",
                "description": "Steps to reproduce:\n1. Edit a note.",
                "status": { "name": "In Review", "statusCategory": { "key": "indeterminate" } },
                "priority": { "name": "High" },
                "project": { "key": "APP", "name": "Desktop app" },
                "reporter": { "displayName": "Imani Okafor" },
                "updated": "2026-11-19T09:30:00.000+0100",
                "duedate": "2026-11-26",
            } },
            { "id": "10002", "key": "APP-13", "fields": {
                "summary": "No description, no priority",
                "description": { "type": "doc", "content": [] },
                "status": { "name": "To Do", "statusCategory": { "key": "new" } },
                "priority": null, "project": { "name": "Desktop app" }, "reporter": null,
                "updated": "2026-11-18T08:00:00.000+0000", "duedate": null,
            } },
        ] })
    }

    #[test]
    fn translates_issues() {
        let items = tickets("https://acme.atlassian.net", &answer());
        assert_eq!(items.len(), 2);
        let first = &items[0];
        assert_eq!(
            (first.kind, first.external_id.as_str()),
            (Kind::Ticket, "10001")
        );
        assert_eq!(first.title, "Search returns stale results");
        assert_eq!(first.body, "Steps to reproduce:\n1. Edit a note.");
        assert_eq!(
            first.url.as_deref(),
            Some("https://acme.atlassian.net/browse/APP-12")
        );
        assert_eq!(
            (first.state.as_str(), first.author.as_deref()),
            ("In Review", Some("Imani Okafor"))
        );
        // 09:30 at +01:00 is 08:30 in UTC.
        assert_eq!(
            first.occurred_at,
            parse_time("2026-11-19T08:30:00Z").unwrap()
        );
        assert_eq!(first.ends_at, parse_day("2026-11-26"));
        assert_eq!(
            first.meta,
            json!({ "identifier": "APP-12", "stateType": "started", "priority": 2, "project": "Desktop app" })
        );
        let second = &items[1];
        assert_eq!(
            (
                second.body.as_str(),
                second.ends_at,
                second.author.as_deref()
            ),
            ("", None, None)
        );
        assert_eq!(second.meta["stateType"], "unstarted");
        assert_eq!(second.meta["priority"], Value::Null);
    }

    #[test]
    fn the_site_must_be_given_in_https() {
        let base = |url: &str| {
            let options = Options {
                url: url.into(),
                email: String::new(),
            };
            Jira::new("jira".into(), options, None).base
        };
        assert_eq!(
            base(" https://acme.atlassian.net/ ").as_deref(),
            Some("https://acme.atlassian.net")
        );
        assert_eq!(base(""), None);
        assert_eq!(base("http://jira.example.com"), None);
    }

    #[tokio::test]
    async fn signs_in_with_the_email_and_the_token_and_reports_what_goes_wrong() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/2/search/jql"))
            .and(query_param("jql", JQL))
            .and(query_param("fields", FIELDS))
            .and(basic_auth(EMAIL, "the-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(answer()))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(basic_auth(EMAIL, "wrong"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(basic_auth(EMAIL, "limited"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_json(json!({ "errorMessages": ["The query is not valid."] })),
            )
            .mount(&server)
            .await;
        let jira = |token: Option<&str>| Jira {
            source: "jira".into(),
            base: Some(server.uri()),
            email: EMAIL.into(),
            token: token.map(str::to_owned),
        };
        assert_eq!(jira(Some("the-token")).sync().await.unwrap().len(), 2);
        let refused = jira(Some("wrong")).sync().await.unwrap_err();
        assert_eq!(refused, "Jira refused this email address and API token.");
        let invalid = jira(Some("limited")).sync().await.unwrap_err();
        assert_eq!(invalid, "Jira: The query is not valid. (400 Bad Request)");
        assert_eq!(
            jira(None).sync().await.unwrap_err(),
            "Add the API token of your account."
        );
    }
}
