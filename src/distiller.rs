use crate::cli::{Cli, Command, ConfigAction};
use crate::config_file;
use crate::openai::ResponsesClient;
use crate::output_collector::OutputCollector;
use crate::prompt::{build_final_prompt, build_progress_prompt};
use crate::settings::{EnvSettings, ProgressMode, RuntimeSettings, resolve_settings};
use crate::text::{ensure_trailing_newline, has_prompt_like_tail, normalize_for_model};
use anyhow::{Context, Result, bail};
use std::io::{self, IsTerminal, Read, Write};
use std::sync::mpsc;
use std::time::{Duration, Instant};

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
                    "progress-max-output-tokens={}",
                    persisted
                        .progress_max_output_tokens
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
                println!(
                    "max-collected-bytes={}",
                    persisted
                        .max_collected_bytes
                        .map(|value| value.to_string())
                        .unwrap_or_default()
                );
                println!(
                    "progress-mode={}",
                    persisted
                        .progress_mode
                        .map(|value| value.to_string())
                        .unwrap_or_default()
                );
                println!(
                    "progress-interval-ms={}",
                    persisted
                        .progress_interval_ms
                        .map(|value| value.to_string())
                        .unwrap_or_default()
                );
                println!(
                    "interactive-detect-ms={}",
                    persisted
                        .interactive_detect_ms
                        .map(|value| value.to_string())
                        .unwrap_or_default()
                );
                println!(
                    "retry-count={}",
                    persisted
                        .retry_count
                        .map(|value| value.to_string())
                        .unwrap_or_default()
                );
                println!(
                    "retry-backoff-ms={}",
                    persisted
                        .retry_backoff_ms
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
    settings.validate()?;

    let read_result = read_stdin_with_progress(&settings, &question).await?;

    if read_result.input.trim().is_empty() {
        return Ok(());
    }

    let prompt = build_final_prompt(
        &question,
        &normalize_for_model(&read_result.input),
        settings.max_input_chars,
        read_result.previous_response_id.is_some(),
    );
    let client = ResponsesClient::new(settings.timeout_ms)?;
    let output = client
        .create_response(
            &settings,
            &prompt,
            settings.max_output_tokens,
            read_result.previous_response_id.as_deref(),
        )
        .await?;

    io::stdout().write_all(ensure_trailing_newline(output.text.trim()).as_bytes())?;
    Ok(())
}

struct ReadResult {
    input: String,
    previous_response_id: Option<String>,
}

