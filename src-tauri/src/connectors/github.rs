//! GitHub, as a forge and a tracker: open pull requests the user wrote or must review,
//! issues assigned to them, and unread notifications.

use super::{Connector, REQUEST_LIMIT, excerpt, parse_time, unreachable};
use crate::{
    http::client,
    items::{Kind, NewItem},
};
use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::process::Command;

const API: &str = "https://api.github.com";

const QUERY: &str = r#"
query {
  authored: search(query: "is:pr is:open archived:false author:@me", type: ISSUE, first: 50) {
    nodes { ...pull }
  }
  review: search(query: "is:pr is:open archived:false review-requested:@me", type: ISSUE, first: 50) {
    nodes { ...pull }
  }
  assigned: search(query: "is:issue is:open archived:false assignee:@me", type: ISSUE, first: 50) {
    nodes {
      ... on Issue {
        id number title url updatedAt bodyText
        repository { nameWithOwner }
        author { login }
      }
    }
  }
}
fragment pull on PullRequest {
  id number title url isDraft updatedAt mergeable reviewDecision
  repository { nameWithOwner }
  author { login }
  commits(last: 1) { nodes { commit { statusCheckRollup { state } } } }
}
"#;

pub struct Github {
    source: String,
    base: String,
    /// `None`: use the GitHub command-line tool's own login, if it is installed.
    token: Option<String>,
}

impl Github {
    pub fn new(source: String, token: Option<String>) -> Self {
        Self {
            source,
            base: API.into(),
            token,
        }
    }

    async fn token(&self) -> Result<String, String> {
        match &self.token {
            Some(token) => Ok(token.clone()),
            None => cli_token().await,
        }
    }

    fn request(&self, method: reqwest::Method, path: &str, token: &str) -> reqwest::RequestBuilder {
        client()
            .request(method, format!("{}{path}", self.base))
            .timeout(REQUEST_LIMIT)
            .bearer_auth(token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
    }
}

/// The token of the GitHub command-line tool, when the user is signed in with it.
async fn cli_token() -> Result<String, String> {
    const HELP: &str = "Add a GitHub token, or install the GitHub CLI and sign in with it.";
    let gh = which::which("gh").map_err(|_| HELP.to_owned())?;
    let mut command = Command::new(gh);
    command.args(["auth", "token"]).kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let output = command.output().await.map_err(|error| error.to_string())?;
    let token = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if output.status.success() && !token.is_empty() {
        Ok(token)
    } else {
        Err(HELP.into())
    }
}

/// Reads GitHub's error out of a failed answer (`{ "message": "…" }`).
async fn failure(response: reqwest::Response) -> String {
    let status = response.status();
    let body: Value = response.json().await.unwrap_or_default();
    match body["message"].as_str() {
        Some(message) => format!("GitHub: {message} ({status})"),
        None => format!("GitHub answered {status}"),
    }
}

#[async_trait]
impl Connector for Github {
    fn source(&self) -> &str {
        &self.source
    }

