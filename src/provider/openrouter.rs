use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;

use crate::config::{Config, ExpandResponseMode};

use super::{
    CompletionEventHandler, CompletionRequest, CompletionResponse, Provider,
    StructuredExpandRequest, StructuredExpandResponse, Usage,
};

const EXPAND_TOOL_NAME: &str = "emit_expand_result";

pub struct OpenRouterProvider {
    client: reqwest::Client,
    endpoint: String,
    api_key: String,
    model: String,
    fallback_model: Option<String>,
    expand_response_mode: ExpandResponseMode,
}

impl OpenRouterProvider {
    pub fn new(config: Config) -> Result<Self> {
        let api_key = config
            .provider_api_key()
            .map(ToString::to_string)
            .ok_or_else(|| anyhow!("missing OpenRouter API key"))?;

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {api_key}"))
                .context("failed to build authorization header")?,
        );
        headers.insert("X-Title", HeaderValue::from_static("lmcomplete"));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .context("failed to build HTTP client")?;

        Ok(Self {
            client,
            endpoint: config.provider.base_url,
            api_key,
            model: config.provider.model,
            fallback_model: config.provider.fallback.map(|value| value.model),
            expand_response_mode: config.expand.response_mode,
        })
    }

    async fn complete_with_model(
        &self,
        model: &str,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse> {
        let body = build_openrouter_request(model, request, None);

        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("failed to send request to OpenRouter")?;

        let status = response.status();
        let raw = response
            .text()
            .await
            .context("failed to read OpenRouter response body")?;

        if !status.is_success() {
            bail!("OpenRouter returned {status}: {raw}");
        }

        let parsed: OpenRouterResponse =
            serde_json::from_str(&raw).context("failed to parse OpenRouter response")?;
        let content = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message.content)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("OpenRouter returned an empty completion"))?;

        Ok(CompletionResponse {
            content,
            usage: parsed.usage.unwrap_or_default().into(),
        })
    }

    async fn expand_with_model(
        &self,
        model: &str,
        request: &StructuredExpandRequest,
    ) -> Result<StructuredExpandResponse> {
        let body = build_structured_expand_request(model, request, self.expand_response_mode);

        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("failed to send request to OpenRouter")?;

        let status = response.status();
        let raw = response
            .text()
            .await
            .context("failed to read OpenRouter response body")?;

        if !status.is_success() {
            bail!("OpenRouter returned {status}: {raw}");
        }

        parse_structured_expand_response(&raw, self.expand_response_mode)
    }

    async fn stream_with_model(
        &self,
        model: &str,
        request: &CompletionRequest,
        handler: &mut dyn CompletionEventHandler,
    ) -> std::result::Result<CompletionResponse, StreamAttemptError> {
        let body = build_openrouter_request(model, request, Some(true));

        let mut response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("failed to send request to OpenRouter")
            .map_err(StreamAttemptError::pre_stream)?;

        let status = response.status();
        if !status.is_success() {
            let raw = response
                .text()
                .await
                .context("failed to read OpenRouter response body")
                .map_err(StreamAttemptError::pre_stream)?;
            return Err(StreamAttemptError::pre_stream(anyhow!(
                "OpenRouter returned {status}: {raw}"
            )));
        }

        let mut parser = SseParser::default();
        let mut content = String::new();
        let mut usage = Usage::default();
        let mut started = false;

        while let Some(chunk) = response
            .chunk()
            .await
            .context("failed to read OpenRouter stream chunk")
            .map_err(|error| StreamAttemptError::new(error, started))?
        {
            let events = parser
                .push(&chunk)
                .map_err(|error| StreamAttemptError::new(error, started))?;

            for data in events {
                if data == "[DONE]" {
                    continue;
                }

                let parsed: OpenRouterStreamResponse = serde_json::from_str(&data)
                    .context("failed to parse OpenRouter stream chunk")
                    .map_err(|error| StreamAttemptError::new(error, started))?;

                if let Some(error) = parsed.error {
                    return Err(StreamAttemptError::new(
                        anyhow!("OpenRouter stream error: {}", error.message),
                        started,
                    ));
                }

                if let Some(delta) = parsed
                    .choices
                    .into_iter()
                    .next()
                    .and_then(|choice| choice.delta)
                    .and_then(|delta| delta.content)
                    .filter(|value| !value.is_empty())
                {
                    started = true;
                    content.push_str(&delta);
                    handler
                        .on_content(&delta)
                        .map_err(|error| StreamAttemptError::new(error, true))?;
                }

                if let Some(event_usage) = parsed.usage {
                    usage = event_usage.into();
                }
            }
        }

        parser
            .finish()
            .map_err(|error| StreamAttemptError::new(error, started))?;

        if content.trim().is_empty() {
            return Err(StreamAttemptError::new(
                anyhow!("OpenRouter returned an empty completion"),
                started,
            ));
        }

        Ok(CompletionResponse { content, usage })
    }
}

