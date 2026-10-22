use crate::prompt::Prompt;
use crate::settings::RuntimeSettings;
use anyhow::{Context, Result, anyhow};
use reqwest::StatusCode;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::error::Error;
use std::io;
use std::time::Duration;

#[derive(Clone)]
pub struct ResponsesClient {
    http: reqwest::Client,
}

impl ResponsesClient {
    pub fn new(timeout_ms: u64) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .context("failed to build HTTP client")?;
        Ok(Self { http })
    }

    pub async fn create_text_response(
        &self,
        settings: &RuntimeSettings,
        prompt: &Prompt,
    ) -> Result<String> {
        Ok(self
            .create_response(settings, prompt, settings.max_output_tokens, None, false)
            .await?
            .text)
    }

    pub async fn create_response(
        &self,
        settings: &RuntimeSettings,
        prompt: &Prompt,
        max_output_tokens: u32,
        previous_response_id: Option<&str>,
        store: bool,
    ) -> Result<ResponseOutput> {
        let mut attempt = 0;
        let mut backoff = Duration::from_millis(settings.retry_backoff_ms);

        loop {
            match self
                .try_create_response(
                    settings,
                    prompt,
                    max_output_tokens,
                    previous_response_id,
                    store,
                )
                .await
            {
                Ok(output) => return Ok(output),
                Err(error) if error.is_retryable() && attempt < settings.retry_count => {
                    attempt += 1;
                    eprintln!(
                        "distill: API request failed, retrying in {}ms (attempt {}/{})",
                        backoff.as_millis(),
                        attempt,
                        settings.retry_count
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = backoff.saturating_mul(2);
                }
                Err(error) => return error.into_result(),
            }
        }
    }

    async fn try_create_response(
        &self,
        settings: &RuntimeSettings,
        prompt: &Prompt,
        max_output_tokens: u32,
        previous_response_id: Option<&str>,
        store: bool,
    ) -> ResponseAttempt {
        let url = responses_url(&settings.api_base)?;
        let request = ResponsesRequest {
            model: &settings.model,
            reasoning: settings
                .reasoning_effort
                .as_ref()
                .map(|effort| Reasoning { effort }),
            instructions: &prompt.instructions,
            input: &prompt.input,
            max_output_tokens,
            previous_response_id,
            store,
        };

        let response = self
            .http
            .post(url)
            .bearer_auth(&settings.api_key)
            .json(&request)
            .send()
            .await
            .map_err(ResponseError::Request)?;

        let status = response.status();
        let payload = response.text().await.map_err(ResponseError::Read)?;

        if !status.is_success() {
            return Err(ResponseError::Status { status, payload });
        }

        extract_response_output(&payload).map_err(ResponseError::Payload)
    }
}

type ResponseAttempt = std::result::Result<ResponseOutput, ResponseError>;

#[derive(Debug)]
enum ResponseError {
    Request(reqwest::Error),
    Read(reqwest::Error),
    Status { status: StatusCode, payload: String },
    Payload(anyhow::Error),
    Other(anyhow::Error),
}

impl ResponseError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Request(error) | Self::Read(error) => {
                !contains_permission_denied(error)
                    && (error.is_timeout() || error.is_connect() || error.is_request())
            }
            Self::Status { status, .. } => matches!(
                *status,
                StatusCode::REQUEST_TIMEOUT
                    | StatusCode::CONFLICT
                    | StatusCode::TOO_MANY_REQUESTS
                    | StatusCode::INTERNAL_SERVER_ERROR
                    | StatusCode::BAD_GATEWAY
                    | StatusCode::SERVICE_UNAVAILABLE
                    | StatusCode::GATEWAY_TIMEOUT
            ),
            Self::Payload(_) | Self::Other(_) => false,
        }
    }

    fn into_result<T>(self) -> Result<T> {
        match self {
            Self::Request(error) if contains_permission_denied(&error) => {
                anyhow::bail!("distill: error: network access denied")
            }
            Self::Request(error) => Err(error).context("request to Responses API failed"),
            Self::Read(error) => Err(error).context("failed to read response"),
            Self::Status { status, payload } => {
                anyhow::bail!("Responses API returned {status}: {payload}")
            }
            Self::Payload(error) | Self::Other(error) => Err(error),
        }
    }
}

