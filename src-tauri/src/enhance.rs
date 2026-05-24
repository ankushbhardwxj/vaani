//! Calls the Anthropic Messages API to enhance transcribed text.
//!
//! Two modes are supported:
//!
//! - **Non-streaming** ([`enhance()`]) — simple request/response, good for
//!   testing and as a fallback.
//! - **Streaming** ([`enhance_streaming()`]) — uses SSE to deliver tokens
//!   incrementally so the UI can paste text as it arrives.
//!
//! A lower-level [`enhance_streaming_with_url()`] variant accepts a custom
//! endpoint URL for integration tests against a mock server.

use crate::error::VaaniError;
use serde::{Deserialize, Serialize};
use tokio::time::Instant;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default Anthropic Messages API endpoint.
const DEFAULT_ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";

/// Required Anthropic API version header value.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// How often (in milliseconds) to flush buffered tokens to the callback.
const TOKEN_FLUSH_INTERVAL_MS: u64 = 50;

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

/// A single message in the Anthropic Messages API request.
#[derive(Debug, Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

/// Request body sent to the Anthropic Messages API.
#[derive(Debug, Serialize)]
struct AnthropicRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: Vec<Message<'a>>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
}

/// A single content block in the Anthropic response.
#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    text: String,
}

/// Top-level Anthropic Messages API response (non-streaming).
#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<ContentBlock>,
}

/// The `delta` field inside a `content_block_delta` SSE event.
#[derive(Debug, Deserialize)]
struct Delta {
    #[serde(rename = "type")]
    delta_type: String,
    #[serde(default)]
    text: String,
}

/// Parsed SSE `data:` payload for a `content_block_delta` event.
#[derive(Debug, Deserialize)]
struct SseEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    delta: Option<Delta>,
}

// ---------------------------------------------------------------------------
// Public API — non-streaming
// ---------------------------------------------------------------------------

/// Enhance text using the Anthropic Messages API (non-streaming).
///
/// This is the simple request/response variant. It posts a single user
/// message with the given `system_prompt` and returns the full enhanced text.
///
/// # Errors
///
/// Returns [`VaaniError::MissingApiKey`] if `api_key` is empty,
/// [`VaaniError::Enhance`] if the input text is empty or the API returns an
/// error, and [`VaaniError::Http`] on network failures.
pub async fn enhance(
    client: &reqwest::Client,
    api_key: &str,
    text: &str,
    model: &str,
    system_prompt: &str,
) -> Result<String, VaaniError> {
    enhance_with_url(
        client,
        DEFAULT_ANTHROPIC_URL,
        api_key,
        text,
        model,
        system_prompt,
    )
    .await
}

/// Non-streaming enhance with a configurable endpoint URL (for testing).
async fn enhance_with_url(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    text: &str,
    model: &str,
    system_prompt: &str,
) -> Result<String, VaaniError> {
    validate_inputs(api_key, text)?;

    let body = AnthropicRequest {
        model,
        max_tokens: 4096,
        system: system_prompt,
        messages: vec![Message {
            role: "user",
            content: text,
        }],
        stream: false,
    };

    tracing::debug!(
        url = url,
        model = model,
        "sending non-streaming enhance request"
    );

    let response = send_request(client, url, api_key, &body).await?;
    let status = response.status();

    if !status.is_success() {
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable body>".into());
        return Err(VaaniError::Enhance(format!("HTTP {status}: {body}")));
    }

    let parsed: AnthropicResponse = response
        .json()
        .await
        .map_err(|e| VaaniError::Enhance(format!("failed to parse response: {e}")))?;

    let result = extract_text_from_response(&parsed);
    tracing::debug!(chars = result.len(), "enhancement complete");
    Ok(result)
}

// ---------------------------------------------------------------------------
// Public API — streaming
// ---------------------------------------------------------------------------

