# llm-distill

A Rust CLI that reduces context costs for expensive AI models by using
smaller, cheaper models to summarize verbose tool output. Build logs, test
results, and diagnostics become concise answers, so the main agent spends fewer
tokens processing raw output.

Pipe tool output and a focused question into `distill`. The distillation model
reads the output, and the main coding model receives the concise answer.

llm-distill targets the OpenAI Responses API. `--model` selects the distillation
model, which defaults to `gpt-6-luna` with `low` reasoning effort.

## Usage

```sh
cmd | distill "Did it pass? Return PASS or FAIL with failing test names."
cargo test | distill --model gpt-6-luna --api-base https://api.openai.com/v1 "Summarize failures."
```

Pipe stdout into `distill` by default. Merge stderr only when stderr is noisy
enough that it should be summarized instead of shown directly:

```sh
cmd 2>&1 | distill "Did it pass? Return PASS or FAIL with failing test names."
```

Configuration uses explicit verbs:

```sh
distill config show
distill config get model
distill config set model gpt-6-luna
distill config set reasoning-effort medium
distill config set api-base https://api.openai.com/v1
distill config set timeout-ms 90000
distill config set progress-max-output-tokens 128
distill config set max-input-chars 100000
distill config set max-collected-bytes 1048576
distill config set progress-mode stats
distill config set progress-interval-ms 30000
distill config set interactive-detect-ms 10000
distill config set retry-count 5
distill config set retry-backoff-ms 1000
```

Environment variables:

- `DISTILL_MODEL`
- `DISTILL_REASONING_EFFORT`
- `DISTILL_API_BASE`
- `DISTILL_API_KEY`
- `DISTILL_TIMEOUT_MS`
- `DISTILL_MAX_OUTPUT_TOKENS`
- `DISTILL_PROGRESS_MAX_OUTPUT_TOKENS`
- `DISTILL_MAX_INPUT_CHARS`
- `DISTILL_MAX_COLLECTED_BYTES`
- `DISTILL_PROGRESS_MODE`
- `DISTILL_PROGRESS_INTERVAL_MS`
- `DISTILL_INTERACTIVE_DETECT_MS`
- `DISTILL_RETRY_COUNT`
- `DISTILL_RETRY_BACKOFF_MS`

## Progress and limits

`distill` reads stdin continuously, but prints the final answer to stdout only
after upstream EOF. Progress messages go to stderr so stdout remains usable as
the distilled result.

- `progress-mode=stats` prints periodic byte and line counts without extra model
  calls.
- `progress-mode=summary` can make one Responses API call per progress interval
  that has new output, writes interim summaries to stderr, chains them with
  `previous_response_id`, and prints the final answer to stdout.
- `progress-max-output-tokens` controls interim summary length.
  `max-output-tokens` controls the final answer length.
- `max-collected-bytes` bounds local command-output retention. Large output
  keeps head and tail with an omission marker.
- Input buffering is limited to eight 8 KiB chunks plus one chunk being read.
  Summary progress keeps one queued snapshot, one in flight, and one pending
  enqueue. Each retained output or snapshot is bounded by `max-collected-bytes`,
  plus its omission marker. Slow API calls apply backpressure to upstream
  commands.
- `interactive-detect-ms` errors if output appears to be waiting at an
  interactive prompt for that long. Set it to `0` to disable the guard.
- `retry-count` and `retry-backoff-ms` apply to transient API failures only.

## Follow-up questions

Use `--resumable` or `-r` to print the final Responses API response ID to
stderr:

```sh
long-command | distill -r "Return PASS/FAIL and root-cause errors."
```

Use `--prev-response-id` or `-p` to continue from that response. Add
`--from-context` or `-c` when asking a follow-up without new stdin:

```sh
distill -c -p resp_123 -r "Which warning is actionable?"
```

If new command output is available, pipe it and pass the previous response ID:

```sh
next-command | distill -p resp_123 -r "Compare this output with the previous run."
```

## Interface shape

The settings model is intentionally split into three layers:

- `PersistedSettings`: values stored in the user config file.
- `EnvSettings`: values read from `DISTILL_*` environment variables.
- `CliOverrides`: values supplied on the current invocation.

Those are resolved once into `RuntimeSettings` with this precedence:

```text
defaults < config file < environment < CLI flags
```

The `api-base` setting is the base URL for API requests. The runtime appends the
selected API resource path itself. For OpenAI, the default API base is
`https://api.openai.com/v1`.

Persisted config uses the platform config directory for `llm-distill`. Reasoning
effort defaults to `low` and can be overridden by config, environment, or CLI.
The effort value is passed through to the API without restricting it to a fixed
list.

## Privacy and transport

Piped command output and your question are sent to the configured API endpoint.
Output is not redacted. Do not pipe credentials, personal data, or confidential
logs unless that endpoint is approved to receive them.

Ordinary requests explicitly set `store: false`. `--resumable`,
`--prev-response-id`, and `progress-mode=summary` enable server-side response
storage for chaining and follow-up questions. Provider logging and retention
rules still apply. See
[OpenAI data controls](https://developers.openai.com/api/docs/guides/your-data).

API endpoints may use HTTP or HTTPS.

## Local development

Set up mount targets before starting the container:

```sh
mkdir -p .local/mnt/.cargo .local/mnt/.config
touch .local/mnt/.bash_history
docker compose up --build -d
```

Example container workflow:

```sh
docker compose run --rm dev cargo fmt
docker compose run --rm dev cargo test
docker compose run --rm dev cargo run -- --help
```

## Validation

Run validation inside the dev container:

```sh
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

## Sandboxed build

Build a macOS binary with dependency and build writes confined to this project:

```sh
scripts/sandboxed-build.sh
```

The script generates sandbox profiles under `.local/`, then runs:

- `cargo fetch --locked` in a network-enabled sandbox.
- `cargo build --release --locked --offline` in a separate sandbox without
  network access.

Both phases use project-local state:

- `HOME=.local/home`
- `CARGO_HOME=.local/cargo-home`
- `CARGO_TARGET_DIR=.local/target` for the build phase
- `TMPDIR=.local/tmp`

The profiles import Apple's `bsd.sb` base profile, allow the project and
resolved Rust toolchain paths, and allow the Xcode app contents, Darwin temp
directory, system SSL config directory, and Xcode license plist needed by
Apple's linker tooling. Host home is not used as `HOME` inside the sandbox.

Install the binary to a user-writable bin directory:

```sh
mkdir -p "$HOME/.local/bin"
sudo cp ".local/target/release/distill" "/usr/local/bin/distill"
```

Make sure `~/.local/bin` is on `PATH`, then test:

```sh
distill --help
distill config show
```

## Agent usage

For agent sessions, install and use `skills/llm-distill` skill.

## License

[MIT](LICENSE).
