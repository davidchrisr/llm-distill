# llm-distill

Rust CLI tool for compressing command output into the answer another agent needs.

llm-distill targets the OpenAI Responses API.

## Interface Shape

The settings model is intentionally split into three layers:

- `PersistedSettings`: values stored in the user config file.
- `EnvSettings`: values read from `DISTILL_*` environment variables.
- `CliOverrides`: values supplied on the current invocation.

Those are resolved once into `RuntimeSettings` with this precedence:

```text
defaults < config file < environment < CLI flags
```

The `api-base` setting is the base URL for API requests. The runtime appends the selected
API resource path itself. For OpenAI, the default API base is `https://api.openai.com/v1`.

Persisted config uses the platform config directory for `llm-distill`.

## Usage

```sh
cmd 2>&1 | distill "Did it pass? Return PASS or FAIL with failing test names."
cargo test 2>&1 | distill --model gpt-5.4-mini --api-base https://api.openai.com/v1 "Summarize failures."
```

Configuration uses explicit verbs:

```sh
distill config show
distill config get model
distill config set model gpt-5.4-mini
distill config set api-base https://api.openai.com/v1
distill config set timeout-ms 90000
distill config set progress-max-output-tokens 128
distill config set max-input-chars 24000
distill config set max-collected-bytes 1048576
distill config set progress-mode stats
distill config set progress-interval-ms 30000
distill config set interactive-detect-ms 10000
distill config set retry-count 5
distill config set retry-backoff-ms 1000
```

Environment variables:

- `DISTILL_MODEL`
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

## Progress And Limits

`distill` reads stdin continuously, but prints the final answer to stdout only after upstream EOF. Progress messages go to stderr so stdout remains usable as the distilled result.

- `progress-mode=stats` prints periodic byte and line counts without extra model calls.
- `progress-mode=summary` can make one Responses API call per progress interval that has new output, writes interim summaries to stderr, chains them with `previous_response_id`, and prints the final answer to stdout.
- `progress-max-output-tokens` controls interim summary length. `max-output-tokens` controls the final answer length.
- `max-collected-bytes` bounds local command-output retention. Large output keeps head and tail with an omission marker.
- `interactive-detect-ms` errors if output appears to be waiting at an interactive prompt for that long. Set it to `0` to disable the guard.
- `retry-count` and `retry-backoff-ms` apply to transient API failures only.

## Local Development

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

## Sandboxed Build

Build a macOS binary with dependency and build writes confined to this project:

```sh
scripts/sandboxed-build.sh
```

The script generates sandbox profiles under `.local/`, then runs:

- `cargo fetch --locked` in a network-enabled sandbox.
- `cargo build --release --locked --offline` in a separate sandbox without network access.

Both phases use project-local state:

- `HOME=.local/home`
- `CARGO_HOME=.local/cargo-home`
- `CARGO_TARGET_DIR=.local/target` for the build phase
- `TMPDIR=.local/tmp`

The profiles import Apple's `bsd.sb` base profile, allow the project and resolved Rust toolchain paths, and allow the Xcode app contents, Darwin temp directory, system SSL config directory, and Xcode license plist needed by Apple's linker tooling. Host home is not used as `HOME` inside the sandbox.

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

## AGENTS.md Usage

For agent sessions, add this project note to `AGENTS.md`:

````md
## llm-distill Usage

- Use `distill` to reduce token usage when command output is only needed as agent context.
- Pipe long, non-interactive command output through `distill` before reading it into the conversation.
- Give `distill` an explicit task. State what the agent needs to know and what shape the distilled output should have.
- `distill` waits for upstream EOF before printing the final answer to stdout.
- If upstream output is empty or whitespace only, `distill` exits successfully without printing a final answer.
- Local command-output retention is bounded by `max-collected-bytes`; large output keeps head and tail with an omission marker.
- Progress goes to stderr. Use `progress-mode=stats` for periodic byte/line counts, or `progress-mode=summary` for periodic interim summaries.
- Summary progress uses Responses API chaining for interim summaries and `progress-max-output-tokens` for the interim output budget. The final answer still uses `max-output-tokens`.
- `progress-mode=stats` makes no extra model calls. `progress-mode=summary` can make one model call per progress interval that has new output, plus the final call, so use it only when interim model-readable status is worth the added cost.
- If output appears to end at an interactive prompt for `interactive-detect-ms`, `distill` exits with an error instead of waiting indefinitely. Set `interactive-detect-ms` to `0` to disable this guard.
- Tune behavior with CLI flags, `DISTILL_*` env vars, or persisted config. Common knobs: `progress-mode`, `progress-interval-ms`, `progress-max-output-tokens`, `max-collected-bytes`, `interactive-detect-ms`, `retry-count`, and `retry-backoff-ms`.

  ```sh
  cargo test 2>&1 | distill "Did the tests pass? Return PASS or FAIL, followed by failing test names if any."
  cargo build 2>&1 | distill --progress-mode summary --progress-interval-ms 30000 "Track build progress and failures."
  git diff 2>&1 | distill "List changed files and one concise summary for each."
  rg -n "TODO|FIXME" . 2>&1 | distill "Return only file paths, one per line."
  ```

- Skip `distill` when exact raw output is required, the command is intentionally interactive, or the output is already small enough for direct agent context.
- Wait for `distill` to finish before continuing.
````
