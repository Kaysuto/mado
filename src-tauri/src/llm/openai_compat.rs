//! The OpenAI Chat Completions protocol, spoken by most services and local servers.

use super::{Outcome, Reply, Request, failure, unreachable};
use serde_json::{Value, json};

fn url(base: &str, path: &str) -> Outcome<String> {
    if !(base.starts_with("https://") || base.starts_with("http://")) {
        return Err("This provider needs an address starting with http:// or https://.".into());
    }
    Ok(format!("{}{path}", base.trim_end_matches('/')))
}

/// Local servers take no key: the header is only sent when there is one.
fn authorized(builder: reqwest::RequestBuilder, key: Option<&str>) -> reqwest::RequestBuilder {
    match key {
        Some(key) if !key.is_empty() => builder.bearer_auth(key),
        _ => builder,
    }
}

pub async fn complete(
    client: &reqwest::Client,
    base: &str,
    key: Option<&str>,
    request: &Request,
) -> Outcome<Reply> {
    if request.model.is_empty() {
        return Err("Choose a model for this provider.".into());
    }
    let mut messages = Vec::new();
    if let Some(system) = &request.system {
        messages.push(json!({ "role": "system", "content": system }));
    }
    messages.extend(request.messages.iter().map(|message| json!(message)));

    // No token limit is sent: its name differs between services, and each has a sane default.
    let body = json!({ "model": request.model, "messages": messages });
    let response = authorized(client.post(url(base, "/chat/completions")?), key)
        .json(&body)
        .send()
        .await
        .map_err(unreachable)?;
    if !response.status().is_success() {
        return Err(failure(response).await);
    }
    let answer: Value = response.json().await.map_err(unreachable)?;
    let choice = &answer["choices"][0];
    if choice["finish_reason"] == "length" {
        return Err("The answer was cut off: it was too long.".into());
    }
    let text = choice["message"]["content"]
        .as_str()
        .ok_or("The provider sent an answer without text.")?
        .to_owned();
    Ok(Reply {
        text,
        input_tokens: answer["usage"]["prompt_tokens"].as_u64(),
        output_tokens: answer["usage"]["completion_tokens"].as_u64(),
    })
}

pub async fn models(
    client: &reqwest::Client,
    base: &str,
    key: Option<&str>,
) -> Outcome<Vec<String>> {
    let response = authorized(client.get(url(base, "/models")?), key)
        .send()
        .await
        .map_err(unreachable)?;
    if !response.status().is_success() {
        return Err(failure(response).await);
    }
    let answer: Value = response.json().await.map_err(unreachable)?;
    Ok(model_ids(&answer))
}

/// Model lists have the same shape everywhere: `{ "data": [{ "id": "…" }] }`.
pub fn model_ids(answer: &Value) -> Vec<String> {
    answer["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|model| model["id"].as_str().map(str::to_owned))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{Message, Role};
    use wiremock::{
        Mock, MockServer, Request as Received, ResponseTemplate,
        matchers::{body_partial_json, header, method, path},
    };

    fn request() -> Request {
        Request {
            model: "some-model".into(),
            system: Some("Be brief.".into()),
            messages: vec![Message {
                role: Role::User,
                content: "Hello".into(),
            }],
        }
    }

    #[tokio::test]
    async fn sends_the_system_prompt_first_and_reads_the_answer() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer the-key"))
            .and(body_partial_json(json!({
                "model": "some-model",
                "messages": [
                    { "role": "system", "content": "Be brief." },
                    { "role": "user", "content": "Hello" },
                ],
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "finish_reason": "stop", "message": { "role": "assistant", "content": "Hi." } }],
                "usage": { "prompt_tokens": 9, "completion_tokens": 2 },
            })))
            .expect(1)
            .mount(&server)
            .await;

        // A trailing slash in the address the user typed must not matter.
        let base = format!("{}/v1/", server.uri());
        let reply = complete(&reqwest::Client::new(), &base, Some("the-key"), &request())
            .await
            .unwrap();
        assert_eq!(
            reply,
            Reply {
                text: "Hi.".into(),
                input_tokens: Some(9),
                output_tokens: Some(2),
            }
        );
    }

    #[tokio::test]
    async fn sends_no_authorization_without_a_key_and_lists_models() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .and(|received: &Received| !received.headers.contains_key("authorization"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": [{ "id": "llama" }, { "id": "qwen" }],
            })))
            .expect(1)
            .mount(&server)
            .await;
        let base = format!("{}/v1", server.uri());
        let models = models(&reqwest::Client::new(), &base, None).await.unwrap();
        assert_eq!(models, ["llama", "qwen"]);
    }

    #[tokio::test]
    async fn refuses_an_address_that_is_not_a_web_address() {
        let error = complete(&reqwest::Client::new(), "localhost:11434", None, &request())
            .await
            .unwrap_err();
        assert!(error.contains("http://"));
    }
}
