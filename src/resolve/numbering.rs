//! Heading, figure, and equation numbering (references.md §2).
//! Owner: T17. Stub created by T00.

use codex::numeral_systems::NamedNumeralSystem;

/// Typst's counting symbols and their literal prefixes and final suffix.
#[derive(Debug, Clone)]
pub struct NumberingPattern {
    pieces: Vec<(String, NamedNumeralSystem)>,
    suffix: String,
}

impl NumberingPattern {
    /// Reads every supported counting symbol; a pattern without one is invalid.
    pub fn parse(pattern: &str) -> Option<NumberingPattern> {
        let mut pieces = Vec::new();
        let mut handled = 0;
        for (at, c) in pattern.char_indices() {
            let Some(system) =
                NamedNumeralSystem::from_shorthand(c.encode_utf8(&mut [0; 4]))
            else {
                continue;
            };
            pieces.push((pattern[handled..at].to_string(), system));
            handled = at + c.len_utf8();
        }
        (!pieces.is_empty()).then(|| NumberingPattern {
            pieces,
            suffix: pattern[handled..].to_string(),
        })
    }

    /// Formats display or trimmed reference numbers, reporting fallback values.
    pub fn apply(&self, numbers: &[u64], trimmed: bool) -> (String, Vec<u64>) {
        let (text, failures) = self.apply_with_systems(numbers, trimmed);
        (text, failures.into_iter().map(|(number, _)| number).collect())
    }

    pub(crate) fn apply_with_systems(
        &self, numbers: &[u64], trimmed: bool,
    ) -> (String, Vec<(u64, &'static str)>) {
        let mut out = String::new();
        let mut failures = Vec::new();
        let mut numbers = numbers.iter();
        for (index, ((prefix, system), &number)) in
            self.pieces.iter().zip(&mut numbers).enumerate()
        {
            if index > 0 || !trimmed {
                out.push_str(prefix);
            }
            represent(&mut out, &mut failures, *system, number);
        }
        for ((prefix, system), &number) in
            self.pieces.last().into_iter().cycle().zip(numbers)
        {
            out.push_str(if prefix.is_empty() {
                &self.suffix
            } else {
                prefix
            });
            represent(&mut out, &mut failures, *system, number);
        }
        if !trimmed {
            out.push_str(&self.suffix);
        }
        (out, failures)
    }
}

fn represent(
    out: &mut String, failures: &mut Vec<(u64, &'static str)>,
    system: NamedNumeralSystem, number: u64,
) {
    match system.system().represent(number) {
        Ok(representation) => out.push_str(&representation.to_string()),
        Err(_) => {
            out.push_str(&number.to_string());
            failures.push((number, system.name()));
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Counters {
    pub headings: Vec<u64>,
    image: u64,
    table: u64,
    listing: u64,
    equation: u64,
}

impl Counters {
    pub fn heading(&mut self, level: u8, section: bool) {
        let depth = usize::from(level.max(1));
        self.headings.resize(self.headings.len().max(depth), 0);
        self.headings[depth - 1] += 1;
        self.headings.truncate(depth);
        if level == 1 && section {
            self.image = 0;
            self.table = 0;
            self.listing = 0;
            self.equation = 0;
        }
    }

    pub fn figure(
        &mut self, kind: crate::ir::FigureKind, section: bool,
    ) -> String {
        let counter = match kind {
            crate::ir::FigureKind::Image | crate::ir::FigureKind::Other => {
                &mut self.image
            }
            crate::ir::FigureKind::Table => &mut self.table,
            crate::ir::FigureKind::Listing => &mut self.listing,
        };
        *counter += 1;
        let number = *counter;
        self.display(number, section)
    }

    pub fn equation(&mut self, section: bool) -> String {
        self.equation += 1;
        self.display(self.equation, section)
    }

    fn display(&self, number: u64, section: bool) -> String {
        if section {
            format!("{}.{number}", self.headings.first().copied().unwrap_or(0))
        } else {
            number.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_table() {
        for (pattern, numbers, display, reference) in [
            ("1.1", vec![1], "1", "1"),
            ("1.1", vec![1, 2], "1.2", "1.2"),
            ("1.", vec![1], "1.", "1"),
            ("1.", vec![1, 1, 1], "1.1.1.", "1.1.1"),
            ("A.1", vec![1], "A", "A"),
            ("A.1", vec![1, 0, 1], "A.0.1", "A.0.1"),
            ("I.", vec![1, 1], "I.I.", "I.I"),
            ("1.a", vec![2], "2", "2"),
            ("1.a", vec![2, 0, 1], "2.0.a", "2.0.a"),
            ("(i)", vec![2], "(ii)", "ii"),
            ("(i)", vec![2, 1], "(ii(i)", "ii(i"),
            ("(1)", vec![3], "(3)", "3"),
        ] {
            let parsed = NumberingPattern::parse(pattern).unwrap();
            assert_eq!(parsed.apply(&numbers, false).0, display);
            assert_eq!(parsed.apply(&numbers, true).0, reference);
        }
        assert_eq!(
            NumberingPattern::parse("1.a").unwrap().apply(&[2, 0, 1], false).1,
            [0]
        );
        assert!(NumberingPattern::parse("x").is_none());
        assert!(NumberingPattern::parse("").is_none());
    }
}
