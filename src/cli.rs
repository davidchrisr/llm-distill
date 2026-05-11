use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "distill")]
#[command(about = "Compress command output into the answer an agent needs")]
pub struct Cli {
    /// Override the persisted config path.
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    /// OpenAI Responses API model.
    #[arg(long)]
    pub model: Option<String>,

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

    /// Maximum command-output characters included in the model input.
    #[arg(long)]
    pub max_input_chars: Option<usize>,

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
    ApiBase,
    ApiKey,
    TimeoutMs,
    MaxOutputTokens,
    MaxInputChars,
}

#[derive(Debug, Clone, Default)]
pub struct CliOverrides {
    pub model: Option<String>,
    pub api_base: Option<String>,
    pub api_key: Option<String>,
    pub timeout_ms: Option<u64>,
    pub max_output_tokens: Option<u32>,
    pub max_input_chars: Option<usize>,
}

impl Cli {
    pub fn overrides(&self) -> CliOverrides {
        CliOverrides {
            model: self.model.clone(),
            api_base: self.api_base.clone(),
            api_key: self.api_key.clone(),
            timeout_ms: self.timeout_ms,
            max_output_tokens: self.max_output_tokens,
            max_input_chars: self.max_input_chars,
        }
    }

    pub fn question_text(&self) -> String {
        self.question.join(" ").trim().to_string()
    }
}
