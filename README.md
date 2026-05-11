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

## Usage Draft

```sh
cmd 2>&1 | distill "Did it pass? Return PASS or FAIL with failing test names."
distill --model gpt-5.4-mini --api-base https://api.openai.com/v1 "Summarize the errors."
```

Configuration uses explicit verbs:

```sh
distill config show
distill config get model
distill config set model gpt-5.4-mini
distill config set api-base https://api.openai.com/v1
distill config set timeout-ms 90000
distill config set max-input-chars 24000
```

Environment variables:

- `DISTILL_MODEL`
- `DISTILL_API_BASE`
- `DISTILL_API_KEY`
- `DISTILL_TIMEOUT_MS`
- `DISTILL_MAX_OUTPUT_TOKENS`
- `DISTILL_MAX_INPUT_CHARS`

## Local Development

Set up mount targets before starting the container:

```sh
mkdir -p .local/mnt/.cargo .local/mnt/.config
touch .local/mnt/.bash_history
docker compose up --build -d
```

Example container workflow:

```sh
docker compose run --rm dev cargo fmt --check
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

Build a macOS binary with writes confined to this project:

```sh
scripts/sandboxed-build.sh
```

The script runs `cargo fetch --locked` before entering the sandbox, then builds with `--offline` inside the sandbox.

Install the binary to a user-writable bin directory:

```sh
mkdir -p "$HOME/.local/bin"
cp ".local/target/release/distill" "/usr/local/bin/distill"
```

Make sure `~/.local/bin` is on `PATH`, then test:

```sh
distill --help
distill config show
```

## AGENTS.md Usage

For agent sessions, add this project note to `AGENTS.md`:

```md
## llm-distill Usage

- Use `distill` to reduce token usage when command output is only needed as agent context.
- Pipe long, non-interactive command output through `distill` before reading it into the conversation.
- Give `distill` an explicit task. State what the agent needs to know and what shape the distilled output should have.

  ```sh
  cargo test 2>&1 | distill "Did the tests pass? Return PASS or FAIL, followed by failing test names if any."
  git diff 2>&1 | distill "List changed files and one concise summary for each."
  rg -n "TODO|FIXME" . 2>&1 | distill "Return only file paths, one per line."
  ```

- Skip `distill` when exact raw output is required, the command is interactive, or the output is already small enough for direct agent context.
- Wait for `distill` to finish before continuing.
```
