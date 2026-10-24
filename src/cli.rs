use crate::settings::ProgressMode;
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "distill")]
#[command(about = "Reduce coding agent context costs with a cheaper model for tool output")]
#[command(
    override_usage = "distill [OPTIONS] <QUESTION>...\n       distill [OPTIONS] config [COMMAND]"
)]
pub struct Cli {
    /// Override the persisted config path.
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    /// Distillation model used to summarize tool output.
    #[arg(long)]
    pub model: Option<String>,

    /// Reasoning effort requested from supported models.
    #[arg(long)]
    pub reasoning_effort: Option<String>,

    /// API base URL. The Responses path is appended by the client.
    #[arg(long)]
    pub api_base: Option<String>,

    /// API key used for Authorization: Bearer.
    #[arg(long)]
    pub api_key: Option<String>,

    /// Request timeout in milliseconds.
    #[arg(long)]
    pub timeout_ms: Option<u64>,

    /// Maximum output tokens requested from the model.
    #[arg(long)]
    pub max_output_tokens: Option<u32>,

    /// Maximum output tokens for periodic progress summaries.
    #[arg(long)]
    pub progress_max_output_tokens: Option<u32>,

    /// Maximum command-output characters included in the model input.
    #[arg(long)]
    pub max_input_chars: Option<usize>,

    /// Maximum command-output bytes retained locally while reading stdin.
    #[arg(long)]
    pub max_collected_bytes: Option<usize>,

    /// Periodic progress behavior. Summary mode enables server-side response storage.
    #[arg(long, value_enum)]
    pub progress_mode: Option<ProgressMode>,

    /// Milliseconds between progress updates while reading upstream output.
    #[arg(long)]
    pub progress_interval_ms: Option<u64>,

    /// Milliseconds to wait before treating a prompt-like output tail as interactive.
    #[arg(long)]
    pub interactive_detect_ms: Option<u64>,

    /// Number of retries for transient API failures.
    #[arg(long)]
    pub retry_count: Option<u32>,

    /// Initial retry backoff in milliseconds for transient API failures.
    #[arg(long)]
    pub retry_backoff_ms: Option<u64>,

    /// Enable server-side response storage and print the final response ID to stderr.
    #[arg(short = 'r', long)]
    pub resumable: bool,

    /// Continue from a previous response ID, enabling server-side response storage.
    #[arg(short = 'p', long)]
    pub prev_response_id: Option<String>,

    /// Do not read stdin; answer using only the previous response context.
    #[arg(short = 'c', long)]
    pub from_context: bool,

    #[command(subcommand)]
    pub command: Option<Command>,

    /// Question for the distillation run.
    #[arg(
        value_name = "QUESTION",
        trailing_var_arg = true,
        allow_hyphen_values = true
    )]
    pub question: Vec<String>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Read or mutate persisted configuration.
    Config(ConfigCommand),
}

#[derive(Args, Debug)]
pub struct ConfigCommand {
    #[command(subcommand)]
    pub action: Option<ConfigAction>,
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Show the config path and current persisted values.
    Show,
    /// Get one persisted config key.
    Get { key: ConfigKey },
    /// Set one persisted config key.
    Set { key: ConfigKey, value: String },
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum ConfigKey {
    Model,
    ReasoningEffort,
    ApiBase,
    ApiKey,
    TimeoutMs,
    MaxOutputTokens,
    ProgressMaxOutputTokens,
    MaxInputChars,
    MaxCollectedBytes,
    ProgressMode,
    ProgressIntervalMs,
    InteractiveDetectMs,
    RetryCount,
    RetryBackoffMs,
}

#[derive(Debug, Clone, Default)]
pub struct CliOverrides {
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

impl Cli {
    pub fn overrides(&self) -> CliOverrides {
        CliOverrides {
            model: self.model.clone(),
            reasoning_effort: self.reasoning_effort.clone(),
            api_base: self.api_base.clone(),
            api_key: self.api_key.clone(),
            timeout_ms: self.timeout_ms,
            max_output_tokens: self.max_output_tokens,
            progress_max_output_tokens: self.progress_max_output_tokens,
            max_input_chars: self.max_input_chars,
            max_collected_bytes: self.max_collected_bytes,
            progress_mode: self.progress_mode,
            progress_interval_ms: self.progress_interval_ms,
            interactive_detect_ms: self.interactive_detect_ms,
            retry_count: self.retry_count,
            retry_backoff_ms: self.retry_backoff_ms,
        }
    }

    pub fn question_text(&self) -> String {
        self.question.join(" ").trim().to_string()
    }
}
