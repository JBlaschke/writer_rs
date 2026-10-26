//! Shared comrak parser configuration (markdown-dialect.md §1).
//! Owner: T11. Stub created by T00.

use crate::config::MarkdownConfig;

/// The shared Markdown dialect, with user-controlled punctuation and scripts.
pub fn comrak_options(md: &MarkdownConfig) -> comrak::Options<'static> {
    let mut o = comrak::Options::default();
    // presenter's set (R8 §4.1), unchanged
    o.extension.front_matter_delimiter = Some("---".into());
    o.extension.table = true;
    o.extension.strikethrough = true;
    o.extension.tasklist = true;
    o.extension.autolink = true;
    o.extension.footnotes = true;
    o.extension.description_lists = true;
    o.extension.alerts = true;
    o.extension.math_dollars = true;
    o.extension.math_code = true;
    o.extension.block_directive = true;
    o.extension.header_attributes = true;
    o.extension.fenced_code_attributes = true;
    o.extension.link_attributes = true;
    o.extension.inline_code_attributes = true;
    // writer additions
    o.extension.inline_footnotes = true;
    o.extension.superscript = md.superscript;
    o.extension.subscript = md.subscript;
    // Keep definitions in place, including unreferenced ones for linting.
    o.parse.leave_footnote_definitions = true;
    // Preserve escapes so reference scanning can skip literal @ signs (§7.2).
    o.parse.escaped_char_spans = true;
    // parse
    o.parse.smart = md.smart_punctuation;
    o.parse.relaxed_autolinks = false;
    o.parse.sourcepos_chars = false; // byte columns; writer uses lines (§4.9)
    // Keep comrak's formatter usable as a debugging dump.
    o.render.r#unsafe = true;
    o.render.escape = false;
    o.render.hardbreaks = false;
    o.render.github_pre_lang = false;
    o.render.full_info_string = false;
    o.render.sourcepos = false; // the AST carries sourcepos regardless (R9 §12)
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comrak_options_follow_the_dialect_exactly() {
        let o = comrak_options(&MarkdownConfig::default());
        let e = &o.extension;
        assert_eq!(e.front_matter_delimiter.as_deref(), Some("---"));
        assert!(
            e.table
                && e.strikethrough
                && e.tasklist
                && e.autolink
                && e.footnotes
        );
        assert!(
            e.description_lists && e.alerts && e.math_dollars && e.math_code
        );
        assert!(
            e.block_directive
                && e.header_attributes
                && e.fenced_code_attributes
        );
        assert!(
            e.link_attributes && e.inline_code_attributes && e.inline_footnotes
        );
        assert!(!e.superscript && !e.subscript);
        assert!(!o.parse.smart && !o.parse.relaxed_autolinks);
        assert!(
            o.parse.leave_footnote_definitions && o.parse.escaped_char_spans
        );
        assert!(!o.parse.sourcepos_chars);
        assert!(o.render.r#unsafe && !o.render.escape && !o.render.hardbreaks);
        assert!(
            !o.render.github_pre_lang
                && !o.render.full_info_string
                && !o.render.sourcepos
        );
        let config = MarkdownConfig {
            smart_punctuation: true,
            sourcepos: true,
            superscript: true,
            subscript: true,
            ..MarkdownConfig::default()
        };
        let o = comrak_options(&config);
        assert!(
            o.parse.smart && o.extension.superscript && o.extension.subscript
        );
        assert!(!o.render.sourcepos);
    }

    #[test]
    fn extensions_writer_never_enables() {
        let o = comrak_options(&MarkdownConfig::default());
        let e = &o.extension;
        assert!(!e.underline && !e.math_latex && !e.multiline_block_quotes);
        assert!(!e.highlight && !e.insert && !e.spoiler && !e.greentext);
        assert!(
            !e.subtext
                && !e.wikilinks_title_after_pipe
                && !e.wikilinks_title_before_pipe
        );
        assert!(!e.ofm_comments && !e.cjk_friendly_emphasis);
        assert!(!o.parse.ignore_setext);
        assert!(
            e.image_url_rewriter.is_none() && e.link_url_rewriter.is_none()
        );
        assert!(e.header_id_prefix.is_none());
    }
}
