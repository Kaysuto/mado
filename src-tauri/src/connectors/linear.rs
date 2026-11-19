//! Linear, as a tracker: the open tickets assigned to the user.

use super::{Connector, REQUEST_LIMIT, excerpt, parse_day, parse_time, unreachable};
use crate::{
    http::client,
    items::{Kind, NewItem},
};
use async_trait::async_trait;
use serde_json::{Value, json};

const API: &str = "https://api.linear.app/graphql";

const QUERY: &str = r#"
query {
  viewer {
    assignedIssues(
      first: 100
      orderBy: updatedAt
      filter: { state: { type: { nin: ["completed", "canceled"] } } }
    ) {
      nodes {
        id identifier title description url priority updatedAt dueDate
        state { name type }
        project { name }
      }
    }
  }
}
"#;

pub struct Linear {
    source: String,
    url: String,
    token: Option<String>,
}

impl Linear {
    pub fn new(source: String, token: Option<String>) -> Self {
        Self {
            source,
            url: API.into(),
            token,
        }
    }
}

#[async_trait]
impl Connector for Linear {
    fn source(&self) -> &str {
        &self.source
    }

    async fn sync(&self) -> Result<Vec<NewItem>, String> {
        let token = self
            .token
            .as_deref()
            .ok_or("This source has no API key yet.")?;
        let response = client()
            .post(&self.url)
            .timeout(REQUEST_LIMIT)
            // Personal API keys go in the header as is, without "Bearer".
            .header("Authorization", token)
            .json(&json!({ "query": QUERY }))
            .send()
            .await
            .map_err(unreachable)?;
        let status = response.status();
        let answer: Value = response.json().await.unwrap_or_default();
        // Linear reports problems in `errors`, whatever the HTTP status.
        if let Some(message) = answer["errors"][0]["message"].as_str() {
            return Err(format!("Linear: {message}"));
        }
        if !status.is_success() {
            return Err(format!("Linear answered {status}"));
        }
        Ok(tickets(&answer["data"]))
    }
}

fn tickets(data: &Value) -> Vec<NewItem> {
    data["viewer"]["assignedIssues"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|issue| {
            Some(NewItem {
                external_id: issue["id"].as_str()?.into(),
                kind: Kind::Ticket,
                title: issue["title"].as_str().unwrap_or_default().into(),
                body: excerpt(issue["description"].as_str().unwrap_or_default(), 500),
                author: None,
                url: issue["url"].as_str().map(str::to_owned),
                state: issue["state"]["name"].as_str().unwrap_or_default().into(),
                occurred_at: issue["updatedAt"]
                    .as_str()
                    .and_then(parse_time)
                    .unwrap_or(0),
                ends_at: issue["dueDate"].as_str().and_then(parse_day),
                meta: json!({
                    "identifier": issue["identifier"],
                    // backlog, unstarted or started: what the Tasks space groups by.
                    "stateType": issue["state"]["type"],
                    // 0 none, 1 urgent, 2 high, 3 medium, 4 low.
                    "priority": issue["priority"],
                    "project": issue["project"]["name"],
                }),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method},
    };

    fn answer() -> Value {
        json!({ "data": { "viewer": { "assignedIssues": { "nodes": [{
            "id": "abc", "identifier": "APP-12", "title": "Speed up the search",
            "description": "It takes two seconds.", "url": "https://linear.app/acme/issue/APP-12",
            "priority": 2, "updatedAt": "1970-01-01T00:01:00Z", "dueDate": "1970-01-02",
            "state": { "name": "In Progress", "type": "started" },
            "project": { "name": "Desktop" },
        }, {
            "id": "def", "identifier": "APP-13", "title": "No due date, no project",
            "description": null, "url": null, "priority": 0, "updatedAt": "1970-01-01T00:02:00Z",
            "dueDate": null, "state": { "name": "Todo", "type": "unstarted" }, "project": null,
        }] } } } })
    }

    #[test]
    fn translates_tickets() {
        let items = tickets(&answer()["data"]);
        assert_eq!(items.len(), 2);
        let first = &items[0];
        assert_eq!(first.kind, Kind::Ticket);
        assert_eq!(first.state, "In Progress");
        assert_eq!(first.occurred_at, 60);
        assert_eq!(first.ends_at, Some(86_400));
        assert_eq!(first.meta["identifier"], "APP-12");
        assert_eq!(first.meta["stateType"], "started");
        assert_eq!(first.meta["priority"], 2);
        assert_eq!(items[1].body, "");
        assert_eq!(items[1].ends_at, None);
    }

    #[tokio::test]
    async fn sends_the_key_as_is_and_reports_linear_errors() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(header("authorization", "lin_api_key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(answer()))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(header("authorization", "revoked"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "errors": [{ "message": "Authentication required" }],
            })))
            .mount(&server)
            .await;
        let linear = |token: Option<&str>| Linear {
            source: "work".into(),
            url: server.uri(),
            token: token.map(str::to_owned),
        };
        assert_eq!(linear(Some("lin_api_key")).sync().await.unwrap().len(), 2);
        assert_eq!(
            linear(Some("revoked")).sync().await.unwrap_err(),
            "Linear: Authentication required"
        );
        assert!(
            linear(None)
                .sync()
                .await
                .unwrap_err()
                .contains("no API key")
        );
    }
}