    async fn sync(&self) -> Result<Vec<NewItem>, String> {
        let token = self.token().await?;

        let response = self
            .request(reqwest::Method::POST, "/graphql", &token)
            .json(&json!({ "query": QUERY }))
            .send()
            .await
            .map_err(unreachable)?;
        if !response.status().is_success() {
            return Err(failure(response).await);
        }
        let answer: Value = response.json().await.map_err(unreachable)?;
        if let Some(message) = answer["errors"][0]["message"].as_str() {
            return Err(format!("GitHub: {message}"));
        }
        let mut items = work_items(&answer["data"]);

        let response = self
            .request(reqwest::Method::GET, "/notifications?per_page=50", &token)
            .send()
            .await
            .map_err(unreachable)?;
        if !response.status().is_success() {
            return Err(failure(response).await);
        }
        let notifications: Value = response.json().await.map_err(unreachable)?;
        items.extend(notification_items(&notifications));
        Ok(items)
    }
}

/// Pull requests and issues out of the GraphQL answer.
fn work_items(data: &Value) -> Vec<NewItem> {
    let nodes = |name: &str| data[name]["nodes"].as_array().into_iter().flatten();
    let mut items: Vec<NewItem> = Vec::new();

    for (group, review_requested) in [("review", true), ("authored", false)] {
        for pull in nodes(group) {
            let Some(id) = pull["id"].as_str() else {
                continue;
            };
            // A pull request can only be listed once.
            if items.iter().any(|item| item.external_id == id) {
                continue;
            }
            let state = if review_requested {
                "review requested"
            } else if pull["isDraft"] == true {
                "draft"
            } else {
                "open"
            };
            items.push(NewItem {
                external_id: id.into(),
                kind: Kind::MergeRequest,
                title: pull["title"].as_str().unwrap_or_default().into(),
                body: reference(pull),
                author: pull["author"]["login"].as_str().map(str::to_owned),
                url: pull["url"].as_str().map(str::to_owned),
                state: state.into(),
                occurred_at: pull["updatedAt"].as_str().and_then(parse_time).unwrap_or(0),
                ends_at: None,
                meta: json!({
                    "repository": pull["repository"]["nameWithOwner"],
                    "number": pull["number"],
                    "reviewRequested": review_requested,
                    "draft": pull["isDraft"],
                    "mergeable": pull["mergeable"],
                    "reviewDecision": pull["reviewDecision"],
                    "checks": pull["commits"]["nodes"][0]["commit"]["statusCheckRollup"]["state"],
                }),
            });
        }
    }

    for issue in nodes("assigned") {
        let Some(id) = issue["id"].as_str() else {
            continue;
        };
        items.push(NewItem {
            external_id: id.into(),
            kind: Kind::Ticket,
            title: issue["title"].as_str().unwrap_or_default().into(),
            body: excerpt(issue["bodyText"].as_str().unwrap_or_default(), 500),
            author: issue["author"]["login"].as_str().map(str::to_owned),
            url: issue["url"].as_str().map(str::to_owned),
            state: "open".into(),
            occurred_at: issue["updatedAt"]
                .as_str()
                .and_then(parse_time)
                .unwrap_or(0),
            ends_at: None,
            meta: json!({
                "identifier": reference(issue),
                "repository": issue["repository"]["nameWithOwner"],
                // Same vocabulary as the trackers: an open issue is work not started.
                "stateType": "unstarted",
            }),
        });
    }
    items
}

/// `owner/repository #12`
fn reference(node: &Value) -> String {
    format!(
        "{} #{}",
        node["repository"]["nameWithOwner"]
            .as_str()
            .unwrap_or_default(),
        node["number"]
    )
}

fn notification_items(notifications: &Value) -> Vec<NewItem> {
    notifications
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|notification| {
            let id = notification["id"].as_str()?;
            let repository = notification["repository"]["full_name"]
                .as_str()
                .unwrap_or_default();
            Some(NewItem {
                external_id: format!("notification-{id}"),
                kind: Kind::Notification,
                title: notification["subject"]["title"]
                    .as_str()
                    .unwrap_or_default()
                    .into(),
                body: repository.into(),
                author: None,
                url: web_url(notification),
                state: notification["reason"]
                    .as_str()
                    .unwrap_or_default()
                    .replace('_', " "),
                occurred_at: notification["updated_at"]
                    .as_str()
                    .and_then(parse_time)
                    .unwrap_or(0),
                ends_at: None,
                meta: json!({ "repository": repository, "type": notification["subject"]["type"] }),
            })
        })
        .collect()
}

