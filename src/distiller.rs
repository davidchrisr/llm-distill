use crate::cli::{Cli, Command, ConfigAction};
use crate::config_file;
use crate::openai::ResponsesClient;
use crate::output_collector::OutputCollector;
use crate::prompt::{
    build_context_followup_prompt, build_empty_output_prompt, build_final_prompt,
    build_progress_prompt,
};
use crate::settings::{EnvSettings, ProgressMode, RuntimeSettings, resolve_settings};
use crate::text::{ensure_trailing_newline, has_prompt_like_tail, normalize_for_model};
use anyhow::{Context, Result, anyhow, bail};
use std::io::{self, IsTerminal, Read, Write};
use std::sync::mpsc;
use std::thread;
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
                println!(
                    "reasoning-effort={}",
                    persisted.reasoning_effort.clone().unwrap_or_default()
                );
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

    if cli.from_context && cli.prev_response_id.is_none() {
        bail!("--from-context requires --prev-response-id");
    }

    let stdin_is_terminal = io::stdin().is_terminal();
    if !cli.from_context && stdin_is_terminal {
        bail!("stdin is required");
    }

    let persisted = config_file::read_config(&config_path)?;
    let settings = resolve_settings(&persisted, &EnvSettings::current(), &cli.overrides());
    settings.validate()?;

    let read_result = if cli.from_context {
        ReadResult {
            input: String::new(),
            previous_response_id: None,
        }
    } else {
        read_stdin_with_progress(&settings, &question).await?
    };

    let has_progress_context = read_result.previous_response_id.is_some();
    let has_previous_context = cli.prev_response_id.is_some();
    let retained_input = read_result.input.trim();

    if retained_input.is_empty() && !has_progress_context && !has_previous_context {
        return Ok(());
    }

    let prompt = if cli.from_context {
        build_context_followup_prompt(&question)
    } else if retained_input.is_empty() {
        build_empty_output_prompt(&question, has_progress_context, has_previous_context)
    } else {
        build_final_prompt(
            &question,
            &normalize_for_model(&read_result.input),
            settings.max_input_chars,
            has_progress_context,
            has_previous_context,
        )
    };
    let client = ResponsesClient::new(settings.timeout_ms)?;
    let previous_response_id = read_result
        .previous_response_id
        .as_deref()
        .or(cli.prev_response_id.as_deref());
    let output = client
        .create_response(
            &settings,
            &prompt,
            settings.max_output_tokens,
            previous_response_id,
        )
        .await?;

    io::stdout().write_all(ensure_trailing_newline(output.text.trim()).as_bytes())?;
    if cli.resumable {
        match output.id {
            Some(id) => eprintln!("distill: response id: {id}"),
            None => eprintln!("distill: response id unavailable"),
        }
    }
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
    let mut summary_worker = match settings.progress_mode {
        ProgressMode::Stats => SummaryWorker::disabled(),
        ProgressMode::Summary => SummaryWorker::start(settings, question)?,
    };

    std::thread::spawn(move || {
        let mut stdin = io::stdin();
        let mut buffer = [0_u8; 8192];

        loop {
            match stdin.read(&mut buffer) {
                Ok(0) => {
                    let _ = sender.send((Instant::now(), Ok(Vec::new())));
                    return;
                }
                Ok(size) => {
                    if sender
                        .send((Instant::now(), Ok(buffer[..size].to_vec())))
                        .is_err()
                    {
                        return;
                    }
                }
                Err(error) => {
                    let _ = sender.send((Instant::now(), Err(error)));
                    return;
                }
            }
        }
    });

    let mut collector = OutputCollector::new(settings.max_collected_bytes);
    let mut interval_collector = OutputCollector::new(settings.max_collected_bytes);
    let mut snapshot_count = 0_u64;
    let started_at = Instant::now();
    let mut next_report_at = started_at + progress_interval;
    let mut last_output = started_at;
    let mut prompt_tail = String::new();
    let mut suspected_prompt_at = None;

    loop {
        summary_worker.drain_completed();

        let now = Instant::now();
        fail_if_interactive_prompt_expired_at(&prompt_tail, suspected_prompt_at, settings, now)?;

        if now >= next_report_at {
            report_progress(
                settings,
                &collector,
                &mut interval_collector,
                &mut snapshot_count,
                last_output,
                now,
                &mut summary_worker,
            )?;
            next_report_at = next_progress_deadline(next_report_at, progress_interval, now);
            continue;
        }

        let wait_until = next_interesting_deadline(next_report_at, suspected_prompt_at, settings);
        let wait_duration = wait_until.saturating_duration_since(now);

        match receiver.recv_timeout(wait_duration) {
            Ok((_, Ok(chunk))) if chunk.is_empty() => break,
            Ok((received_at, Ok(chunk))) => {
                while received_at >= next_report_at {
                    report_progress(
                        settings,
                        &collector,
                        &mut interval_collector,
                        &mut snapshot_count,
                        last_output,
                        next_report_at,
                        &mut summary_worker,
                    )?;
                    next_report_at += progress_interval;
                }

                last_output = received_at;
                collector.push(&chunk);
                interval_collector.push(&chunk);
                push_prompt_tail(&mut prompt_tail, &chunk);
                if has_prompt_like_tail(&prompt_tail) {
                    suspected_prompt_at = Some(received_at);
                } else {
                    suspected_prompt_at = None;
                }
            }
            Ok((_, Err(error))) => return Err(error).context("failed to read stdin"),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let previous_response_id = summary_worker.finish()?;

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

fn fail_if_interactive_prompt_expired_at(
    prompt_tail: &str,
    suspected_prompt_at: Option<Instant>,
    settings: &RuntimeSettings,
    now: Instant,
) -> Result<()> {
    if settings.interactive_detect_ms == 0 {
        return Ok(());
    }

    let Some(suspected_prompt_at) = suspected_prompt_at else {
        return Ok(());
    };

    if now.duration_since(suspected_prompt_at)
        >= Duration::from_millis(settings.interactive_detect_ms)
        && has_prompt_like_tail(prompt_tail)
    {
        bail!("interactive prompt detected; do not pipe interactive commands through distill");
    }

    Ok(())
}

fn next_interesting_deadline(
    next_report_at: Instant,
    suspected_prompt_at: Option<Instant>,
    settings: &RuntimeSettings,
) -> Instant {
    match (settings.interactive_detect_ms, suspected_prompt_at) {
        (0, _) | (_, None) => next_report_at,
        (interactive_detect_ms, Some(suspected_prompt_at)) => {
            let prompt_deadline =
                suspected_prompt_at + Duration::from_millis(interactive_detect_ms);
            next_report_at.min(prompt_deadline)
        }
    }
}

fn next_progress_deadline(
    mut next_report_at: Instant,
    progress_interval: Duration,
    now: Instant,
) -> Instant {
    while next_report_at <= now {
        next_report_at += progress_interval;
    }
    next_report_at
}

fn report_progress(
    settings: &RuntimeSettings,
    collector: &OutputCollector,
    interval_collector: &mut OutputCollector,
    snapshot_count: &mut u64,
    last_output: Instant,
    report_at: Instant,
    summary_worker: &mut SummaryWorker,
) -> Result<()> {
    let idle_duration = report_at.saturating_duration_since(last_output);
    let idle_ms = idle_duration.as_millis();

    match settings.progress_mode {
        ProgressMode::Stats => {
            eprintln!(
                "distill: read {} bytes, {} lines total ({}ms since last output)",
                collector.total_bytes(),
                collector.total_lines(),
                idle_ms
            );
        }
        ProgressMode::Summary => {
            if interval_collector.is_empty() {
                return Ok(());
            }

            *snapshot_count += 1;
            let snapshot = ProgressSnapshot {
                number: *snapshot_count,
                total_bytes: collector.total_bytes(),
                total_lines: collector.total_lines(),
                idle_duration,
                raw_input: interval_collector.materialize(),
                start_reported: false,
            };
            summary_worker.enqueue(snapshot)?;
            *interval_collector = OutputCollector::new(settings.max_collected_bytes);
        }
    }

    Ok(())
}

struct ProgressSnapshot {
    number: u64,
    total_bytes: usize,
    total_lines: usize,
    idle_duration: Duration,
    raw_input: String,
    start_reported: bool,
}

struct SummaryWorker {
    sender: Option<mpsc::Sender<ProgressSnapshot>>,
    completed_receiver: mpsc::Receiver<()>,
    handle: Option<thread::JoinHandle<Result<Option<String>>>>,
    active_count: usize,
}

impl SummaryWorker {
    fn disabled() -> Self {
        let (_sender, completed_receiver) = mpsc::channel();
        Self {
            sender: None,
            completed_receiver,
            handle: None,
            active_count: 0,
        }
    }

    #[cfg(test)]
    fn for_tests() -> (Self, mpsc::Receiver<ProgressSnapshot>, mpsc::Sender<()>) {
        let (sender, receiver) = mpsc::channel::<ProgressSnapshot>();
        let (completed_sender, completed_receiver) = mpsc::channel::<()>();
        (
            Self {
                sender: Some(sender),
                completed_receiver,
                handle: None,
                active_count: 0,
            },
            receiver,
            completed_sender,
        )
    }

    fn start(settings: &RuntimeSettings, question: &str) -> Result<Self> {
        let (sender, receiver) = mpsc::channel::<ProgressSnapshot>();
        let (completed_sender, completed_receiver) = mpsc::channel::<()>();
        let settings = settings.clone();
        let question = question.to_string();
        let handle = thread::Builder::new()
            .name("distill-progress-summary".to_string())
            .spawn(move || run_summary_worker(settings, question, receiver, completed_sender))
            .context("failed to start progress summary worker")?;

        Ok(Self {
            sender: Some(sender),
            completed_receiver,
            handle: Some(handle),
            active_count: 0,
        })
    }

    fn enqueue(&mut self, mut snapshot: ProgressSnapshot) -> Result<()> {
        self.drain_completed();
        let state = if self.active_count == 0 {
            "summarizing"
        } else {
            "queued"
        };
        eprintln!("{}", snapshot_status_line(&snapshot, state));
        snapshot.start_reported = self.active_count == 0;
        let sender = self
            .sender
            .as_ref()
            .ok_or_else(|| anyhow!("progress summary worker is closed"))?;
        sender
            .send(snapshot)
            .context("failed to queue progress summary")?;
        self.active_count += 1;
        Ok(())
    }

    fn drain_completed(&mut self) {
        while self.completed_receiver.try_recv().is_ok() {
            self.active_count = self.active_count.saturating_sub(1);
        }
    }

    fn finish(mut self) -> Result<Option<String>> {
        self.sender.take();
        let Some(handle) = self.handle.take() else {
            return Ok(None);
        };
        handle
            .join()
            .map_err(|_| anyhow!("progress summary worker panicked"))?
    }
}

fn run_summary_worker(
    settings: RuntimeSettings,
    question: String,
    receiver: mpsc::Receiver<ProgressSnapshot>,
    completed_sender: mpsc::Sender<()>,
) -> Result<Option<String>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to start progress summary runtime")?;

    runtime.block_on(async move {
        let client = ResponsesClient::new(settings.timeout_ms)?;
        let mut previous_response_id = None;

        for snapshot in receiver {
            if !snapshot.start_reported {
                eprintln!("{}", snapshot_status_line(&snapshot, "summarizing"));
            }

            let prompt = build_progress_prompt(
                &question,
                &normalize_for_model(&snapshot.raw_input),
                settings.max_input_chars,
            );
            let output = client
                .create_response(
                    &settings,
                    &prompt,
                    settings.progress_max_output_tokens,
                    previous_response_id.as_deref(),
                )
                .await?;
            previous_response_id = output.id.clone();
            eprintln!(
                "distill: snapshot {}: {}",
                snapshot.number,
                output.text.trim()
            );
            let _ = completed_sender.send(());
        }

        Ok(previous_response_id)
    })
}

fn snapshot_status_line(snapshot: &ProgressSnapshot, state: &str) -> String {
    format!(
        "distill: snapshot {}: {} ({} bytes, {} lines total, {}ms since last output)",
        snapshot.number,
        state,
        snapshot.total_bytes,
        snapshot.total_lines,
        snapshot.idle_duration.as_millis()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_progress_deadline_preserves_interval_schedule() {
        let first_deadline = Instant::now() + Duration::from_millis(20);
        let interval = Duration::from_millis(20);
        let delayed_until = first_deadline + Duration::from_millis(55);

        let next_deadline = next_progress_deadline(first_deadline, interval, delayed_until);

        assert_eq!(next_deadline, first_deadline + Duration::from_millis(60));
    }

    #[test]
    fn next_interesting_deadline_prefers_earlier_interactive_deadline() {
        let now = Instant::now();
        let next_report_at = now + Duration::from_millis(20_000);
        let suspected_prompt_at = now + Duration::from_millis(5_000);
        let settings = settings_with_interactive_detect_ms(10_000);

        let deadline =
            next_interesting_deadline(next_report_at, Some(suspected_prompt_at), &settings);

        assert_eq!(deadline, now + Duration::from_millis(15_000));
    }

    #[test]
    fn next_interesting_deadline_uses_report_deadline_when_interactive_detection_disabled() {
        let now = Instant::now();
        let next_report_at = now + Duration::from_millis(20_000);
        let suspected_prompt_at = now + Duration::from_millis(5_000);
        let settings = settings_with_interactive_detect_ms(0);

        let deadline =
            next_interesting_deadline(next_report_at, Some(suspected_prompt_at), &settings);

        assert_eq!(deadline, next_report_at);
    }

    #[test]
    fn snapshot_status_line_uses_stable_snapshot_metadata() {
        let snapshot = ProgressSnapshot {
            number: 2,
            total_bytes: 87_826,
            total_lines: 647,
            idle_duration: Duration::from_millis(12_000),
            raw_input: "ignored".to_string(),
            start_reported: false,
        };

        assert_eq!(
            snapshot_status_line(&snapshot, "queued"),
            "distill: snapshot 2: queued (87826 bytes, 647 lines total, 12000ms since last output)"
        );
    }

    #[test]
    fn summary_worker_marks_second_snapshot_as_queued_until_completion_is_drained() {
        let (mut worker, receiver, completed_sender) = SummaryWorker::for_tests();

        worker.enqueue(snapshot(1)).unwrap();
        let first = receiver.recv().unwrap();
        assert!(first.start_reported);
        assert_eq!(worker.active_count, 1);

        worker.enqueue(snapshot(2)).unwrap();
        let second = receiver.recv().unwrap();
        assert!(!second.start_reported);
        assert_eq!(worker.active_count, 2);

        completed_sender.send(()).unwrap();
        worker.drain_completed();

        assert_eq!(worker.active_count, 1);
    }

    fn snapshot(number: u64) -> ProgressSnapshot {
        ProgressSnapshot {
            number,
            total_bytes: number as usize,
            total_lines: number as usize,
            idle_duration: Duration::from_millis(number),
            raw_input: format!("snapshot {number}"),
            start_reported: false,
        }
    }

    fn settings_with_interactive_detect_ms(interactive_detect_ms: u64) -> RuntimeSettings {
        RuntimeSettings {
            model: "test-model".to_string(),
            reasoning_effort: None,
            api_base: "http://localhost".to_string(),
            api_key: "test-key".to_string(),
            timeout_ms: 1_000,
            max_output_tokens: 100,
            progress_max_output_tokens: 50,
            max_input_chars: 1_000,
            max_collected_bytes: 10_000,
            progress_mode: ProgressMode::Stats,
            progress_interval_ms: 20_000,
            interactive_detect_ms,
            retry_count: 0,
            retry_backoff_ms: 1,
        }
    }
}
