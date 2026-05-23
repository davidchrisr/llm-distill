---
name: llm-distill
description: Use when running commands that may produce long output and the output is only needed as agent context. Summarize builds, tests, logs, searches, installs, and CLI diagnostics with distill instead of reading raw output.
---

# LLM Distill

Use `distill` to compress long, non-interactive command output into the specific answer the agent needs.

## When To Use

- Use for commands that may emit long logs: builds, tests, linters, package installs, service logs, large searches, migrations, framework CLIs, and validation suites.
- Use when the raw output is only needed for agent reasoning, not as an exact artifact for the user.
- Skip `distill` when exact raw output matters, the command is intentionally interactive, output is already small, or the user asked to see the literal output.

## Prompt Shape

Give `distill` an explicit task and requested output shape.

Good prompts:

```sh
some-test-command | distill "Did the tests pass? Return PASS or FAIL, then list failing tests and actionable errors only."
some-build-command | distill "Summarize build result. Return PASS/FAIL and the first root-cause error if any."
some-log-command | distill "Extract only errors, warnings, request IDs, and timestamps relevant to the failure."
git diff | distill "List changed files and one concise behavior summary for each."
```

Prefer constrained answers such as `PASS/FAIL`, failing test names, root-cause errors, changed files, warnings that matter, and next action. Avoid broad prompts like "summarize this".

## Command Form

Pipe stdout into `distill` by default. Host command stderr bypasses the pipe and remains visible directly in tool output, which is usually preferable when stderr is small.

```sh
command | distill --max-output-tokens 800 "Return PASS/FAIL and important warnings/errors."
```

For host commands, merge stderr only when stderr is noisy enough that it should be summarized by `distill` instead of shown raw:

```sh
command 2>&1 | distill --max-output-tokens 800 "Return PASS/FAIL and important warnings/errors."
```

For Docker-wrapped commands, put `2>&1` inside the container shell payload by default so `distill` sees complete inner command output while the outer Docker command remains easy to approve or whitelist:

```sh
docker exec my-container sh -c "actual command 2>&1" | distill "Return PASS/FAIL and important warnings/errors."
```

This matters in agent sandboxes that split commands at pipes. The upstream command and `distill` may need separate approvals or prefix rules. Keep the upstream segment whitelistable, and prefer a narrow saved prefix for `distill` itself, for example `["distill"]`, rather than broad shell or language-runtime prefixes.

For context gathering in containerized workspaces, run scoped and directed `rg` searches inside the relevant container, then pipe the outer Docker command to `distill`. Put `2>&1` inside the container shell payload when stderr should be summarized. Tell `distill` what you are looking for:

```sh
docker exec my-container sh -c "cd /workspace && rg -n 'InvoiceService|createInvoice|invoice total' src tests 2>&1" \
  | distill --max-output-tokens 900 \
    "I am investigating how invoices are created and totals are calculated. Summarize only matches relevant to that goal. Return likely relevant files, key symbols/functions, repeated patterns, and 5-8 next files or symbols to inspect. Do not list every match."
```

## Runtime Behavior

- `distill` waits for upstream EOF before printing the final answer.
- If upstream output is empty or whitespace only, `distill` may exit successfully without printing a final answer.
- With `--prev-response-id`, `distill` can continue from an earlier Responses API result. Use `--from-context` when asking a follow-up without new command output.
- If output appears to stop at an interactive prompt, `distill` can exit with an interactive-detection error instead of waiting forever.
- If `distill` cannot reach its API endpoint from a sandbox, it may fail with a local connection error such as `Operation not permitted`; rerun with the narrow approval required for `distill` instead of broadening the upstream command.

## Useful Options

- `--max-output-tokens N`: cap final answer length.
- `--max-collected-bytes N`: bound retained command output for very large logs.
- `--progress-mode stats`: show periodic byte/line progress without extra model summaries.
- `--progress-mode summary`: produce interim summaries for very long commands; use only when model-readable progress is worth extra calls.
- `--progress-interval-ms N`: tune progress update frequency.
- `--interactive-detect-ms N`: adjust or disable prompt-like interactive detection.
- `-r`, `--resumable`: print the final response ID to stderr so later commands can continue from it.
- `-p ID`, `--prev-response-id ID`: continue from a previous final response ID.
- `-c`, `--from-context`: do not read stdin; ask using only the previous response chain. Requires `--prev-response-id`.

Example for long-running output:

```sh
long-command 2>&1 | distill \
  --progress-mode stats \
  --progress-interval-ms 30000 \
  --max-output-tokens 1000 \
  "Track the run. Return PASS/FAIL, root-cause errors, and any warnings requiring action."
```

## Follow-Up Questions

Use `--resumable` when command output may need later follow-up questions:

```sh
some-test-command | distill -r "Return PASS/FAIL and root-cause errors."
```

`distill` prints the final answer to stdout and the response ID to stderr:

```text
distill: response id: resp_...
```

Use that ID with `--prev-response-id`. If there is no new command output, add `--from-context`:

```sh
distill -c -p resp_... -r "Which warning is actionable?"
```

If there is new command output, pipe it and pass the previous response ID:

```sh
next-command | distill -p resp_... -r "Compare this output with the previous run."
```

Prefer continuing from the latest final response ID for a linear thread. Reusing an older response ID is useful for branching from that point.

## Reporting Back

When answering the user, relay the distilled result and mention if raw output was not inspected. If `distill` failed and exact output is not needed, rerun with the appropriate narrow permission or command shape before drawing conclusions.