/// Notifications point at the API; the user wants the page. Falls back to the repository.
fn web_url(notification: &Value) -> Option<String> {
    match notification["subject"]["url"].as_str() {
        Some(api) => Some(
            api.replace("https://api.github.com/repos/", "https://github.com/")
                .replace("/pulls/", "/pull/"),
        ),
        None => notification["repository"]["html_url"]
            .as_str()
            .map(str::to_owned),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path},
    };

    fn graphql_answer() -> Value {
        let pull = |id: &str, number: u32, draft: bool| {
            json!({
                "id": id, "number": number, "title": format!("Pull {number}"),
                "url": format!("https://github.com/acme/app/pull/{number}"),
                "isDraft": draft, "updatedAt": "1970-01-01T00:01:00Z",
                "mergeable": "MERGEABLE", "reviewDecision": "REVIEW_REQUIRED",
                "repository": { "nameWithOwner": "acme/app" },
                "author": { "login": "octo" },
                "commits": { "nodes": [{ "commit": { "statusCheckRollup": { "state": "SUCCESS" } } }] },
            })
        };
        json!({ "data": {
            "authored": { "nodes": [pull("PR_1", 1, true), pull("PR_2", 2, false)] },
            "review": { "nodes": [pull("PR_3", 3, false), {}] },
            "assigned": { "nodes": [{
                "id": "I_9", "number": 9, "title": "Crash on start", "bodyText": " It crashes. ",
                "url": "https://github.com/acme/app/issues/9", "updatedAt": "1970-01-01T00:02:00Z",
                "repository": { "nameWithOwner": "acme/app" }, "author": { "login": "mona" },
            }] },
        }})
    }

    #[test]
    fn translates_pull_requests_and_issues() {
        let items = work_items(&graphql_answer()["data"]);
        let summary: Vec<_> = items
            .iter()
            .map(|i| (i.external_id.as_str(), i.kind, i.state.as_str()))
            .collect();
        assert_eq!(
            summary,
            [
                ("PR_3", Kind::MergeRequest, "review requested"),
                ("PR_1", Kind::MergeRequest, "draft"),
                ("PR_2", Kind::MergeRequest, "open"),
                ("I_9", Kind::Ticket, "open"),
            ]
        );
        let review = &items[0];
        assert_eq!(review.body, "acme/app #3");
        assert_eq!(review.occurred_at, 60);
        assert_eq!(review.meta["checks"], "SUCCESS");
        assert_eq!(review.meta["reviewRequested"], true);
        let issue = &items[3];
        assert_eq!(issue.body, "It crashes.");
        assert_eq!(issue.meta["identifier"], "acme/app #9");
    }

    #[test]
    fn notifications_link_to_the_page_not_the_api() {
        let items = notification_items(&json!([
            {
                "id": "77", "reason": "review_requested", "updated_at": "1970-01-01T00:03:00Z",
                "subject": { "title": "Fix the build", "type": "PullRequest",
                             "url": "https://api.github.com/repos/acme/app/pulls/4" },
                "repository": { "full_name": "acme/app", "html_url": "https://github.com/acme/app" },
            },
            {
                "id": "78", "reason": "subscribed", "updated_at": "1970-01-01T00:04:00Z",
                "subject": { "title": "New release", "type": "Release", "url": null },
                "repository": { "full_name": "acme/app", "html_url": "https://github.com/acme/app" },
            },
        ]));
        assert_eq!(items[0].external_id, "notification-77");
        assert_eq!(items[0].state, "review requested");
        assert_eq!(
            items[0].url.as_deref(),
            Some("https://github.com/acme/app/pull/4")
        );
        assert_eq!(items[1].url.as_deref(), Some("https://github.com/acme/app"));
    }

    #[tokio::test]
    async fn syncs_with_the_token_and_reports_github_errors() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(header("authorization", "Bearer the-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(graphql_answer()))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/notifications"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&server)
            .await;
        let github = |token: &str| Github {
            source: "gh".into(),
            base: server.uri(),
            token: Some(token.into()),
        };
        assert_eq!(github("the-token").sync().await.unwrap().len(), 4);

        // Any other token gets no mock, hence a 404 from the fake server.
        let error = github("wrong").sync().await.unwrap_err();
        assert!(error.starts_with("GitHub answered 404"), "{error}");
    }

    /// Reads the real account of whoever is signed in with the GitHub CLI, so it only runs on
    /// demand: `cargo test -- --ignored real_github`. Prints counts, never contents.
    #[tokio::test]
    #[ignore]
    async fn real_github_syncs() {
        let items = Github::new("gh".into(), None).sync().await.unwrap();
        let count = |kind| items.iter().filter(|i| i.kind == kind).count();
        println!(
            "merge requests: {}, tickets: {}, notifications: {}",
            count(Kind::MergeRequest),
            count(Kind::Ticket),
            count(Kind::Notification)
        );
        assert!(
            items
                .iter()
                .all(|i| !i.external_id.is_empty() && i.occurred_at > 0)
        );
    }
}