#[async_trait]
impl Provider for OpenRouterProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        match self.complete_with_model(&self.model, &request).await {
            Ok(response) => Ok(response),
            Err(primary_error) => {
                let Some(fallback_model) = &self.fallback_model else {
                    return Err(primary_error);
                };
                if !should_use_fallback_model(&self.model, false) {
                    return Err(primary_error);
                }

                self.complete_with_model(fallback_model, &request)
                    .await
                    .with_context(|| {
                        format!(
                            "primary model '{}' failed and fallback model '{}' also failed: {primary_error:#}",
                            self.model, fallback_model
                        )
                    })
            }
        }
    }

    async fn expand(&self, request: StructuredExpandRequest) -> Result<StructuredExpandResponse> {
        match self.expand_with_model(&self.model, &request).await {
            Ok(response) => Ok(response),
            Err(primary_error) => {
                let Some(fallback_model) = &self.fallback_model else {
                    return Err(primary_error);
                };
                if !should_use_fallback_model(&self.model, false) {
                    return Err(primary_error);
                }

                self.expand_with_model(fallback_model, &request)
                    .await
                    .with_context(|| {
                        format!(
                            "primary model '{}' failed and fallback model '{}' also failed: {primary_error:#}",
                            self.model, fallback_model
                        )
                    })
            }
        }
    }

    async fn stream(
        &self,
        request: CompletionRequest,
        handler: &mut dyn CompletionEventHandler,
    ) -> Result<CompletionResponse> {
        match self.stream_with_model(&self.model, &request, handler).await {
            Ok(response) => Ok(response),
            Err(primary_error) => {
                let Some(fallback_model) = &self.fallback_model else {
                    return Err(primary_error.error);
                };

                if !should_use_fallback_model(&self.model, primary_error.started) {
                    return Err(primary_error.error);
                }

                self.stream_with_model(fallback_model, &request, handler)
                    .await
                    .map_err(|fallback_error| {
                        anyhow!(
                            "primary model '{}' failed and fallback model '{}' also failed: {:#}; fallback error: {:#}",
                            self.model,
                            fallback_model,
                            primary_error.error,
                            fallback_error.error
                        )
                    })
            }
        }
    }
}

#[derive(Debug, Serialize)]
struct OpenRouterRequest {
    model: String,
    messages: Vec<Message>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<OpenRouterProviderPreferences>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<ToolChoice>,
}

#[derive(Debug, Clone, Serialize)]
struct Message {
    role: &'static str,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenRouterResponse {
    choices: Vec<OpenRouterChoice>,
    usage: Option<OpenRouterUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterChoice {
    message: OpenRouterMessage,
}

#[derive(Debug, Deserialize)]
struct OpenRouterMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OpenRouterToolCall>>,
}

#[derive(Debug, Default, Deserialize)]
struct OpenRouterUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
    cost: Option<f64>,
}

