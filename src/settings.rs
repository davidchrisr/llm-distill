use crate::cli::CliOverrides;
use anyhow::{Result, bail};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::env;
use std::ffi::OsString;
use std::fmt;
use std::str::FromStr;

pub const DEFAULT_MODEL: &str = "gpt-6-luna";
pub const DEFAULT_API_BASE: &str = "https://api.openai.com/v1";
pub const DEFAULT_TIMEOUT_MS: u64 = 90_000;
pub const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 512;
pub const DEFAULT_PROGRESS_MAX_OUTPUT_TOKENS: u32 = 128;
pub const DEFAULT_MAX_INPUT_CHARS: usize = 100_000;
pub const DEFAULT_MAX_COLLECTED_BYTES: usize = 1_048_576;
pub const DEFAULT_PROGRESS_INTERVAL_MS: u64 = 30_000;
pub const DEFAULT_INTERACTIVE_DETECT_MS: u64 = 10_000;
pub const DEFAULT_RETRY_COUNT: u32 = 5;
pub const DEFAULT_RETRY_BACKOFF_MS: u64 = 1_000;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum ProgressMode {
    Stats,
    Summary,
}

pub const DEFAULT_REASONING_EFFORT: &str = "low";

impl fmt::Display for ProgressMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stats => formatter.write_str("stats"),
            Self::Summary => formatter.write_str("summary"),
        }
    }
}

