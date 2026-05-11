use crate::prompt::Prompt;
use crate::settings::RuntimeSettings;
use anyhow::{Context, Result, anyhow};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
        let url = responses_url(&settings.api_base)?;
        let request = ResponsesRequest {
            model: &settings.model,
            instructions: &prompt.instructions,
            input: &prompt.input,
            max_output_tokens: settings.max_output_tokens,
        };

        let response = self
            .http
            .post(url)
            .bearer_auth(&settings.api_key)
            .json(&request)
            .send()
            .await
            .context("request to Responses API failed")?;

        let status = response.status();
        let payload = response.text().await.context("failed to read response")?;

        if !status.is_success() {
            anyhow::bail!("Responses API returned {status}: {payload}");
        }

        extract_output_text(&payload)
    }
}

#[derive(Debug, Serialize)]
struct ResponsesRequest<'a> {
    model: &'a str,
    instructions: &'a str,
    input: &'a str,
    max_output_tokens: u32,
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
    let payload: ResponsesPayload =
        serde_json::from_str(raw).context("Responses API returned invalid JSON")?;

    if let Some(output_text) = payload.output_text {
        let trimmed = output_text.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
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
        Ok(trimmed.to_string())
    }
}

#[derive(Debug, Deserialize)]
struct ResponsesPayload {
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
}