async fn read_stdin_with_progress(
    settings: &RuntimeSettings,
    question: &str,
) -> Result<ReadResult> {
    let (sender, receiver) = mpsc::channel();
    let progress_interval = Duration::from_millis(settings.progress_interval_ms);
    let interactive_interval = match settings.interactive_detect_ms {
        0 => progress_interval,
        value => Duration::from_millis(value).min(progress_interval),
    };

    std::thread::spawn(move || {
        let mut stdin = io::stdin();
        let mut buffer = [0_u8; 8192];

        loop {
            match stdin.read(&mut buffer) {
                Ok(0) => {
                    let _ = sender.send(Ok(Vec::new()));
                    return;
                }
                Ok(size) => {
                    if sender.send(Ok(buffer[..size].to_vec())).is_err() {
                        return;
                    }
                }
                Err(error) => {
                    let _ = sender.send(Err(error));
                    return;
                }
            }
        }
    });

    let mut collector = OutputCollector::new(settings.max_collected_bytes);
    let mut interval_collector = OutputCollector::new(settings.max_collected_bytes);
    let mut snapshot_count = 0_u64;
    let mut last_report = Instant::now();
    let mut last_output = Instant::now();
    let mut previous_response_id = None;
    let mut prompt_tail = String::new();
    let mut suspected_prompt_at = None;
    let client = ResponsesClient::new(settings.timeout_ms)?;

    loop {
        match receiver.recv_timeout(interactive_interval) {
            Ok(Ok(chunk)) if chunk.is_empty() => break,
            Ok(Ok(chunk)) => {
                last_output = Instant::now();
                collector.push(&chunk);
                interval_collector.push(&chunk);
                push_prompt_tail(&mut prompt_tail, &chunk);
                if has_prompt_like_tail(&prompt_tail) {
                    suspected_prompt_at = Some(Instant::now());
                } else {
                    suspected_prompt_at = None;
                }
                if last_report.elapsed() >= progress_interval {
                    let mut report = ProgressReport {
                        collector: &collector,
                        interval_collector: &mut interval_collector,
                        idle_duration: last_output.elapsed(),
                        snapshot_count: &mut snapshot_count,
                        previous_response_id: &mut previous_response_id,
                    };
                    report_progress(settings, question, &client, &mut report).await?;
                    last_report = Instant::now();
                }
            }
            Ok(Err(error)) => return Err(error).context("failed to read stdin"),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                fail_if_interactive_prompt_expired(&prompt_tail, suspected_prompt_at, settings)?;
                if last_report.elapsed() >= progress_interval {
                    let mut report = ProgressReport {
                        collector: &collector,
                        interval_collector: &mut interval_collector,
                        idle_duration: last_output.elapsed(),
                        snapshot_count: &mut snapshot_count,
                        previous_response_id: &mut previous_response_id,
                    };
                    report_progress(settings, question, &client, &mut report).await?;
                    last_report = Instant::now();
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    Ok(ReadResult {
        input: collector.materialize(),
        previous_response_id,
    })
}

fn push_prompt_tail(prompt_tail: &mut String, chunk: &[u8]) {
    prompt_tail.push_str(&String::from_utf8_lossy(chunk));
    let tail = prompt_tail
        .chars()
        .rev()
        .take(1024)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    *prompt_tail = tail;
}

fn fail_if_interactive_prompt_expired(
    prompt_tail: &str,
    suspected_prompt_at: Option<Instant>,
    settings: &RuntimeSettings,
) -> Result<()> {
    if settings.interactive_detect_ms == 0 {
        return Ok(());
    }

    let Some(suspected_prompt_at) = suspected_prompt_at else {
        return Ok(());
    };

    if suspected_prompt_at.elapsed() >= Duration::from_millis(settings.interactive_detect_ms)
        && has_prompt_like_tail(prompt_tail)
    {
        bail!("interactive prompt detected; do not pipe interactive commands through distill");
    }

    Ok(())
}

struct ProgressReport<'a> {
    collector: &'a OutputCollector,
    interval_collector: &'a mut OutputCollector,
    idle_duration: Duration,
    snapshot_count: &'a mut u64,
    previous_response_id: &'a mut Option<String>,
}

async fn report_progress(
    settings: &RuntimeSettings,
    question: &str,
    client: &ResponsesClient,
    report: &mut ProgressReport<'_>,
) -> Result<()> {
    let idle_ms = report.idle_duration.as_millis();

    match settings.progress_mode {
        ProgressMode::Stats => {
            eprintln!(
                "distill: reading {} bytes, {} lines ({}ms since last output)",
                report.collector.total_bytes(),
                report.collector.total_lines(),
                idle_ms
            );
        }
        ProgressMode::Summary => {
            if report.interval_collector.is_empty() {
                return Ok(());
            }

            *report.snapshot_count += 1;
            eprintln!(
                "distill: summarizing snapshot {} ({} bytes, {} lines, {}ms since last output)",
                *report.snapshot_count,
                report.collector.total_bytes(),
                report.collector.total_lines(),
                idle_ms
            );

            let raw_input = report.interval_collector.materialize();
            let prompt = build_progress_prompt(
                question,
                &normalize_for_model(&raw_input),
                settings.max_input_chars,
            );
            let output = client
                .create_response(
                    settings,
                    &prompt,
                    settings.progress_max_output_tokens,
                    report.previous_response_id.as_deref(),
                )
                .await?;
            *report.previous_response_id = output.id.clone();
            eprintln!(
                "distill: snapshot {}: {}",
                *report.snapshot_count,
                output.text.trim()
            );
            *report.interval_collector = OutputCollector::new(settings.max_collected_bytes);
        }
    }

    Ok(())
}