#[derive(Debug, Serialize)]
struct OpenRouterProviderPreferences {
    only: Vec<String>,
    allow_fallbacks: bool,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamResponse {
    choices: Vec<OpenRouterStreamChoice>,
    usage: Option<OpenRouterUsage>,
    error: Option<OpenRouterStreamError>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamChoice {
    delta: Option<OpenRouterStreamDelta>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamDelta {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamError {
    message: String,
}

#[derive(Debug, Deserialize)]
struct OpenRouterToolCall {
    function: OpenRouterFunctionCall,
}

#[derive(Debug, Deserialize)]
struct OpenRouterFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct ExpandPayload {
    command: String,
    explanation: String,
}

#[derive(Debug, Serialize)]
struct ToolDefinition {
    #[serde(rename = "type")]
    tool_type: &'static str,
    function: ToolFunction,
}

#[derive(Debug, Serialize)]
struct ToolFunction {
    name: &'static str,
    description: &'static str,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct ToolChoice {
    #[serde(rename = "type")]
    choice_type: &'static str,
    function: ToolChoiceFunction,
}

#[derive(Debug, Serialize)]
struct ToolChoiceFunction {
    name: &'static str,
}

impl From<OpenRouterUsage> for Usage {
    fn from(value: OpenRouterUsage) -> Self {
        Self {
            prompt_tokens: value.prompt_tokens,
            completion_tokens: value.completion_tokens,
            total_tokens: value.total_tokens,
            cost: value.cost,
        }
    }
}

#[derive(Debug)]
struct StreamAttemptError {
    error: anyhow::Error,
    started: bool,
}

#[derive(Debug)]
struct RoutedModel {
    model: String,
    provider: Option<OpenRouterProviderPreferences>,
}

impl RoutedModel {
    fn new(model: &str) -> Self {
        let Some((base_model, provider_slug)) = split_provider_suffix(model) else {
            return Self {
                model: model.to_string(),
                provider: None,
            };
        };

        Self {
            model: base_model.to_string(),
            provider: Some(OpenRouterProviderPreferences {
                only: vec![provider_slug.to_string()],
                allow_fallbacks: false,
            }),
        }
    }
}

fn split_provider_suffix(model: &str) -> Option<(&str, &str)> {
    let (base_model, suffix) = model.rsplit_once(':')?;
    if matches!(suffix, "nitro" | "floor") {
        return None;
    }
    if suffix.is_empty() || base_model.is_empty() {
        return None;
    }

    Some((base_model, suffix))
}

fn should_use_fallback_model(primary_model: &str, started_streaming: bool) -> bool {
    if started_streaming {
        return false;
    }

    split_provider_suffix(primary_model).is_none()
}

fn build_openrouter_request(
    model: &str,
    request: &CompletionRequest,
    stream: Option<bool>,
) -> OpenRouterRequest {
    let routing = RoutedModel::new(model);
    OpenRouterRequest {
        model: routing.model,
        messages: vec![
            Message {
                role: "system",
                content: request.system_prompt.clone(),
            },
            Message {
                role: "user",
                content: request.user_prompt.clone(),
            },
        ],
        max_tokens: Some(256),
        temperature: Some(0.0),
        stream,
        provider: routing.provider,
        tools: None,
        tool_choice: None,
    }
}

fn build_structured_expand_request(
    model: &str,
    request: &StructuredExpandRequest,
    response_mode: ExpandResponseMode,
) -> OpenRouterRequest {
    let routing = RoutedModel::new(model);
    let (tools, tool_choice) = match response_mode {
        ExpandResponseMode::ToolCall => (
            Some(vec![expand_tool_definition()]),
            Some(expand_tool_choice()),
        ),
        ExpandResponseMode::MessageJson => (None, None),
    };

    OpenRouterRequest {
        model: routing.model,
        messages: vec![
            Message {
                role: "system",
                content: request.system_prompt.clone(),
            },
            Message {
                role: "user",
                content: request.user_prompt.clone(),
            },
        ],
        max_tokens: Some(256),
        temperature: Some(0.0),
        stream: None,
        provider: routing.provider,
        tools,
        tool_choice,
    }
}

fn parse_structured_expand_response(
    raw: &str,
    response_mode: ExpandResponseMode,
) -> Result<StructuredExpandResponse> {
    let parsed: OpenRouterResponse =
        serde_json::from_str(raw).context("failed to parse OpenRouter response")?;
    let usage = parsed.usage.unwrap_or_default().into();
    let choice = parsed
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("OpenRouter returned no completion choices"))?;

    let payload = match response_mode {
        ExpandResponseMode::ToolCall => parse_tool_call_payload(choice.message)?,
        ExpandResponseMode::MessageJson => parse_message_json_payload(choice.message)?,
    };

    let command = payload.command.trim();
    if command.is_empty() {
        bail!("OpenRouter returned an empty expand command");
    }

    let explanation = payload.explanation.trim();
    if explanation.is_empty() {
        bail!("OpenRouter returned an empty expand explanation");
    }

    Ok(StructuredExpandResponse {
        command: command.to_string(),
        explanation: explanation.to_string(),
        usage,
    })
}

fn parse_tool_call_payload(message: OpenRouterMessage) -> Result<ExpandPayload> {
    let tool_call = message
        .tool_calls
        .and_then(|calls| {
            calls
                .into_iter()
                .find(|call| call.function.name == EXPAND_TOOL_NAME)
        })
        .ok_or_else(|| anyhow!("OpenRouter did not return the required expand tool call"))?;

    serde_json::from_str(&tool_call.function.arguments)
        .context("failed to parse expand tool call arguments")
}

fn parse_message_json_payload(message: OpenRouterMessage) -> Result<ExpandPayload> {
    let content = message
        .content
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("OpenRouter returned an empty completion"))?;

    serde_json::from_str(&content).context("failed to parse expand JSON response")
}

fn expand_tool_definition() -> ToolDefinition {
    ToolDefinition {
        tool_type: "function",
        function: ToolFunction {
            name: EXPAND_TOOL_NAME,
            description: "Return a shell command and a short explanation for it.",
            parameters: json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Exactly one shell command on one line with no markdown or code fences."
                    },
                    "explanation": {
                        "type": "string",
                        "description": "One short plain-text sentence explaining what the command does."
                    }
                },
                "required": ["command", "explanation"]
            }),
        },
    }
}

