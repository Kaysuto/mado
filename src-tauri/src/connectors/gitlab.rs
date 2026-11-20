//! GitLab, as a forge and a tracker, on gitlab.com or on an instance of one's own: open merge
//! requests the user wrote or must review, issues assigned to them, and their pending to-dos.

use super::{Connector, REQUEST_LIMIT, excerpt, parse_time, unreachable};
use crate::{
    assistant::line,
    http::client,
    items::{Kind, NewItem},
};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

const WORK: &str = r#"
query {
  currentUser {
    username
    reviewRequestedMergeRequests(state: opened, first: 50) { nodes { ...request } }
    authoredMergeRequests(state: opened, first: 50) { nodes { ...request } }
    todos(state: pending, first: 50) {
      nodes { id body action createdAt targetUrl author { username } project { fullPath } }
    }
  }
}
fragment request on MergeRequest {
  id iid title draft webUrl updatedAt reference(full: true)
  author { username }
  headPipeline { status }
}
"#;

/// Issues are asked for by user name: the first answer gives it.
const ISSUES: &str = r#"
query($me: [String!]) {
  issues(assigneeUsernames: $me, state: opened, first: 50) {
    nodes { id title description webUrl updatedAt reference(full: true) author { username } }
  }
}
"#;

/// Where the instance is, for those who host their own.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Options {
    /// Empty means gitlab.com.
    pub url: String,
}

pub struct Gitlab {
    source: String,
    /// Address of the instance, without a trailing slash. `None` when what was typed is not one.
    base: Option<String>,
    token: Option<String>,
}

impl Gitlab {
    pub fn new(source: String, options: Options, token: Option<String>) -> Self {
        let url = options.url.trim().trim_end_matches('/');
        let base = match url {
            "" => Some("https://gitlab.com".to_owned()),
            // The token travels with every request: never in the clear.
            url if url.starts_with("https://") => Some(url.to_owned()),
            _ => None,
        };
        Self {
            source,
            base,
            token,
        }
    }

    /// The `data` of a GraphQL answer, or GitLab's own words on what went wrong.
    async fn graphql(&self, query: &str, variables: Value) -> Result<Value, String> {
        let base = self
            .base
            .as_deref()
            .ok_or("A GitLab address starts with https://.")?;
        let token = self
            .token
            .as_deref()
            .ok_or("Add an access token that can read the API (read_api).")?;
        let response = client()
            .post(format!("{base}/api/graphql"))
            .timeout(REQUEST_LIMIT)
            .bearer_auth(token)
            .json(&json!({ "query": query, "variables": variables }))
            .send()
            .await
            .map_err(unreachable)?;
        if !response.status().is_success() {
            return Err(format!("GitLab answered {}", response.status()));
        }
        let mut answer: Value = response.json().await.map_err(unreachable)?;
        if let Some(message) = answer["errors"][0]["message"].as_str() {
            return Err(format!("GitLab: {message}"));
        }
        Ok(answer["data"].take())
    }
}

#[async_trait]
impl Connector for Gitlab {
    fn source(&self) -> &str {
        &self.source
    }

    async fn sync(&self) -> Result<Vec<NewItem>, String> {
        let work = self.graphql(WORK, json!({})).await?;
        let user = &work["currentUser"];
        // Without a valid token GitLab answers as to a visitor: nobody is signed in.
        let name = user["username"]
            .as_str()
            .ok_or("GitLab does not know this access token.")?;
        let mut items = merge_requests(user);
        items.extend(todos(user));
        let issues = self.graphql(ISSUES, json!({ "me": [name] })).await?;
        items.extend(tickets(&issues));
        Ok(items)
    }
}

fn nodes(list: &Value) -> impl Iterator<Item = &Value> {
    list["nodes"].as_array().into_iter().flatten()
}

/// `group/project` out of a full reference such as `group/project!12` or `group/project#7`.
fn project(reference: &str) -> &str {
    reference
        .rsplit_once(['!', '#'])
        .map_or("", |(path, _)| path)
}

/// How a pipeline went, in the words every forge is translated into.
fn checks(status: &Value) -> Option<&'static str> {
    match status.as_str()? {
        "SUCCESS" => Some("SUCCESS"),
        "FAILED" => Some("FAILURE"),
        "CREATED"
        | "WAITING_FOR_RESOURCE"
        | "PREPARING"
        | "WAITING_FOR_CALLBACK"
        | "PENDING"
        | "RUNNING" => Some("PENDING"),
        // Cancelled, skipped, manual or scheduled: nothing ran that passed or failed.
        _ => None,
    }
}

