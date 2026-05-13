use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct OutputCollector {
    head: Vec<u8>,
    tail: VecDeque<u8>,
    total_bytes: usize,
    newline_count: usize,
    ends_with_newline: bool,
    omitted_bytes: usize,
    max_collected_bytes: usize,
}

impl OutputCollector {
    pub fn new(max_collected_bytes: usize) -> Self {
        Self {
            head: Vec::new(),
            tail: VecDeque::new(),
            total_bytes: 0,
            newline_count: 0,
            ends_with_newline: true,
            omitted_bytes: 0,
            max_collected_bytes,
        }
    }

    pub fn push(&mut self, chunk: &[u8]) {
        self.total_bytes += chunk.len();
        self.newline_count += chunk.iter().filter(|byte| **byte == b'\n').count();
        if let Some(last) = chunk.last() {
            self.ends_with_newline = *last == b'\n';
        }

        if self.max_collected_bytes == 0 {
            self.omitted_bytes += chunk.len();
            return;
        }

        let head_cap = self.head_cap();
        let head_remaining = head_cap.saturating_sub(self.head.len());
        let head_len = head_remaining.min(chunk.len());
        self.head.extend_from_slice(&chunk[..head_len]);

        for byte in &chunk[head_len..] {
            self.tail.push_back(*byte);
            if self.tail.len() > self.tail_cap() {
                self.tail.pop_front();
                self.omitted_bytes += 1;
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.total_bytes == 0
    }

    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    pub fn total_lines(&self) -> usize {
        self.newline_count + usize::from(self.total_bytes > 0 && !self.ends_with_newline)
    }

    pub fn materialize(&self) -> String {
        if self.omitted_bytes == 0 {
            return String::from_utf8_lossy(&self.head).into_owned();
        }

        let head = String::from_utf8_lossy(&self.head);
        let tail_bytes = self.tail.iter().copied().collect::<Vec<_>>();
        let tail = String::from_utf8_lossy(&tail_bytes);
        format!(
            "{head}\n... [{} bytes omitted, {} total lines observed] ...\n{tail}",
            self.omitted_bytes,
            self.total_lines()
        )
    }

    fn head_cap(&self) -> usize {
        self.max_collected_bytes.div_ceil(2)
    }

    fn tail_cap(&self) -> usize {
        self.max_collected_bytes / 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_short_output_exactly() {
        let mut collector = OutputCollector::new(20);
        collector.push(b"abc\n");

        assert_eq!(collector.materialize(), "abc\n");
        assert_eq!(collector.total_bytes(), 4);
        assert_eq!(collector.total_lines(), 1);
    }

    #[test]
    fn counts_unterminated_partial_line() {
        let mut collector = OutputCollector::new(20);
        collector.push(b"abc");

        assert_eq!(collector.total_lines(), 1);
    }

    #[test]
    fn keeps_head_and_tail_for_long_output() {
        let mut collector = OutputCollector::new(10);
        collector.push(b"0123456789abcdef");

        let output = collector.materialize();
        assert!(output.starts_with("01234"));
        assert!(output.ends_with("bcdef"));
        assert!(output.contains("6 bytes omitted"));
        assert_eq!(collector.total_bytes(), 16);
    }

    #[test]
    fn zero_limit_counts_everything_as_omitted() {
        let mut collector = OutputCollector::new(0);
        collector.push(b"abc\n");

        let output = collector.materialize();
        assert!(output.contains("4 bytes omitted"));
        assert!(output.contains("1 total lines observed"));
    }
}
