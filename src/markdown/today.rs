//! The `{{today}}` placeholder (markdown-dialect.md §6.1).
//!
//! The walker replaces `{{today}}` in Markdown text with the build date, such
//! as `19 November 2026`. The date follows the PDF timestamp policy without
//! the document date: `[pdf] timestamp`, then `SOURCE_DATE_EPOCH`, then the
//! current time in UTC. Code, raw HTML and a placeholder with an escaped brace
//! (`\{{today}}`) never reach the walker as plain text, so they stay literal.

use std::ops::Range;
use std::time::SystemTime;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::ir::DocMeta;
use crate::typst::compile::pdf_timestamp;

const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August",
    "September", "October", "November", "December",
];

/// Whether `text` holds a `{{today}}` placeholder.
pub(crate) fn contains(text: &str) -> bool {
    find(text).is_some()
}

/// Returns `text` with each `{{today}}` placeholder replaced by `date`.
pub(crate) fn expand(text: &str, date: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(range) = find(rest) {
        out.push_str(&rest[..range.start]);
        out.push_str(date);
        rest = &rest[range.end..];
    }
    out.push_str(rest);
    out
}

/// The build date as day, English month name and year: `19 November 2026`.
pub(crate) fn build_date(config: &Config) -> Result<String> {
    let epoch = std::env::var("SOURCE_DATE_EPOCH").ok();
    date_at(config, epoch.as_deref(), SystemTime::now())
}

fn date_at(
    config: &Config, epoch: Option<&str>, now: SystemTime,
) -> Result<String> {
    let stamp = pdf_timestamp(&config.pdf, &DocMeta::default(), epoch, now)?;
    let Some(((day, month), year)) =
        stamp.day().zip(stamp.month()).zip(stamp.year())
    else {
        return Err(Error::Config("the build timestamp has no date".into()));
    };
    Ok(format!("{day} {} {year}", MONTHS[usize::from(month) - 1]))
}

/// The byte range of the first placeholder in `text`. Spaces or tabs may
/// pad the name, as in `{{ today }}`.
fn find(text: &str) -> Option<Range<usize>> {
    let blank = |c: char| c == ' ' || c == '\t';
    let mut from = 0;
    while let Some(offset) = text[from..].find("{{") {
        let start = from + offset;
        let after = &text[start + 2..];
        let rest = after
            .trim_start_matches(blank)
            .strip_prefix("today")
            .map(|tail| tail.trim_start_matches(blank))
            .and_then(|tail| tail.strip_prefix("}}"));
        if let Some(rest) = rest {
            return Some(start..text.len() - rest.len());
        }
        from = start + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;

    #[test]
    fn expand_replaces_each_placeholder() {
        let date = "19 November 2026";
        assert_eq!(
            expand("Dated {{today}}; again {{ today }}.", date),
            "Dated 19 November 2026; again 19 November 2026."
        );
        assert_eq!(expand("{{\ttoday  }}", date), date);
        assert_eq!(expand("{{{today}}}", date), "{19 November 2026}");
    }

    #[test]
    fn other_braces_stay_literal() {
        for text in [
            "{today}", "{{todays}}", "{{to day}}", "{{Today}}", "{{today}",
            "{{ today } }", "{{date}}", "no braces", "{{",
        ] {
            assert!(!contains(text), "{text}");
            assert_eq!(expand(text, "X"), text);
        }
        assert!(contains("a {{ {{today}} b"));
        assert_eq!(expand("a {{ {{today}} b", "X"), "a {{ X b");
    }

    #[test]
    fn date_follows_the_pinned_timestamp_then_the_epoch_then_now() {
        let now = UNIX_EPOCH + Duration::from_secs(1_795_000_000);
        let mut config = Config::default();
        assert_eq!(date_at(&config, None, now).unwrap(), "18 November 2026");
        assert_eq!(date_at(&config, Some("0"), now).unwrap(), "1 January 1970");
        config.pdf.timestamp = "2026-05-09T23:30:00-01:00".into();
        assert_eq!(date_at(&config, Some("0"), now).unwrap(), "10 May 2026");
    }

    #[test]
    fn invalid_timestamps_are_configuration_errors() {
        let mut config = Config::default();
        config.pdf.timestamp = "yesterday".into();
        let error = date_at(&config, None, UNIX_EPOCH).unwrap_err();
        assert!(error.to_string().contains("invalid [pdf] timestamp"));
        config.pdf.timestamp.clear();
        assert!(date_at(&config, Some("soon"), UNIX_EPOCH).is_err());
    }
}
