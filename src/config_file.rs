use crate::cli::ConfigKey;
use crate::settings::PersistedSettings;
use anyhow::{Context, Result, anyhow};
use directories::ProjectDirs;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

pub fn default_config_path() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("", "", "llm-distill")
        .ok_or_else(|| anyhow!("could not resolve a config directory"))?;
    Ok(dirs.config_dir().join("config.json"))
}

pub fn read_config(path: &Path) -> Result<PersistedSettings> {
    match fs::read_to_string(path) {
        Ok(raw) => {
            serde_json::from_str(&raw).with_context(|| format!("invalid config {}", path.display()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(PersistedSettings::default())
        }
        Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
    }
}

pub fn write_config(path: &Path, config: &PersistedSettings) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("config path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;

    let mut tmp = NamedTempFile::new_in(parent)
        .with_context(|| format!("failed to create temp file in {}", parent.display()))?;
    serde_json::to_writer_pretty(&mut tmp, config).context("failed to serialize config")?;
    std::io::Write::write_all(&mut tmp, b"\n").context("failed to finish config file")?;
    tmp.persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("failed to replace {}", path.display()))?;
    Ok(())
}

pub fn get_value(config: &PersistedSettings, key: ConfigKey) -> Option<String> {
    match key {
        ConfigKey::Model => config.model.clone(),
        ConfigKey::ApiBase => config.api_base.clone(),
        ConfigKey::ApiKey => config.api_key.as_ref().map(|_| "***".to_string()),
        ConfigKey::TimeoutMs => config.timeout_ms.map(|value| value.to_string()),
        ConfigKey::MaxOutputTokens => config.max_output_tokens.map(|value| value.to_string()),
        ConfigKey::MaxInputChars => config.max_input_chars.map(|value| value.to_string()),
    }
}

pub fn set_value(config: &mut PersistedSettings, key: ConfigKey, value: &str) -> Result<()> {
    match key {
        ConfigKey::Model => config.model = Some(non_empty(value, "model")?),
        ConfigKey::ApiBase => config.api_base = Some(non_empty(value, "api-base")?),
        ConfigKey::ApiKey => config.api_key = Some(non_empty(value, "api-key")?),
        ConfigKey::TimeoutMs => config.timeout_ms = Some(parse_positive(value, "timeout-ms")?),
        ConfigKey::MaxOutputTokens => {
            config.max_output_tokens = Some(parse_positive(value, "max-output-tokens")?)
        }
        ConfigKey::MaxInputChars => {
            config.max_input_chars = Some(parse_positive(value, "max-input-chars")?)
        }
    }
    Ok(())
}

fn non_empty(value: &str, name: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        anyhow::bail!("{name} cannot be empty");
    }
    Ok(value.trim_end_matches('/').to_string())
}

fn parse_positive<T>(value: &str, name: &str) -> Result<T>
where
    T: std::str::FromStr + PartialOrd + From<u8>,
{
    let parsed = value
        .trim()
        .parse::<T>()
        .map_err(|_| anyhow!("{name} must be a positive number"))?;
    if parsed <= T::from(0) {
        anyhow::bail!("{name} must be a positive number");
    }
    Ok(parsed)
}