fn contains_permission_denied(error: &(dyn Error + 'static)) -> bool {
    let mut current = Some(error);

    while let Some(error) = current {
        if let Some(io_error) = error.downcast_ref::<io::Error>() {
            return io_error.kind() == io::ErrorKind::PermissionDenied;
        }
        current = error.source();
    }

    false
}

impl From<anyhow::Error> for ResponseError {
    fn from(error: anyhow::Error) -> Self {
        Self::Other(error)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ResponseOutput {
    pub id: Option<String>,
    pub text: String,
}

#[derive(Debug, Serialize)]
struct ResponsesRequest<'a> {
    model: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<Reasoning<'a>>,
    instructions: &'a str,
    input: &'a str,
    max_output_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_response_id: Option<&'a str>,
    store: bool,
}

#[derive(Debug, Serialize)]
struct Reasoning<'a> {
    effort: &'a str,
}

#[cfg(test)]
fn responses_request_json(
    model: &str,
    reasoning_effort: Option<&str>,
    prompt: &Prompt,
    max_output_tokens: u32,
    previous_response_id: Option<&str>,
    store: bool,
) -> Result<Value> {
    let request = ResponsesRequest {
        model,
        reasoning: reasoning_effort.map(|effort| Reasoning { effort }),
        instructions: &prompt.instructions,
        input: &prompt.input,
        max_output_tokens,
        previous_response_id,
        store,
    };

    serde_json::to_value(request).context("failed to serialize Responses request")
}

pub fn responses_url(api_base: &str) -> Result<Url> {
    let mut url = Url::parse(api_base).with_context(|| format!("invalid api-base: {api_base}"))?;
    let path = url.path().trim_end_matches('/');
    url.set_path(&format!("{path}/responses"));
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

pub fn extract_output_text(raw: &str) -> Result<String> {
    Ok(extract_response_output(raw)?.text)
}

pub fn extract_response_output(raw: &str) -> Result<ResponseOutput> {
    let payload: ResponsesPayload =
        serde_json::from_str(raw).context("Responses API returned invalid JSON")?;

    if let Some(output_text) = payload.output_text {
        let trimmed = output_text.trim();
        if !trimmed.is_empty() {
            return Ok(ResponseOutput {
                id: payload.id,
                text: trimmed.to_string(),
            });
        }
    }

    let value: Value = serde_json::from_str(raw).context("Responses API returned invalid JSON")?;
    let text = value
        .get("output")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("content").and_then(Value::as_array))
        .flatten()
        .filter_map(|content| content.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("");

    let trimmed = text.trim();
    if trimmed.is_empty() {
        Err(anyhow!("Responses API returned no output text"))
    } else {
        Ok(ResponseOutput {
            id: payload.id,
            text: trimmed.to_string(),
        })
    }
}

#[derive(Debug, Deserialize)]
struct ResponsesPayload {
    id: Option<String>,
    output_text: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_responses_url_from_api_base() {
        assert_eq!(
            responses_url("https://api.openai.com/v1")
                .unwrap()
                .to_string(),
            "https://api.openai.com/v1/responses"
        );
    }

    #[test]
    fn extracts_output_text_field() {
        assert_eq!(
            extract_output_text(r#"{"output_text":"  compact answer  "}"#).unwrap(),
            "compact answer"
        );
    }

    #[test]
    fn extracts_response_id_with_output_text() {
        assert_eq!(
            extract_response_output(r#"{"id":"resp_123","output_text":"  compact answer  "}"#)
                .unwrap(),
            ResponseOutput {
                id: Some("resp_123".to_string()),
                text: "compact answer".to_string(),
            }
        );
    }

    #[test]
    fn serializes_previous_response_id_when_present() {
        let prompt = Prompt {
            instructions: "instructions".to_string(),
            input: "input".to_string(),
        };
        let value =
            responses_request_json("model", None, &prompt, 128, Some("resp_previous"), true)
                .unwrap();

        assert_eq!(value["previous_response_id"], "resp_previous");
        assert_eq!(value["max_output_tokens"], 128);
        assert_eq!(value["store"], true);
    }

    #[test]
    fn omits_previous_response_id_when_absent() {
        let prompt = Prompt {
            instructions: "instructions".to_string(),
            input: "input".to_string(),
        };
        let value = responses_request_json("model", None, &prompt, 128, None, false).unwrap();

        assert!(value.get("previous_response_id").is_none());
        assert!(value.get("reasoning").is_none());
        assert_eq!(value["store"], false);
    }

    #[test]
    fn serializes_reasoning_effort_when_present() {
        let prompt = Prompt {
            instructions: "instructions".to_string(),
            input: "input".to_string(),
        };
        let value =
            responses_request_json("model", Some("future-effort"), &prompt, 128, None, false)
                .unwrap();

        assert_eq!(value["reasoning"]["effort"], "future-effort");
    }

    #[test]
    fn detects_permission_denied_in_error_chain() {
        let error = anyhow::anyhow!(std::io::Error::from(std::io::ErrorKind::PermissionDenied));

        assert!(contains_permission_denied(error.as_ref()));
    }

    #[test]
    fn ignores_non_permission_denied_io_errors() {
        let error = anyhow::anyhow!(std::io::Error::from(std::io::ErrorKind::ConnectionRefused));

        assert!(!contains_permission_denied(error.as_ref()));
    }
}