fn expand_tool_choice() -> ToolChoice {
    ToolChoice {
        choice_type: "function",
        function: ToolChoiceFunction {
            name: EXPAND_TOOL_NAME,
        },
    }
}

impl StreamAttemptError {
    fn new(error: anyhow::Error, started: bool) -> Self {
        Self { error, started }
    }

    fn pre_stream(error: anyhow::Error) -> Self {
        Self::new(error, false)
    }
}

#[derive(Debug, Default)]
struct SseParser {
    buffer: Vec<u8>,
}

impl SseParser {
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>> {
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();

        while let Some((end, delimiter_len)) = find_event_boundary(&self.buffer) {
            let event = self.buffer.drain(..end + delimiter_len).collect::<Vec<_>>();
            let content = &event[..end];
            if let Some(data) = parse_sse_event(content)? {
                events.push(data);
            }
        }

        Ok(events)
    }

    fn finish(&self) -> Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        let trailing = String::from_utf8_lossy(&self.buffer);
        if trailing.trim().is_empty() {
            Ok(())
        } else {
            bail!("OpenRouter stream ended with an incomplete SSE event")
        }
    }
}

fn find_event_boundary(buffer: &[u8]) -> Option<(usize, usize)> {
    let mut index = 0;
    while index < buffer.len() {
        if index + 1 < buffer.len() && buffer[index] == b'\n' && buffer[index + 1] == b'\n' {
            return Some((index, 2));
        }

        if index + 3 < buffer.len()
            && buffer[index] == b'\r'
            && buffer[index + 1] == b'\n'
            && buffer[index + 2] == b'\r'
            && buffer[index + 3] == b'\n'
        {
            return Some((index, 4));
        }

        index += 1;
    }

    None
}

