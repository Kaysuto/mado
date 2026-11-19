//! Anthropic's Messages API, over plain HTTP (there is no official Rust SDK).

use super::{OnDelta, Outcome, Reply, Request, events, failure, unreachable};
use serde_json::{Value, json};

const OFFICIAL: &str = "https://api.anthropic.com";
const VERSION: &str = "2023-06-01";
/// Room for the answer (and the model's own reasoning, which counts towards it).
const MAX_TOKENS: u32 = 16_000;

fn url(base: &str, path: &str) -> String {
    let base = if base.is_empty() { OFFICIAL } else { base };
    format!("{}{path}", base.trim_end_matches('/'))
}

fn body(request: &Request, stream: bool) -> Value {
    let mut body = json!({
        "model": request.model,
        "max_tokens": MAX_TOKENS,
        "messages": request.messages,
        "stream": stream,
    });
    if let Some(system) = &request.system {
        body["system"] = json!(system);
    }
    body
}

/// Like `complete`, calling `on_delta` with each piece of text as it arrives.
pub async fn stream(
    client: &reqwest::Client,
    base: &str,
    key: &str,
    request: &Request,
    on_delta: OnDelta<'_>,
) -> Outcome<Reply> {
    if request.model.is_empty() {
        return Err("Choose a model for this provider.".into());
    }
    let mut response = client
        .post(url(base, "/v1/messages"))
        .header("x-api-key", key)
        .header("anthropic-version", VERSION)
        .json(&body(request, true))
        .send()
        .await
        .map_err(unreachable)?;
    if !response.status().is_success() {
        return Err(failure(response).await);
    }
    let mut reply = Reply {
        text: String::new(),
        input_tokens: None,
        output_tokens: None,
    };
    let mut stop = None;
    let mut problem = None;
    events(&mut response, |data| {
        let Ok(event) = serde_json::from_str::<Value>(data) else {
            return;
        };
        match event["type"].as_str() {
            Some("message_start") => {
                reply.input_tokens = event["message"]["usage"]["input_tokens"].as_u64();
            }
            // Only text is shown; the model's reasoning comes in other kinds of pieces.
            Some("content_block_delta") if event["delta"]["type"] == "text_delta" => {
                if let Some(text) = event["delta"]["text"].as_str() {
                    reply.text.push_str(text);
                    on_delta(text);
                }
            }
            Some("message_delta") => {
                reply.output_tokens = event["usage"]["output_tokens"].as_u64();
                stop = event["delta"]["stop_reason"].as_str().map(str::to_owned);
            }
            Some("error") => problem = event["error"]["message"].as_str().map(str::to_owned),
            _ => {}
        }
    })
    .await?;
    if let Some(problem) = problem {
        return Err(problem);
    }
    match stop.as_deref() {
        Some("refusal") => Err("The model declined this request.".into()),
        Some("max_tokens") => Err("The answer was cut off: it was too long.".into()),
        _ => Ok(reply),
    }
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
    let response = client
        .post(url(base, "/v1/messages"))
        .header("x-api-key", key)
        .header("anthropic-version", VERSION)
        .json(&body(request, false))
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
    async fn streams_the_text_piece_by_piece_and_ignores_the_reasoning() {
        let stream = [
            "event: message_start",
            r#"data: {"type":"message_start","message":{"usage":{"input_tokens":12}}}"#,
            "",
            r#"data: {"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"hmm"}}"#,
            r#"data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"Bon"}}"#,
            r#"data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"jour été"}}"#,
            r#"data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":5}}"#,
            r#"data: {"type":"message_stop"}"#,
            "",
        ]
        .join("\n");
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(body_partial_json(json!({ "stream": true })))
            .respond_with(ResponseTemplate::new(200).set_body_raw(stream, "text/event-stream"))
            .mount(&server)
            .await;

        let pieces = std::sync::Mutex::new(Vec::new());
        let reply = super::stream(
            &reqwest::Client::new(),
            &server.uri(),
            "key",
            &request(None),
            &|piece| pieces.lock().unwrap().push(piece.to_owned()),
        )
        .await
        .unwrap();
        assert_eq!(*pieces.lock().unwrap(), ["Bon", "jour été"]);
        assert_eq!(
            reply,
            Reply {
                text: "Bonjour été".into(),
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
