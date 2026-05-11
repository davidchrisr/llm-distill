use crate::cli::CliOverrides;
use serde::{Deserialize, Serialize};
use std::env;
use std::ffi::OsString;

pub const DEFAULT_MODEL: &str = "gpt-5.4-mini";
pub const DEFAULT_API_BASE: &str = "https://api.openai.com/v1";
pub const DEFAULT_TIMEOUT_MS: u64 = 90_000;
pub const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 512;
pub const DEFAULT_MAX_INPUT_CHARS: usize = 24_000;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PersistedSettings {
    pub model: Option<String>,
    pub api_base: Option<String>,
    pub api_key: Option<String>,
    pub timeout_ms: Option<u64>,
    pub max_output_tokens: Option<u32>,
    pub max_input_chars: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct EnvSettings {
    pub model: Option<String>,
    pub api_base: Option<String>,
    pub api_key: Option<String>,
    pub timeout_ms: Option<u64>,
    pub max_output_tokens: Option<u32>,
    pub max_input_chars: Option<usize>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RuntimeSettings {
    pub model: String,
    pub api_base: String,
    pub api_key: String,
    pub timeout_ms: u64,
    pub max_output_tokens: u32,
    pub max_input_chars: usize,
}

impl EnvSettings {
    pub fn current() -> Self {
        Self::from_pairs(env::vars_os())
    }

    pub fn from_pairs<I, K, V>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<OsString>,
        V: Into<OsString>,
    {
        let mut env = Self::default();

        for (key, value) in pairs {
            let key = key.into();
            let value = value.into().to_string_lossy().trim().to_string();

            if value.is_empty() {
                continue;
            }

            match key.to_string_lossy().as_ref() {
                "DISTILL_MODEL" => env.model = Some(value),
                "DISTILL_API_BASE" => env.api_base = Some(value),
                "DISTILL_API_KEY" => env.api_key = Some(value),
                "DISTILL_TIMEOUT_MS" => env.timeout_ms = value.parse().ok(),
                "DISTILL_MAX_OUTPUT_TOKENS" => env.max_output_tokens = value.parse().ok(),
                "DISTILL_MAX_INPUT_CHARS" => env.max_input_chars = value.parse().ok(),
                _ => {}
            }
        }

        env
    }
}

pub fn resolve_settings(
    persisted: &PersistedSettings,
    env: &EnvSettings,
    cli: &CliOverrides,
) -> RuntimeSettings {
    RuntimeSettings {
        model: first_non_empty([
            cli.model.as_deref(),
            env.model.as_deref(),
            persisted.model.as_deref(),
            Some(DEFAULT_MODEL),
        ])
        .to_string(),
        api_base: trim_trailing_slash(first_non_empty([
            cli.api_base.as_deref(),
            env.api_base.as_deref(),
            persisted.api_base.as_deref(),
            Some(DEFAULT_API_BASE),
        ])),
        api_key: first_non_empty([
            cli.api_key.as_deref(),
            env.api_key.as_deref(),
            persisted.api_key.as_deref(),
            Some(""),
        ])
        .to_string(),
        timeout_ms: cli
            .timeout_ms
            .or(env.timeout_ms)
            .or(persisted.timeout_ms)
            .unwrap_or(DEFAULT_TIMEOUT_MS),
        max_output_tokens: cli
            .max_output_tokens
            .or(env.max_output_tokens)
            .or(persisted.max_output_tokens)
            .unwrap_or(DEFAULT_MAX_OUTPUT_TOKENS),
        max_input_chars: cli
            .max_input_chars
            .or(env.max_input_chars)
            .or(persisted.max_input_chars)
            .unwrap_or(DEFAULT_MAX_INPUT_CHARS),
    }
}

fn first_non_empty<const N: usize>(values: [Option<&str>; N]) -> &str {
    values
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|value| !value.is_empty())
        .unwrap_or("")
}

fn trim_trailing_slash(value: &str) -> String {
    value.trim().trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_defaults_file_env_and_cli_in_order() {
        let persisted = PersistedSettings {
            model: Some("file-model".to_string()),
            api_base: Some("https://file.test/v1/".to_string()),
            api_key: Some("file-key".to_string()),
            timeout_ms: Some(10),
            max_output_tokens: Some(20),
            max_input_chars: Some(200),
        };
        let env = EnvSettings {
            model: Some("env-model".to_string()),
            api_base: Some("https://env.test/v1".to_string()),
            api_key: Some("env-key".to_string()),
            timeout_ms: Some(30),
            max_output_tokens: Some(40),
            max_input_chars: Some(400),
        };
        let cli = CliOverrides {
            model: Some("cli-model".to_string()),
            api_base: None,
            api_key: None,
            timeout_ms: Some(50),
            max_output_tokens: None,
            max_input_chars: Some(500),
        };

        assert_eq!(
            resolve_settings(&persisted, &env, &cli),
            RuntimeSettings {
                model: "cli-model".to_string(),
                api_base: "https://env.test/v1".to_string(),
                api_key: "env-key".to_string(),
                timeout_ms: 50,
                max_output_tokens: 40,
                max_input_chars: 500,
            }
        );
    }
}
