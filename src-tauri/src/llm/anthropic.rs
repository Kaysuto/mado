//! Anthropic's Messages API, over plain HTTP (there is no official Rust SDK).

use super::{Outcome, Reply, Request, failure, unreachable};
use serde_json::{Value, json};

const OFFICIAL: &str = "https://api.anthropic.com";
const VERSION: &str = "2023-06-01";
/// Room for the answer (and the model's own reasoning, which counts towards it).
const MAX_TOKENS: u32 = 16_000;

fn url(base: &str, path: &str) -> String {
    let base = if base.is_empty() { OFFICIAL } else { base };
    format!("{}{path}", base.trim_end_matches('/'))
}

pub async fn complete(
    client: &reqwest::Client,
    base: &str,
    key: &str,
    request: &Request,
) -> Outcome<Reply> {
    if request.model.is_empty() {
        return Err("Choose a model for this provider.".into());
    }
    let mut body = json!({
        "model": request.model,
        "max_tokens": MAX_TOKENS,
        "messages": request.messages,
    });
    if let Some(system) = &request.system {
        body["system"] = json!(system);
    }
    let response = client
        .post(url(base, "/v1/messages"))
        .header("x-api-key", key)
        .header("anthropic-version", VERSION)
        .json(&body)
        .send()
        .await
        .map_err(unreachable)?;
    if !response.status().is_success() {
        return Err(failure(response).await);
    }
    let answer: Value = response.json().await.map_err(unreachable)?;

    // Always look at why the model stopped before reading what it wrote.
    match answer["stop_reason"].as_str() {
        Some("refusal") => return Err("The model declined this request.".into()),
        Some("max_tokens") => return Err("The answer was cut off: it was too long.".into()),
        _ => {}
    }
    let text: String = answer["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|block| block["type"] == "text")
        .filter_map(|block| block["text"].as_str())
        .collect();
    Ok(Reply {
        text,
        input_tokens: answer["usage"]["input_tokens"].as_u64(),
        output_tokens: answer["usage"]["output_tokens"].as_u64(),
    })
}

pub async fn models(client: &reqwest::Client, base: &str, key: &str) -> Outcome<Vec<String>> {
    let response = client
        .get(url(base, "/v1/models?limit=1000"))
        .header("x-api-key", key)
        .header("anthropic-version", VERSION)
        .send()
        .await
        .map_err(unreachable)?;
    if !response.status().is_success() {
        return Err(failure(response).await);
    }
    let answer: Value = response.json().await.map_err(unreachable)?;
    Ok(super::openai_compat::model_ids(&answer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{Message, Role};
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_partial_json, header, method, path},
    };

    fn request(system: Option<&str>) -> Request {
        Request {
            model: "some-model".into(),
            system: system.map(str::to_owned),
            messages: vec![Message {
                role: Role::User,
                content: "Hello".into(),
            }],
        }
    }

    #[tokio::test]
    async fn sends_the_conversation_and_reads_the_text_blocks() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "the-key"))
            .and(header("anthropic-version", VERSION))
            .and(body_partial_json(json!({
                "model": "some-model",
                "system": "Be brief.",
                "messages": [{ "role": "user", "content": "Hello" }],
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "stop_reason": "end_turn",
                "content": [
                    { "type": "thinking", "thinking": "" },
                    { "type": "text", "text": "Hi " },
                    { "type": "text", "text": "there." },
                ],
                "usage": { "input_tokens": 12, "output_tokens": 5 },
            })))
            .expect(1)
            .mount(&server)
            .await;

        let reply = complete(
            &reqwest::Client::new(),
            &server.uri(),
            "the-key",
            &request(Some("Be brief.")),
        )
        .await
        .unwrap();
        assert_eq!(
            reply,
            Reply {
                text: "Hi there.".into(),
                input_tokens: Some(12),
                output_tokens: Some(5),
            }
        );
    }

    #[tokio::test]
    async fn reports_the_providers_own_error_message() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({
                "type": "error",
                "error": { "type": "authentication_error", "message": "invalid x-api-key" },
            })))
            .mount(&server)
            .await;
        let error = complete(
            &reqwest::Client::new(),
            &server.uri(),
            "bad",
            &request(None),
        )
        .await
        .unwrap_err();
        assert_eq!(error, "401 Unauthorized: invalid x-api-key");
    }

    #[tokio::test]
    async fn a_refusal_is_an_error_not_an_empty_answer() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "stop_reason": "refusal",
                "content": [],
            })))
            .mount(&server)
            .await;
        let error = complete(
            &reqwest::Client::new(),
            &server.uri(),
            "key",
            &request(None),
        )
        .await
        .unwrap_err();
        assert_eq!(error, "The model declined this request.");
    }
}
