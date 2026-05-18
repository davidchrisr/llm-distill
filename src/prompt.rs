#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Prompt {
    pub instructions: String,
    pub input: String,
}

pub fn build_batch_prompt(question: &str, command_output: &str, max_input_chars: usize) -> Prompt {
    Prompt {
        instructions: instructions(false),
        input: format!(
            "Command output:\n{}\n\nQuestion:\n{}",
            fit_input(command_output, max_input_chars),
            question.trim()
        ),
    }
}

pub fn build_progress_prompt(question: &str, new_output: &str, max_input_chars: usize) -> Prompt {
    Prompt {
        instructions: progress_instructions(),
        input: format!(
            "New command output since the previous progress summary:\n{}\n\nFinal question:\n{}",
            fit_input(new_output, max_input_chars),
            question.trim()
        ),
    }
}

pub fn build_final_prompt(
    question: &str,
    retained_output: &str,
    max_input_chars: usize,
    has_progress_context: bool,
) -> Prompt {
    let context_note = if has_progress_context {
        "Earlier progress summaries are available in the Responses conversation chain. Use them together with the retained final command output below."
    } else {
        "No earlier progress summaries are available."
    };

    Prompt {
        instructions: instructions(false),
        input: format!(
            "{context_note}\n\nRetained final command output:\n{}\n\nQuestion:\n{}",
            fit_input(retained_output, max_input_chars),
            question.trim()
        ),
    }
}

pub fn build_watch_prompt(
    question: &str,
    previous: &str,
    current: &str,
    max_input_chars: usize,
) -> Prompt {
    let snapshot_chars = max_input_chars / 2;

    Prompt {
        instructions: instructions(true),
        input: format!(
            "Previous cycle:\n{}\n\nCurrent cycle:\n{}\n\nQuestion:\n{}",
            fit_input(previous, snapshot_chars),
            fit_input(current, snapshot_chars),
            question.trim()
        ),
    }
}

pub fn fit_input(input: &str, max_chars: usize) -> String {
    let input_chars = input.chars().count();

    if input_chars <= max_chars {
        return input.to_string();
    }

    if max_chars == 0 {
        return String::new();
    }

    if max_chars < 32 {
        return input.chars().take(max_chars).collect();
    }

    let mut kept_chars = max_chars;
    let (head_chars, tail_chars, marker) = loop {
        let head_chars = kept_chars / 2;
        let tail_chars = kept_chars - head_chars;
        let omitted = input_chars.saturating_sub(head_chars + tail_chars);
        let marker = format!("\n... [{omitted} chars omitted] ...\n");
        let total_chars = head_chars + tail_chars + marker.chars().count();

        if total_chars <= max_chars || kept_chars == 0 {
            break (head_chars, tail_chars, marker);
        }

        kept_chars = kept_chars.saturating_sub(total_chars - max_chars);
    };

    let head: String = input.chars().take(head_chars).collect();
    let tail: String = input
        .chars()
        .rev()
        .take(tail_chars)
        .collect::<String>()
        .chars()
        .rev()
        .collect();

    format!("{head}{marker}{tail}")
}

fn progress_instructions() -> String {
    [
        "Summarize only the new command output for another agent.",
        "Preserve facts needed to answer the final question later.",
        "Do not produce the final answer unless the new output conclusively contains it.",
        "Do not add preamble, markdown fences, or invented data.",
        "Keep this interim summary concise.",
    ]
    .join(" ")
}

fn instructions(watch_mode: bool) -> String {
    let role = if watch_mode {
        "Compare two consecutive command-output snapshots and report only the relevant change."
    } else {
        "Compress command output for another agent that will act on your answer."
    };

    [
        role,
        "Return only what the user's question asks for.",
        "Do not add preamble, markdown fences, or invented data.",
        "If the requested format is JSON, return raw valid JSON only.",
        "For PASS, FAIL, SAFE, REVIEW, or UNSAFE requests, put the verdict first.",
        "When safety is ambiguous, choose the more cautious label and cite the risky line.",
        "If the output is insufficient, say exactly: `distill: insufficient information.` and explain why",
    ]
    .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_input_keeps_short_input() {
        assert_eq!(fit_input("abc", 10), "abc");
    }

    #[test]
    fn fit_input_keeps_head_and_tail() {
        let input = "a".repeat(100) + &"b".repeat(100) + &"c".repeat(100);
        let fitted = fit_input(&input, 120);

        assert!(fitted.starts_with("a"));
        assert!(fitted.ends_with("c"));
        assert!(fitted.contains("omitted"));
    }

    #[test]
    fn fit_input_keeps_source_text_for_tiny_limits() {
        assert_eq!(fit_input("abcdefghijkl", 10), "abcdefghij");
    }

    #[test]
    fn progress_prompt_marks_output_as_incremental() {
        let prompt = build_progress_prompt("what failed?", "line", 100);

        assert!(prompt.input.contains("New command output"));
        assert!(prompt.input.contains("Final question"));
    }

    #[test]
    fn final_prompt_mentions_chained_progress_when_available() {
        let prompt = build_final_prompt("what failed?", "line", 100, true);

        assert!(prompt.input.contains("Responses conversation chain"));
        assert!(prompt.input.contains("Retained final command output"));
    }
}
