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
}