/// Enhance text using the Anthropic Messages API with SSE streaming.
///
/// Tokens are buffered and flushed to `on_tokens` approximately every 50 ms,
/// which allows the caller to paste text incrementally. The full accumulated
/// text is returned when the stream completes.
///
/// # Errors
///
/// Same error conditions as [`enhance()`].
pub async fn enhance_streaming(
    client: &reqwest::Client,
    api_key: &str,
    text: &str,
    model: &str,
    system_prompt: &str,
    on_tokens: impl FnMut(&str) + Send,
) -> Result<String, VaaniError> {
    enhance_streaming_with_url(
        client,
        DEFAULT_ANTHROPIC_URL,
        api_key,
        text,
        model,
        system_prompt,
        on_tokens,
    )
    .await
}

/// Streaming enhance with a configurable endpoint URL (for testing).
pub async fn enhance_streaming_with_url(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    text: &str,
    model: &str,
    system_prompt: &str,
    mut on_tokens: impl FnMut(&str) + Send,
) -> Result<String, VaaniError> {
    validate_inputs(api_key, text)?;

    let body = AnthropicRequest {
        model,
        max_tokens: 4096,
        system: system_prompt,
        messages: vec![Message {
            role: "user",
            content: text,
        }],
        stream: true,
    };

    tracing::debug!(
        url = url,
        model = model,
        "sending streaming enhance request"
    );

    let response = send_request(client, url, api_key, &body).await?;
    let status = response.status();

    if !status.is_success() {
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable body>".into());
        return Err(VaaniError::Enhance(format!("HTTP {status}: {body}")));
    }

    read_sse_stream(response, &mut on_tokens).await
}

// ---------------------------------------------------------------------------
// SSE stream reader with token batching
// ---------------------------------------------------------------------------

/// Read an SSE response body, buffer tokens, and flush periodically.
async fn read_sse_stream(
    mut response: reqwest::Response,
    on_tokens: &mut (impl FnMut(&str) + Send),
) -> Result<String, VaaniError> {
    let mut full_text = String::new();
    let mut token_buffer = String::new();
    let mut last_flush = Instant::now();
    let mut line_buffer = String::new();
    let flush_interval = std::time::Duration::from_millis(TOKEN_FLUSH_INTERVAL_MS);

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| VaaniError::Enhance(format!("stream read error: {e}")))?
    {
        let chunk_str = String::from_utf8_lossy(&chunk);
        line_buffer.push_str(&chunk_str);

        // Process all complete lines in the buffer.
        while let Some(newline_pos) = line_buffer.find('\n') {
            let line = line_buffer[..newline_pos]
                .trim_end_matches('\r')
                .to_string();
            line_buffer = line_buffer[newline_pos + 1..].to_string();

            if let Some(text) = extract_sse_data_text(&line) {
                full_text.push_str(&text);
                token_buffer.push_str(&text);
            }
        }

        // Flush token buffer if enough time has elapsed.
        if !token_buffer.is_empty() && last_flush.elapsed() >= flush_interval {
            on_tokens(&token_buffer);
            token_buffer.clear();
            last_flush = Instant::now();
        }
    }

    // Flush any remaining tokens.
    if !token_buffer.is_empty() {
        on_tokens(&token_buffer);
    }

    tracing::debug!(chars = full_text.len(), "streaming enhancement complete");
    Ok(full_text)
}

// ---------------------------------------------------------------------------
// SSE parsing helper
// ---------------------------------------------------------------------------

/// Extract text content from a single SSE line, if it contains a text delta.
///
/// Lines that are not `data: ` prefixed, or that contain non-text events,
/// return `None`.
fn extract_sse_data_text(line: &str) -> Option<String> {
    let data = line.strip_prefix("data: ")?;
    parse_sse_text_delta(data)
}

