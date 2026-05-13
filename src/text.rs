pub fn normalize_for_model(input: &str) -> String {
    strip_ansi(input)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .split("\n\n\n")
        .collect::<Vec<_>>()
        .join("\n\n")
        .trim()
        .to_string()
}

pub fn has_prompt_like_tail(input: &str) -> bool {
    let tail = input
        .chars()
        .rev()
        .take(1024)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    let normalized = normalize_for_model(&tail);
    let trimmed = normalized.trim_end().to_ascii_lowercase();

    PROMPT_SUFFIXES
        .iter()
        .any(|suffix| trimmed.ends_with(suffix))
        || ends_with_choice_prompt(&trimmed)
}

const PROMPT_SUFFIXES: &[&str] = &[
    "password:",
    "passphrase:",
    "continue?",
    "proceed?",
    "are you sure?",
];

fn ends_with_choice_prompt(input: &str) -> bool {
    ["[y/n]", "[n/y]", "(y/n)", "(n/y)"]
        .iter()
        .any(|suffix| input.ends_with(suffix))
}

pub fn ensure_trailing_newline(input: &str) -> String {
    if input.ends_with('\n') {
        input.to_string()
    } else {
        format!("{input}\n")
    }
}

fn strip_ansi(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            output.push(ch);
            continue;
        }

        if chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_removes_common_ansi_sequences() {
        assert_eq!(normalize_for_model("\u{1b}[31mFAIL\u{1b}[0m\r\n"), "FAIL");
    }

    #[test]
    fn detects_prompt_like_tail() {
        assert!(has_prompt_like_tail("Proceed? [y/N]"));
        assert!(has_prompt_like_tail("password: "));
        assert!(has_prompt_like_tail("Are you sure?"));
    }

    #[test]
    fn ignores_prompt_words_not_at_tail() {
        assert!(!has_prompt_like_tail("password: accepted\ncontinuing"));
    }
}
