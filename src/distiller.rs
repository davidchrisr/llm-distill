use crate::cli::{Cli, Command, ConfigAction};
use crate::config_file;
use crate::openai::ResponsesClient;
use crate::prompt::build_batch_prompt;
use crate::settings::{EnvSettings, resolve_settings};
use crate::text::{ensure_trailing_newline, normalize_for_model};
use anyhow::{Result, bail};
use std::io::{self, IsTerminal, Read, Write};

pub async fn run(cli: Cli) -> Result<()> {
    let config_path = match cli.config.as_ref() {
        Some(path) => path.clone(),
        None => config_file::default_config_path()?,
    };

    if let Some(Command::Config(config)) = &cli.command {
        let mut persisted = config_file::read_config(&config_path)?;
        match config.action.as_ref().unwrap_or(&ConfigAction::Show) {
            ConfigAction::Show => {
                println!("path={}", config_path.display());
                println!("model={}", persisted.model.as_deref().unwrap_or(""));
                println!("api-base={}", persisted.api_base.as_deref().unwrap_or(""));
                println!(
                    "api-key={}",
                    persisted.api_key.as_ref().map(|_| "***").unwrap_or("")
                );
                println!(
                    "timeout-ms={}",
                    persisted
                        .timeout_ms
                        .map(|value| value.to_string())
                        .unwrap_or_default()
                );
                println!(
                    "max-output-tokens={}",
                    persisted
                        .max_output_tokens
                        .map(|value| value.to_string())
                        .unwrap_or_default()
                );
                println!(
                    "max-input-chars={}",
                    persisted
                        .max_input_chars
                        .map(|value| value.to_string())
                        .unwrap_or_default()
                );
            }
            ConfigAction::Get { key } => {
                println!(
                    "{}",
                    config_file::get_value(&persisted, *key).unwrap_or_default()
                );
            }
            ConfigAction::Set { key, value } => {
                config_file::set_value(&mut persisted, *key, value)?;
                config_file::write_config(&config_path, &persisted)?;
                println!("updated {key:?}");
            }
        }
        return Ok(());
    }

    let question = cli.question_text();
    if question.is_empty() {
        bail!("a question is required");
    }

    if io::stdin().is_terminal() {
        bail!("stdin is required");
    }

    let persisted = config_file::read_config(&config_path)?;
    let settings = resolve_settings(&persisted, &EnvSettings::current(), &cli.overrides());

    let mut raw_input = String::new();
    io::stdin()
        .read_to_string(&mut raw_input)
        .map_err(anyhow::Error::from)?;

    if raw_input.trim().is_empty() {
        return Ok(());
    }

    let prompt = build_batch_prompt(
        &question,
        &normalize_for_model(&raw_input),
        settings.max_input_chars,
    );
    let client = ResponsesClient::new(settings.timeout_ms)?;
    let output = client.create_text_response(&settings, &prompt).await?;

    io::stdout().write_all(ensure_trailing_newline(output.trim()).as_bytes())?;
    Ok(())
}