fn parse_sse_event(event: &[u8]) -> Result<Option<String>> {
    let text = std::str::from_utf8(event).context("OpenRouter SSE event was not valid UTF-8")?;
    let mut data = Vec::new();

    for raw_line in text.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with(':') {
            continue;
        }

        if let Some(rest) = line.strip_prefix("data:") {
            data.push(rest.trim_start().to_string());
        }
    }

    if data.is_empty() {
        Ok(None)
    } else {
        Ok(Some(data.join("\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CompletionRequest, OpenRouterStreamResponse, OpenRouterUsage, RoutedModel, SseParser,
        build_openrouter_request, find_event_boundary, parse_message_json_payload, parse_sse_event,
        parse_structured_expand_response, parse_tool_call_payload, should_use_fallback_model,
        split_provider_suffix,
    };
    use crate::config::ExpandResponseMode;
    use crate::provider::Usage;

    #[test]
    fn converts_usage() {
        let usage = Usage::from(OpenRouterUsage {
            prompt_tokens: Some(10),
            completion_tokens: Some(20),
            total_tokens: Some(30),
            cost: Some(0.42),
        });
        assert_eq!(usage.total_tokens, Some(30));
        assert_eq!(usage.cost, Some(0.42));
    }

    #[test]
    fn parses_sse_comments_and_content_chunks() {
        let mut parser = SseParser::default();
        let mut events = parser.push(b": OPENROUTER PROCESSING\n\n").unwrap();
        assert!(events.is_empty());

        events.extend(
            parser
                .push(
                    br#"data: {"choices":[{"delta":{"content":"git "}}]}

data: {"choices":[{"delta":{"content":"status"}}]}

"#,
                )
                .unwrap(),
        );

        assert_eq!(events.len(), 2);
        let first: OpenRouterStreamResponse = serde_json::from_str(&events[0]).unwrap();
        let second: OpenRouterStreamResponse = serde_json::from_str(&events[1]).unwrap();
        assert_eq!(
            first.choices[0].delta.as_ref().unwrap().content.as_deref(),
            Some("git ")
        );
        assert_eq!(
            second.choices[0].delta.as_ref().unwrap().content.as_deref(),
            Some("status")
        );
    }

    #[test]
    fn captures_usage_in_final_stream_chunk() {
        let event = parse_sse_event(
            br#"data: {"choices":[{"delta":{"content":""}}],"usage":{"prompt_tokens":1,"completion_tokens":2,"total_tokens":3,"cost":0.5}}
"#,
        )
        .unwrap()
        .unwrap();

        let parsed: OpenRouterStreamResponse = serde_json::from_str(&event).unwrap();
        let usage = Usage::from(parsed.usage.unwrap());
        assert_eq!(usage.prompt_tokens, Some(1));
        assert_eq!(usage.completion_tokens, Some(2));
        assert_eq!(usage.total_tokens, Some(3));
        assert_eq!(usage.cost, Some(0.5));
    }

    #[test]
    fn detects_mixed_newline_boundaries() {
        assert_eq!(find_event_boundary(b"data: 1\n\ndata: 2"), Some((7, 2)));
        assert_eq!(find_event_boundary(b"data: 1\r\n\r\ndata: 2"), Some((7, 4)));
    }

    #[test]
    fn joins_multiline_sse_data_fields() {
        let event = parse_sse_event(b"data: first\ndata: second\n\n")
            .unwrap()
            .unwrap();
        assert_eq!(event, "first\nsecond");
    }

    #[test]
    fn routes_provider_suffixes_via_provider_preferences() {
        let routed = RoutedModel::new("openai/gpt-oss-120b:groq");
        assert_eq!(routed.model, "openai/gpt-oss-120b");
        let provider = routed.provider.unwrap();
        assert_eq!(provider.only, vec!["groq"]);
        assert!(!provider.allow_fallbacks);
    }

    #[test]
    fn preserves_openrouter_shortcuts() {
        assert_eq!(split_provider_suffix("openai/gpt-oss-120b:nitro"), None);
        assert_eq!(split_provider_suffix("openai/gpt-oss-120b:floor"), None);
    }

    #[test]
    fn disables_model_fallback_for_provider_pinned_models() {
        assert!(!should_use_fallback_model(
            "openai/gpt-oss-120b:groq",
            false
        ));
        assert!(!should_use_fallback_model("openai/gpt-oss-120b", true));
        assert!(should_use_fallback_model("openai/gpt-oss-120b", false));
    }

    #[test]
    fn uses_zero_temperature() {
        let request = build_openrouter_request(
            "openai/gpt-oss-120b:groq",
            &CompletionRequest {
                system_prompt: "system".into(),
                user_prompt: "user".into(),
            },
            Some(true),
        );

        assert_eq!(request.temperature, Some(0.0));
    }

    #[test]
    fn parses_structured_expand_tool_call_response() {
        let response = parse_structured_expand_response(
            r#"{
  "choices": [{
    "message": {
      "content": null,
      "tool_calls": [{
        "function": {
          "name": "emit_expand_result",
          "arguments": "{\"command\":\"git status\",\"explanation\":\"Shows the current repository status.\"}"
        }
      }]
    }
  }],
  "usage": { "total_tokens": 12 }
}"#,
            ExpandResponseMode::ToolCall,
        )
        .unwrap();

        assert_eq!(response.command, "git status");
        assert_eq!(response.explanation, "Shows the current repository status.");
        assert_eq!(response.usage.total_tokens, Some(12));
    }

    #[test]
    fn rejects_missing_tool_call_in_tool_call_mode() {
        let error = parse_structured_expand_response(
            r#"{
  "choices": [{
    "message": {
      "content": "{\"command\":\"git status\",\"explanation\":\"Shows status.\"}"
    }
  }]
}"#,
            ExpandResponseMode::ToolCall,
        )
        .unwrap_err();

        assert!(error.to_string().contains("required expand tool call"));
    }

    #[test]
    fn parses_message_json_expand_response() {
        let response = parse_structured_expand_response(
            r#"{
  "choices": [{
    "message": {
      "content": "{\"command\":\"git status\",\"explanation\":\"Shows the current repository status.\"}"
    }
  }]
}"#,
            ExpandResponseMode::MessageJson,
        )
        .unwrap();

        assert_eq!(response.command, "git status");
        assert_eq!(response.explanation, "Shows the current repository status.");
    }

    #[test]
    fn rejects_invalid_json_content_in_message_mode() {
        let error = parse_structured_expand_response(
            r#"{
  "choices": [{
    "message": {
      "content": "git status"
    }
  }]
}"#,
            ExpandResponseMode::MessageJson,
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("failed to parse expand JSON response")
        );
    }

    #[test]
    fn parses_tool_call_payload_from_first_matching_call() {
        let payload = parse_tool_call_payload(
            serde_json::from_str(
                r#"{
  "content": null,
  "tool_calls": [
    {
      "function": {
        "name": "ignored",
        "arguments": "{\"command\":\"false\",\"explanation\":\"Ignore.\"}"
      }
    },
    {
      "function": {
        "name": "emit_expand_result",
        "arguments": "{\"command\":\"git status\",\"explanation\":\"Shows status.\"}"
      }
    }
  ]
}"#,
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(payload.command, "git status");
    }

    #[test]
    fn parses_message_json_payload() {
        let payload = parse_message_json_payload(
            serde_json::from_str(
                r#"{
  "content": "{\"command\":\"git status\",\"explanation\":\"Shows status.\"}"
}"#,
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(payload.explanation, "Shows status.");
    }
}