fn merge_requests(user: &Value) -> Vec<NewItem> {
    let mut items: Vec<NewItem> = Vec::new();
    let groups = [
        ("reviewRequestedMergeRequests", true),
        ("authoredMergeRequests", false),
    ];
    for (group, review_requested) in groups {
        for request in nodes(&user[group]) {
            let Some(id) = request["id"].as_str() else {
                continue;
            };
            // A merge request can only be listed once.
            if items.iter().any(|item| item.external_id == id) {
                continue;
            }
            let reference = request["reference"].as_str().unwrap_or_default();
            let state = if review_requested {
                "review requested"
            } else if request["draft"] == true {
                "draft"
            } else {
                "open"
            };
            items.push(NewItem {
                external_id: id.into(),
                kind: Kind::MergeRequest,
                title: request["title"].as_str().unwrap_or_default().into(),
                body: reference.into(),
                author: request["author"]["username"].as_str().map(str::to_owned),
                url: request["webUrl"].as_str().map(str::to_owned),
                state: state.into(),
                occurred_at: request["updatedAt"]
                    .as_str()
                    .and_then(parse_time)
                    .unwrap_or(0),
                ends_at: None,
                meta: json!({
                    "repository": project(reference),
                    "number": request["iid"],
                    "reviewRequested": review_requested,
                    "draft": request["draft"],
                    "checks": checks(&request["headPipeline"]["status"]),
                }),
            });
        }
    }
    items
}

fn tickets(data: &Value) -> Vec<NewItem> {
    nodes(&data["issues"])
        .filter_map(|issue| {
            let reference = issue["reference"].as_str().unwrap_or_default();
            Some(NewItem {
                external_id: issue["id"].as_str()?.into(),
                kind: Kind::Ticket,
                title: issue["title"].as_str().unwrap_or_default().into(),
                body: excerpt(issue["description"].as_str().unwrap_or_default(), 500),
                author: issue["author"]["username"].as_str().map(str::to_owned),
                url: issue["webUrl"].as_str().map(str::to_owned),
                state: "open".into(),
                occurred_at: issue["updatedAt"]
                    .as_str()
                    .and_then(parse_time)
                    .unwrap_or(0),
                ends_at: None,
                meta: json!({
                    "identifier": reference,
                    "repository": project(reference),
                    // Same vocabulary as the trackers: an open issue is work not started.
                    "stateType": "unstarted",
                }),
            })
        })
        .collect()
}