/// Parse a JSON `data:` payload from the SSE stream.
///
/// Returns `Some(text)` if the event is a `content_block_delta` with a
/// `text_delta` delta. Returns `None` for all other events (message_start,
/// content_block_start, message_stop, ping, etc.) and for malformed JSON.
fn parse_sse_text_delta(data: &str) -> Option<String> {
    let event: SseEvent = serde_json::from_str(data).ok()?;

    if event.event_type != "content_block_delta" {
        return None;
    }

    let delta = event.delta.as_ref()?;
    if delta.delta_type != "text_delta" {
        return None;
    }

    Some(delta.text.clone())
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Validate API key and input text before making a request.
fn validate_inputs(api_key: &str, text: &str) -> Result<(), VaaniError> {
    if api_key.is_empty() {
        return Err(VaaniError::MissingApiKey("Anthropic".into()));
    }
    if text.trim().is_empty() {
        return Err(VaaniError::Enhance("input text is empty".into()));
    }
    Ok(())
}

/// Send a POST request to the Anthropic Messages API.
async fn send_request(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    body: &AnthropicRequest<'_>,
) -> Result<reqwest::Response, VaaniError> {
    let response = client
        .post(url)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("content-type", "application/json")
        .json(body)
        .send()
        .await
        .map_err(|e| VaaniError::Enhance(format!("request failed: {e}")))?;

    Ok(response)
}

/// Extract all text content from an Anthropic response.
fn extract_text_from_response(response: &AnthropicResponse) -> String {
    response
        .content
        .iter()
        .filter(|block| block.block_type == "text")
        .map(|block| block.text.as_str())
        .collect::<Vec<_>>()
        .join("")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Input validation ----

    #[tokio::test]
    async fn missing_api_key_returns_error() {
        let client = reqwest::Client::new();
        let result = enhance(&client, "", "some text", "claude-haiku", "system").await;

        assert!(result.is_err());
        match result.unwrap_err() {
            VaaniError::MissingApiKey(provider) => {
                assert_eq!(provider, "Anthropic");
            }
            other => panic!("expected MissingApiKey, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn empty_text_returns_error() {
        let client = reqwest::Client::new();
        let result = enhance(&client, "sk-test-key", "", "claude-haiku", "system").await;

        assert!(result.is_err());
        match result.unwrap_err() {
            VaaniError::Enhance(msg) => {
                assert!(msg.contains("empty"), "expected 'empty' in message: {msg}");
            }
            other => panic!("expected Enhance error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn whitespace_only_text_returns_error() {
        let client = reqwest::Client::new();
        let result = enhance(
            &client,
            "sk-test-key",
            "   \n\t  ",
            "claude-haiku",
            "system",
        )
        .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            VaaniError::Enhance(msg) => {
                assert!(msg.contains("empty"), "expected 'empty' in message: {msg}");
            }
            other => panic!("expected Enhance error, got: {other:?}"),
        }
    }

    // ---- SSE parsing ----

    #[test]
    fn parse_sse_text_delta_valid() {
        let data = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
        let result = parse_sse_text_delta(data);
        assert_eq!(result, Some("Hello".to_string()));
    }

    #[test]
    fn parse_sse_text_delta_non_text_event() {
        let data = r#"{"type":"message_start","message":{"id":"msg_123"}}"#;
        let result = parse_sse_text_delta(data);
        assert_eq!(result, None);
    }

    #[test]
    fn parse_sse_text_delta_invalid_json() {
        let result = parse_sse_text_delta("this is not json at all");
        assert_eq!(result, None);
    }

    #[test]
    fn parse_sse_text_delta_message_stop() {
        let data = r#"{"type":"message_stop"}"#;
        let result = parse_sse_text_delta(data);
        assert_eq!(result, None);
    }

    #[test]
    fn parse_sse_text_delta_content_block_start() {
        let data =
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#;
        let result = parse_sse_text_delta(data);
        assert_eq!(result, None);
    }

    #[test]
    fn parse_sse_text_delta_with_special_characters() {
        let data = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello \"world\" \n\ttab"}}"#;
        let result = parse_sse_text_delta(data);
        assert_eq!(result, Some("Hello \"world\" \n\ttab".to_string()));
    }

    // ---- extract_sse_data_text ----

    #[test]
    fn extract_sse_data_text_strips_prefix() {
        let line = r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}"#;
        let result = extract_sse_data_text(line);
        assert_eq!(result, Some("Hi".to_string()));
    }

    #[test]
    fn extract_sse_data_text_ignores_non_data_lines() {
        assert_eq!(extract_sse_data_text("event: content_block_delta"), None);
        assert_eq!(extract_sse_data_text(""), None);
        assert_eq!(extract_sse_data_text(": comment"), None);
    }

    // ---- Response deserialization ----

    #[test]
    fn anthropic_response_deserializes() {
        let json = r#"{
            "content": [{"type": "text", "text": "Enhanced text here"}],
            "stop_reason": "end_turn",
            "id": "msg_123",
            "model": "claude-3-haiku"
        }"#;
        let resp: AnthropicResponse =
            serde_json::from_str(json).expect("deserialization should succeed");
        assert_eq!(resp.content.len(), 1);
        assert_eq!(resp.content[0].block_type, "text");
        assert_eq!(resp.content[0].text, "Enhanced text here");
    }

    #[test]
    fn anthropic_response_extracts_text() {
        let response = AnthropicResponse {
            content: vec![
                ContentBlock {
                    block_type: "text".to_string(),
                    text: "Hello ".to_string(),
                },
                ContentBlock {
                    block_type: "text".to_string(),
                    text: "world!".to_string(),
                },
            ],
        };
        let result = extract_text_from_response(&response);
        assert_eq!(result, "Hello world!");
    }

    #[test]
    fn anthropic_response_skips_non_text_blocks() {
        let response = AnthropicResponse {
            content: vec![
                ContentBlock {
                    block_type: "text".to_string(),
                    text: "Hello".to_string(),
                },
                ContentBlock {
                    block_type: "tool_use".to_string(),
                    text: String::new(),
                },
            ],
        };
        let result = extract_text_from_response(&response);
        assert_eq!(result, "Hello");
    }

    // ---- Error variants ----

    #[test]
    fn enhance_error_variants_are_constructible() {
        let _enhance = VaaniError::Enhance("test".into());
        let _missing = VaaniError::MissingApiKey("Anthropic".into());
    }

    // ---- Validate inputs ----

    #[test]
    fn validate_inputs_accepts_valid_input() {
        let result = validate_inputs("sk-key", "hello");
        assert!(result.is_ok());
    }

    #[test]
    fn validate_inputs_rejects_empty_key() {
        let result = validate_inputs("", "hello");
        assert!(result.is_err());
    }

    #[test]
    fn validate_inputs_rejects_empty_text() {
        let result = validate_inputs("sk-key", "");
        assert!(result.is_err());
    }

    // ---- Request serialization ----

    #[test]
    fn anthropic_request_serializes_without_stream_when_false() {
        let req = AnthropicRequest {
            model: "claude-3-haiku",
            max_tokens: 4096,
            system: "You are helpful.",
            messages: vec![Message {
                role: "user",
                content: "Hello",
            }],
            stream: false,
        };
        let json = serde_json::to_value(&req).expect("serialization should succeed");
        assert!(
            json.get("stream").is_none(),
            "stream: false should be omitted"
        );
        assert_eq!(json["model"], "claude-3-haiku");
        assert_eq!(json["max_tokens"], 4096);
        assert_eq!(json["messages"][0]["role"], "user");
    }

    #[test]
    fn anthropic_request_serializes_with_stream_when_true() {
        let req = AnthropicRequest {
            model: "claude-3-haiku",
            max_tokens: 4096,
            system: "You are helpful.",
            messages: vec![Message {
                role: "user",
                content: "Hello",
            }],
            stream: true,
        };
        let json = serde_json::to_value(&req).expect("serialization should succeed");
        assert_eq!(json["stream"], true);
    }
}
