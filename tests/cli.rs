use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::{Child, Command, Output, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

struct MockApi {
    base: String,
    stop: Arc<AtomicBool>,
    handle: thread::JoinHandle<Vec<Value>>,
}

impl MockApi {
    fn start(redirect: Option<String>) -> Self {
        Self::with_delay(redirect, Duration::ZERO)
    }

    fn with_delay(redirect: Option<String>, delay: Duration) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            while !stopped.load(Ordering::Acquire) {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("mock API accept failed: {error}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut raw = Vec::new();
                let mut buffer = [0; 4096];
                let (body_start, body_length) = loop {
                    let size = stream.read(&mut buffer).unwrap();
                    assert_ne!(size, 0, "request ended before its headers");
                    raw.extend_from_slice(&buffer[..size]);
                    if let Some(end) = raw.windows(4).position(|part| part == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&raw[..end]);
                        assert!(headers.starts_with("POST /v1/responses HTTP/1.1"));
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                name.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse::<usize>().unwrap())
                            })
                            .unwrap();
                        break (end + 4, length);
                    }
                };
                while raw.len() < body_start + body_length {
                    let size = stream.read(&mut buffer).unwrap();
                    assert_ne!(size, 0, "request ended before its body");
                    raw.extend_from_slice(&buffer[..size]);
                }
                requests.push(
                    serde_json::from_slice(&raw[body_start..body_start + body_length]).unwrap(),
                );
                thread::sleep(delay);
                let (status, headers, body) = match &redirect {
                    Some(location) => (
                        "307 Temporary Redirect",
                        format!("Location: {location}\r\n"),
                        "redirect".to_string(),
                    ),
                    None => (
                        "200 OK",
                        String::new(),
                        json!({"id": "resp_test", "output_text": "answer"}).to_string(),
                    ),
                };
                write!(stream, "HTTP/1.1 {status}\r\n{headers}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
            requests
        });
        Self { base, stop, handle }
    }

    fn finish(self) -> Vec<Value> {
        self.stop.store(true, Ordering::Release);
        self.handle.join().unwrap()
    }
}

fn spawn_cli(base: &str, args: &[&str]) -> Child {
    Command::new(env!("CARGO_BIN_EXE_distill"))
        .env_clear()
        .env("DISTILL_API_KEY", "test-key")
        .args([
            "--config",
            ".local/release-hardening/cli-test-config-missing.json",
            "--api-base",
            base,
            "--timeout-ms",
            "2000",
            "--retry-count",
            "0",
        ])
        .args(args)
        .arg("Summarize output")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn run_cli(base: &str, args: &[&str], input: &[u8]) -> Output {
    let mut child = spawn_cli(base, args);
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn storage_requires_a_chaining_mode_and_default_model_is_luna() {
    for (args, stored, previous) in [
        (vec![], false, false),
        (vec!["--resumable"], true, false),
        (vec!["--prev-response-id", "resp_previous"], true, true),
        (
            vec!["--from-context", "--prev-response-id", "resp_previous"],
            true,
            true,
        ),
        (vec!["--progress-mode", "summary"], true, false),
    ] {
        let api = MockApi::start(None);
        let input = if args.contains(&"--from-context") {
            b"".as_slice()
        } else {
            b"test log\n".as_slice()
        };
        let output = run_cli(&api.base, &args, input);
        let requests = api.finish();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"answer\n");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0]["store"], stored);
        assert_eq!(requests[0]["model"], "gpt-6-luna");
        assert_eq!(requests[0]["reasoning"]["effort"], "low");
        assert_eq!(requests[0].get("previous_response_id").is_some(), previous);
    }
}

#[test]
fn output_between_half_and_full_retention_limit_reaches_the_api() {
    let api = MockApi::start(None);
    let output = run_cli(
        &api.base,
        &["--max-collected-bytes", "20"],
        b"0123456789FAIL\n",
    );
    let requests = api.finish();
    assert!(output.status.success());
    assert!(
        requests[0]["input"]
            .as_str()
            .unwrap()
            .contains("0123456789FAIL")
    );
}