/// A to-do is GitLab's notification: something that waits for the user.
fn todos(user: &Value) -> Vec<NewItem> {
    nodes(&user["todos"])
        .filter_map(|todo| {
            let repository = todo["project"]["fullPath"].as_str().unwrap_or_default();
            Some(NewItem {
                external_id: todo["id"].as_str()?.into(),
                kind: Kind::Notification,
                title: line(todo["body"].as_str().unwrap_or_default(), 200),
                body: repository.into(),
                author: todo["author"]["username"].as_str().map(str::to_owned),
                url: todo["targetUrl"].as_str().map(str::to_owned),
                state: todo["action"].as_str().unwrap_or("unread").into(),
                occurred_at: todo["createdAt"].as_str().and_then(parse_time).unwrap_or(0),
                ends_at: None,
                meta: json!({ "repository": repository, "type": todo["action"] }),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_string_contains, header, method, path},
    };

    fn request(id: &str, title: &str, draft: bool, pipeline: Value) -> Value {
        json!({
            "id": id, "iid": "12", "title": title, "draft": draft,
            "webUrl": "https://gitlab.com/acme/tools/app/-/merge_requests/12",
            "updatedAt": "2026-11-19T08:30:00Z", "reference": "acme/tools/app!12",
            "author": { "username": "mona" }, "headPipeline": pipeline,
        })
    }

    fn user() -> Value {
        let review = request(
            "gid://1",
            "Add the retry",
            false,
            json!({ "status": "RUNNING" }),
        );
        json!({
            "username": "me",
            "reviewRequestedMergeRequests": { "nodes": [review.clone()] },
            "authoredMergeRequests": { "nodes": [
                review,
                request("gid://2", "Draft: new parser", true, json!({ "status": "FAILED" })),
                request("gid://3", "Docs", false, Value::Null),
            ] },
            "todos": { "nodes": [{
                "id": "gid://todo/1", "body": "Can you\ncheck this?", "action": "mentioned",
                "createdAt": "2026-11-18T10:00:00Z",
                "targetUrl": "https://gitlab.com/acme/tools/app/-/issues/7#note_1",
                "author": { "username": "kenji" }, "project": { "fullPath": "acme/tools/app" },
            }] },
        })
    }

    #[test]
    fn translates_merge_requests_and_their_pipelines() {
        let items = merge_requests(&user());
        let states: Vec<&str> = items.iter().map(|item| item.state.as_str()).collect();
        assert_eq!(states, ["review requested", "draft", "open"]);
        let checks: Vec<&Value> = items.iter().map(|item| &item.meta["checks"]).collect();
        assert_eq!(checks, [&json!("PENDING"), &json!("FAILURE"), &Value::Null]);
        let review = &items[0];
        assert_eq!(
            (review.kind, review.body.as_str()),
            (Kind::MergeRequest, "acme/tools/app!12")
        );
        assert_eq!(review.meta["repository"], "acme/tools/app");
        assert_eq!(review.meta["reviewRequested"], true);
        assert_eq!(review.author.as_deref(), Some("mona"));
        assert_eq!(
            review.occurred_at,
            parse_time("2026-11-19T08:30:00Z").unwrap()
        );
    }

    #[test]
    fn translates_issues_and_to_dos() {
        let data = json!({ "issues": { "nodes": [{
            "id": "gid://issue/7", "title": "Search is stale", "description": "Steps…",
            "webUrl": "https://gitlab.com/acme/tools/app/-/issues/7",
            "updatedAt": "2026-11-19T08:30:00Z", "reference": "acme/tools/app#7",
            "author": { "username": "imani" },
        }] } });
        let issue = &tickets(&data)[0];
        assert_eq!(
            (issue.kind, issue.title.as_str()),
            (Kind::Ticket, "Search is stale")
        );
        assert_eq!(issue.meta["identifier"], "acme/tools/app#7");
        assert_eq!(issue.meta["repository"], "acme/tools/app");

        let todo = &todos(&user())[0];
        assert_eq!(
            (todo.kind, todo.title.as_str()),
            (Kind::Notification, "Can you check this?")
        );
        assert_eq!(
            (todo.state.as_str(), todo.body.as_str()),
            ("mentioned", "acme/tools/app")
        );
        assert_eq!(
            todo.url.as_deref(),
            Some("https://gitlab.com/acme/tools/app/-/issues/7#note_1")
        );
    }

    #[test]
    fn the_instance_is_gitlab_com_unless_another_https_address_is_given() {
        let base = |url: &str| Gitlab::new("gl".into(), Options { url: url.into() }, None).base;
        assert_eq!(base("").as_deref(), Some("https://gitlab.com"));
        assert_eq!(
            base(" https://git.example.com/ ").as_deref(),
            Some("https://git.example.com")
        );
        assert_eq!(base("http://git.example.com"), None);
    }

    #[tokio::test]
    async fn syncs_with_the_token_and_reports_what_goes_wrong() {
        let server = MockServer::start().await;
        let answer =
            |data: Value| ResponseTemplate::new(200).set_body_json(json!({ "data": data }));
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(header("authorization", "Bearer the-token"))
            .and(body_string_contains("currentUser"))
            .respond_with(answer(json!({ "currentUser": user() })))
            .mount(&server)
            .await;
        // The issues are asked for with the name the first answer gave.
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_string_contains(r#""me":["me"]"#))
            .respond_with(answer(json!({ "issues": { "nodes": [] } })))
            .mount(&server)
            .await;
        // What GitLab answers to a token it does not know: a visitor, signed in as nobody.
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(header("authorization", "Bearer wrong"))
            .respond_with(answer(json!({ "currentUser": null })))
            .mount(&server)
            .await;
        let gitlab = |token: Option<&str>| Gitlab {
            source: "gl".into(),
            base: Some(server.uri()),
            token: token.map(str::to_owned),
        };
        // Three merge requests and one to-do.
        assert_eq!(gitlab(Some("the-token")).sync().await.unwrap().len(), 4);
        let unknown = gitlab(Some("wrong")).sync().await.unwrap_err();
        assert_eq!(unknown, "GitLab does not know this access token.");
        let missing = gitlab(None).sync().await.unwrap_err();
        assert!(missing.starts_with("Add an access token"), "{missing}");
    }

    /// Asks the real gitlab.com, as a visitor, whether it still accepts the two queries: a
    /// field that no longer exists is an error even without an account. Only on demand:
    /// `cargo test -- --ignored real_gitlab`.
    #[tokio::test]
    #[ignore]
    async fn real_gitlab_accepts_the_queries() {
        for (query, variables) in [(WORK, json!({})), (ISSUES, json!({ "me": ["nobody-xyz"] }))] {
            let answer: Value = client()
                .post("https://gitlab.com/api/graphql")
                .json(&json!({ "query": query, "variables": variables }))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            assert_eq!(answer["errors"], Value::Null, "{answer}");
            assert!(answer["data"].is_object(), "{answer}");
        }
    }
}