impl FromStr for ProgressMode {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "stats" => Ok(Self::Stats),
            "summary" => Ok(Self::Summary),
            _ => anyhow::bail!("progress-mode must be stats or summary"),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PersistedSettings {
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub api_base: Option<String>,
    pub api_key: Option<String>,
    pub timeout_ms: Option<u64>,
    pub max_output_tokens: Option<u32>,
    pub progress_max_output_tokens: Option<u32>,
    pub max_input_chars: Option<usize>,
    pub max_collected_bytes: Option<usize>,
    pub progress_mode: Option<ProgressMode>,
    pub progress_interval_ms: Option<u64>,
    pub interactive_detect_ms: Option<u64>,
    pub retry_count: Option<u32>,
    pub retry_backoff_ms: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct EnvSettings {
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub api_base: Option<String>,
    pub api_key: Option<String>,
    pub timeout_ms: Option<u64>,
    pub max_output_tokens: Option<u32>,
    pub progress_max_output_tokens: Option<u32>,
    pub max_input_chars: Option<usize>,
    pub max_collected_bytes: Option<usize>,
    pub progress_mode: Option<ProgressMode>,
    pub progress_interval_ms: Option<u64>,
    pub interactive_detect_ms: Option<u64>,
    pub retry_count: Option<u32>,
    pub retry_backoff_ms: Option<u64>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RuntimeSettings {
    pub model: String,
    pub reasoning_effort: Option<String>,
    pub api_base: String,
    pub api_key: String,
    pub timeout_ms: u64,
    pub max_output_tokens: u32,
    pub progress_max_output_tokens: u32,
    pub max_input_chars: usize,
    pub max_collected_bytes: usize,
    pub progress_mode: ProgressMode,
    pub progress_interval_ms: u64,
    pub interactive_detect_ms: u64,
    pub retry_count: u32,
    pub retry_backoff_ms: u64,
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
                "DISTILL_REASONING_EFFORT" => env.reasoning_effort = Some(value),
                "DISTILL_API_BASE" => env.api_base = Some(value),
                "DISTILL_API_KEY" => env.api_key = Some(value),
                "DISTILL_TIMEOUT_MS" => env.timeout_ms = value.parse().ok(),
                "DISTILL_MAX_OUTPUT_TOKENS" => env.max_output_tokens = value.parse().ok(),
                "DISTILL_PROGRESS_MAX_OUTPUT_TOKENS" => {
                    env.progress_max_output_tokens = value.parse().ok()
                }
                "DISTILL_MAX_INPUT_CHARS" => env.max_input_chars = value.parse().ok(),
                "DISTILL_MAX_COLLECTED_BYTES" => env.max_collected_bytes = value.parse().ok(),
                "DISTILL_PROGRESS_MODE" => env.progress_mode = value.parse().ok(),
                "DISTILL_PROGRESS_INTERVAL_MS" => env.progress_interval_ms = value.parse().ok(),
                "DISTILL_INTERACTIVE_DETECT_MS" => env.interactive_detect_ms = value.parse().ok(),
                "DISTILL_RETRY_COUNT" => env.retry_count = value.parse().ok(),
                "DISTILL_RETRY_BACKOFF_MS" => env.retry_backoff_ms = value.parse().ok(),
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
        reasoning_effort: Some(
            first_non_empty([
                cli.reasoning_effort.as_deref(),
                env.reasoning_effort.as_deref(),
                persisted.reasoning_effort.as_deref(),
                Some(DEFAULT_REASONING_EFFORT),
            ])
            .to_string(),
        ),
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
        progress_max_output_tokens: cli
            .progress_max_output_tokens
            .or(env.progress_max_output_tokens)
            .or(persisted.progress_max_output_tokens)
            .unwrap_or(DEFAULT_PROGRESS_MAX_OUTPUT_TOKENS),
        max_input_chars: cli
            .max_input_chars
            .or(env.max_input_chars)
            .or(persisted.max_input_chars)
            .unwrap_or(DEFAULT_MAX_INPUT_CHARS),
        max_collected_bytes: cli
            .max_collected_bytes
            .or(env.max_collected_bytes)
            .or(persisted.max_collected_bytes)
            .unwrap_or(DEFAULT_MAX_COLLECTED_BYTES),
        progress_mode: cli
            .progress_mode
            .or(env.progress_mode)
            .or(persisted.progress_mode)
            .unwrap_or(ProgressMode::Stats),
        progress_interval_ms: cli
            .progress_interval_ms
            .or(env.progress_interval_ms)
            .or(persisted.progress_interval_ms)
            .unwrap_or(DEFAULT_PROGRESS_INTERVAL_MS),
        interactive_detect_ms: cli
            .interactive_detect_ms
            .or(env.interactive_detect_ms)
            .or(persisted.interactive_detect_ms)
            .unwrap_or(DEFAULT_INTERACTIVE_DETECT_MS),
        retry_count: cli
            .retry_count
            .or(env.retry_count)
            .or(persisted.retry_count)
            .unwrap_or(DEFAULT_RETRY_COUNT),
        retry_backoff_ms: cli
            .retry_backoff_ms
            .or(env.retry_backoff_ms)
            .or(persisted.retry_backoff_ms)
            .unwrap_or(DEFAULT_RETRY_BACKOFF_MS),
    }
}

impl RuntimeSettings {
    pub fn validate(&self) -> Result<()> {
        require_positive(self.timeout_ms, "timeout-ms")?;
        require_positive(self.max_output_tokens, "max-output-tokens")?;
        require_positive(
            self.progress_max_output_tokens,
            "progress-max-output-tokens",
        )?;
        require_positive(self.max_input_chars, "max-input-chars")?;
        require_positive(self.max_collected_bytes, "max-collected-bytes")?;
        require_positive(self.progress_interval_ms, "progress-interval-ms")?;
        require_positive(self.retry_backoff_ms, "retry-backoff-ms")?;
        Ok(())
    }
}

fn require_positive<T>(value: T, name: &str) -> Result<()>
where
    T: PartialOrd + From<u8>,
{
    if value <= T::from(0) {
        bail!("{name} must be a positive number");
    }
    Ok(())
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
            reasoning_effort: Some("file-effort".to_string()),
            api_base: Some("https://file.test/v1/".to_string()),
            api_key: Some("file-key".to_string()),
            timeout_ms: Some(10),
            max_output_tokens: Some(20),
            progress_max_output_tokens: Some(12),
            max_input_chars: Some(200),
            max_collected_bytes: Some(2_000),
            progress_mode: Some(ProgressMode::Stats),
            progress_interval_ms: Some(2_000),
            interactive_detect_ms: Some(6_000),
            retry_count: Some(1),
            retry_backoff_ms: Some(100),
        };
        let env = EnvSettings {
            model: Some("env-model".to_string()),
            reasoning_effort: Some("env-effort".to_string()),
            api_base: Some("https://env.test/v1".to_string()),
            api_key: Some("env-key".to_string()),
            timeout_ms: Some(30),
            max_output_tokens: Some(40),
            progress_max_output_tokens: Some(14),
            max_input_chars: Some(400),
            max_collected_bytes: Some(4_000),
            progress_mode: Some(ProgressMode::Summary),
            progress_interval_ms: Some(4_000),
            interactive_detect_ms: Some(8_000),
            retry_count: Some(2),
            retry_backoff_ms: Some(200),
        };
        let cli = CliOverrides {
            model: Some("cli-model".to_string()),
            reasoning_effort: Some("future-effort".to_string()),
            api_base: None,
            api_key: None,
            timeout_ms: Some(50),
            max_output_tokens: None,
            progress_max_output_tokens: None,
            max_input_chars: Some(500),
            max_collected_bytes: None,
            progress_mode: None,
            progress_interval_ms: None,
            interactive_detect_ms: None,
            retry_count: None,
            retry_backoff_ms: None,
        };

        assert_eq!(
            resolve_settings(&persisted, &env, &cli),
            RuntimeSettings {
                model: "cli-model".to_string(),
                reasoning_effort: Some("future-effort".to_string()),
                api_base: "https://env.test/v1".to_string(),
                api_key: "env-key".to_string(),
                timeout_ms: 50,
                max_output_tokens: 40,
                progress_max_output_tokens: 14,
                max_input_chars: 500,
                max_collected_bytes: 4_000,
                progress_mode: ProgressMode::Summary,
                progress_interval_ms: 4_000,
                interactive_detect_ms: 8_000,
                retry_count: 2,
                retry_backoff_ms: 200,
            }
        );
    }

