//! Prose break positions travel separately from literal TeX bytes.

use std::ops::Range;

#[derive(Clone, Default)]
pub(super) struct Output {
    pub text: String,
    breaks: Vec<Range<usize>>,
}

impl Output {
    pub fn literal(&mut self, text: &str) {
        self.text.push_str(text);
    }

    pub fn prose(&mut self, text: &str, map: bool) {
        let mut start = 0;
        for (index, ch) in text.char_indices() {
            if ch != ' ' {
                continue;
            }
            self.literal(&super::super::escape::text(&text[start..index], map));
            let offset = self.text.len();
            self.text.push(' ');
            if let Some(previous) = self.breaks.last_mut()
                && previous.end == offset
            {
                previous.end += 1;
            } else {
                self.breaks.push(offset..offset + 1);
            }
            start = index + 1;
        }
        self.literal(&super::super::escape::text(&text[start..], map));
    }

    pub fn append(&mut self, other: Self) {
        let offset = self.text.len();
        self.text.push_str(&other.text);
        for range in other.breaks {
            let range = range.start + offset..range.end + offset;
            if let Some(previous) = self.breaks.last_mut()
                && previous.end == range.start
            {
                previous.end = range.end;
            } else {
                self.breaks.push(range);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn wrapped(self, width: u32) -> String {
        if width == 0 || self.breaks.is_empty() {
            return self.text;
        }
        let mut output = String::with_capacity(self.text.len());
        let mut start = 0;
        let mut choices = self.breaks.iter().peekable();
        while start < self.text.len() {
            let end = self.text[start..]
                .find('\n')
                .map_or(self.text.len(), |end| start + end);
            let mut byte = start;
            let mut character = 0;
            let mut candidates = Vec::new();
            while choices.peek().is_some_and(|range| range.start < end) {
                let range = choices.next().unwrap();
                if range.start < start {
                    continue;
                }
                character += self.text[byte..range.start].chars().count();
                let from = character;
                character += range.end - range.start;
                byte = range.end;
                // Breaking a final run would invent an empty line.
                if range.start > start && range.end < end {
                    candidates.push((range.clone(), from, character));
                }
            }
            let total = character + self.text[byte..end].chars().count();
            let mut line_byte = start;
            let mut line_character = 0;
            let mut next = 0;
            while total - line_character > width as usize {
                while next < candidates.len()
                    && candidates[next].0.start <= line_byte
                {
                    next += 1;
                }
                if next == candidates.len() {
                    break;
                }
                let limit = line_character + width as usize;
                let mut chosen = next;
                while chosen + 1 < candidates.len()
                    && candidates[chosen + 1].1 <= limit
                {
                    chosen += 1;
                }
                let (range, _, to) = &candidates[chosen];
                output.push_str(&self.text[line_byte..range.start]);
                output.push('\n');
                line_byte = range.end;
                line_character = *to;
                next = chosen + 1;
            }
            output.push_str(&self.text[line_byte..end]);
            if end < self.text.len() {
                output.push('\n');
            }
            start = end.saturating_add(1);
        }
        output
    }
}

impl From<String> for Output {
    fn from(text: String) -> Self {
        Self { text, breaks: Vec::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_only_breaks_tracked_prose_and_counts_unicode_scalars() {
        let mut output = Output::default();
        output.prose("éé  abc def", false);
        output.literal(" \\(raw math with spaces\\)");
        assert_eq!(
            output.wrapped(6),
            "éé\nabc\ndef \\(raw math with spaces\\)"
        );
    }

    #[test]
    fn nested_output_preserves_break_offsets_and_never_adds_empty_lines() {
        let mut output = Output::default();
        output.literal("\\emph{");
        let mut inner = Output::default();
        inner.prose("longword   after", false);
        output.append(inner);
        output.literal("}\n");
        assert_eq!(output.wrapped(5), "\\emph{longword\nafter}\n");
    }

    #[test]
    fn zero_width_and_outputs_without_prose_breaks_are_exact_passthrough() {
        let mut output = Output::default();
        output.literal("\nRAW  α\t");
        output.prose("text   with spaces", false);
        output.literal("\n\\url{a b}\n");
        let exact = output.text.clone();
        assert_eq!(output.wrapped(0), exact);
        let exact = "raw  with spaces\n\\(math with spaces\\)\n";
        let output: Output = exact.to_owned().into();
        assert_eq!(output.wrapped(1), exact);
    }

    #[test]
    fn appended_and_adjacent_text_runs_merge_into_one_break_opportunity() {
        let mut output = Output::default();
        output.prose("a ", false);
        let mut nested = Output::default();
        nested.prose("  ", false);
        nested.append(Output::default());
        let mut tail = Output::default();
        tail.prose(" b", false);
        nested.append(tail);
        output.append(nested);
        assert_eq!(output.text, "a    b");
        assert_eq!(output.breaks.len(), 1);
        assert_eq!(output.breaks[0], 1..5);
        assert_eq!(output.wrapped(2), "a\nb");
    }

    #[test]
    fn trailing_runs_and_leading_runs_never_create_an_empty_line() {
        for (source, width, expected) in [
            ("word   ", 3, "word   "),
            ("one two   ", 7, "one\ntwo   "),
            ("  a   b  ", 3, "  a\nb  "),
            ("   ", 1, "   "),
        ] {
            let mut output = Output::default();
            output.prose(source, false);
            assert_eq!(output.wrapped(width), expected);
        }
    }

    #[test]
    fn existing_hard_newlines_and_blank_lines_remain_in_place() {
        let mut output = Output::default();
        output.prose("aa bb\n\ncc dd\n", false);
        assert_eq!(output.wrapped(2), "aa\nbb\n\ncc\ndd\n");
        let mut output = Output::default();
        output.prose("aa ", false);
        output.literal("\n");
        let mut nested = Output::default();
        nested.prose("bb cc", false);
        output.append(nested);
        assert_eq!(output.wrapped(2), "aa \nbb\ncc");
    }

    #[test]
    fn unicode_width_counts_scalars_and_only_ascii_spaces_are_breakable() {
        let mut output = Output::default();
        output.prose("αα ββ γγ", false);
        assert_eq!(output.wrapped(5), "αα ββ\nγγ");
        let mut output = Output::default();
        output.prose("a\tb a\u{a0}b a\u{202f}b", false);
        assert_eq!(output.wrapped(1), "a\tb\na~b\na\\,b");
    }

    #[test]
    fn literal_spaces_stay_atomic_between_real_prose_breaks() {
        let literal = "\\(a + b\\) \\url{raw URL with spaces} RAW  bytes";
        let mut output = Output::default();
        output.prose("before ", false);
        output.literal(literal);
        output.prose(" after", false);
        assert_eq!(output.wrapped(5), format!("before\n{literal}\nafter"));
    }

    #[test]
    fn long_unbreakable_source_is_retained_without_splitting_or_reescaping() {
        let word = "α".repeat(64 * 1024);
        let mut output = Output::default();
        output.prose(&word, false);
        output.prose(" tail words", false);
        assert_eq!(output.wrapped(5), format!("{word}\ntail\nwords"));
    }
}