    #[test]
    fn resolves_progress_cli_overrides() {
        let persisted = PersistedSettings {
            progress_max_output_tokens: Some(64),
            max_collected_bytes: Some(1_000),
            progress_mode: Some(ProgressMode::Stats),
            progress_interval_ms: Some(10_000),
            interactive_detect_ms: Some(10_000),
            retry_count: Some(1),
            retry_backoff_ms: Some(100),
            ..PersistedSettings::default()
        };
        let env = EnvSettings {
            progress_max_output_tokens: Some(96),
            max_collected_bytes: Some(2_000),
            progress_mode: Some(ProgressMode::Stats),
            progress_interval_ms: Some(20_000),
            interactive_detect_ms: Some(20_000),
            retry_count: Some(2),
            retry_backoff_ms: Some(200),
            ..EnvSettings::default()
        };
        let cli = CliOverrides {
            progress_max_output_tokens: Some(128),
            max_collected_bytes: Some(3_000),
            progress_mode: Some(ProgressMode::Summary),
            progress_interval_ms: Some(30_000),
            interactive_detect_ms: Some(30_000),
            retry_count: Some(3),
            retry_backoff_ms: Some(300),
            ..CliOverrides::default()
        };

        let settings = resolve_settings(&persisted, &env, &cli);

        assert_eq!(settings.progress_max_output_tokens, 128);
        assert_eq!(settings.max_collected_bytes, 3_000);
        assert_eq!(settings.progress_mode, ProgressMode::Summary);
        assert_eq!(settings.progress_interval_ms, 30_000);
        assert_eq!(settings.interactive_detect_ms, 30_000);
        assert_eq!(settings.retry_count, 3);
        assert_eq!(settings.retry_backoff_ms, 300);
    }

    #[test]
    fn validates_runtime_numeric_settings() {
        let mut settings = resolve_settings(
            &PersistedSettings::default(),
            &EnvSettings::default(),
            &CliOverrides::default(),
        );

        assert!(settings.validate().is_ok());
        assert_eq!(settings.model, "gpt-6-luna");
        assert_eq!(settings.reasoning_effort.as_deref(), Some("low"));

        settings.max_output_tokens = 0;
        assert!(settings.validate().is_err());
    }

    #[test]
    fn allows_zero_interactive_detect_to_disable_guard() {
        let settings = resolve_settings(
            &PersistedSettings {
                interactive_detect_ms: Some(0),
                ..PersistedSettings::default()
            },
            &EnvSettings::default(),
            &CliOverrides::default(),
        );

        assert_eq!(settings.interactive_detect_ms, 0);
        assert!(settings.validate().is_ok());
    }
}
