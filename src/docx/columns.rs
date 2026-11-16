//! Multi-column body sections (`[design.page] columns`, design.md §7.5).
//!
//! The title block, an optional full-width lead and every top-level figure
//! are one-column sections; the rest of the body is set in columns. Each
//! section after the first starts with a continuous break, so sections share
//! pages, and each repeats the furniture references; only the first has
//! `w:titlePg`.

use crate::design::{ColumnStart, Design};
use crate::ir::Block;

use super::Emitter;
use super::design::{SectionColumns, section_with, twips};

/// Word's default space between columns, half an inch.
const DEFAULT_GAP: u32 = 720;

/// Holds a section break: no space before or after, a one-point line and
/// paragraph mark, as furniture rule paragraphs are built (design.md §9.5).
const BREAK_PARAGRAPH: &str = concat!(
    "<w:spacing w:before=\"0\" w:after=\"0\" w:line=\"20\" ",
    "w:lineRule=\"exact\"/><w:rPr><w:sz w:val=\"2\"/>",
    "<w:szCs w:val=\"2\"/></w:rPr>",
);

/// The generated `w:sectPr` variants of a multi-column document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnSections {
    /// One-column and multi-column properties of the first section.
    first: [String; 2],
    /// One-column and multi-column properties of a continuous section.
    later: [String; 2],
    /// Width of one column, in twips.
    pub column_width_twips: u32,
    /// The first body block set in columns.
    pub start: ColumnStart,
}

impl ColumnSections {
    /// The sections, when the design asks for more than one column.
    pub(crate) fn new(
        design: &Design, paper: &str, header_refs: &[(&str, String)],
        footer_refs: &[(&str, String)], text_width: u32,
    ) -> Option<Self> {
        let columns =
            design.page.columns.filter(|columns| columns.count > 1)?;
        let space = columns.gap.map_or(DEFAULT_GAP, twips);
        let count = u32::from(columns.count);
        let gaps = space.saturating_mul(count - 1);
        let xml = |count, continuous| {
            section_with(
                design,
                paper,
                header_refs,
                footer_refs,
                Some(SectionColumns { count, space, continuous }),
            )
        };
        Some(Self {
            first: [xml(1, false), xml(columns.count, false)],
            later: [xml(1, true), xml(columns.count, true)],
            column_width_twips: (text_width.saturating_sub(gaps) / count)
                .max(1),
            start: columns.start,
        })
    }

    fn sect_pr(&self, multi: bool, first: bool) -> &str {
        let variants = if first { &self.first } else { &self.later };
        &variants[usize::from(multi)]
    }
}

/// The open section and the last body paragraph, for section breaks.
#[derive(Debug, Default)]
pub(crate) struct SectionState {
    /// The open section is set in columns.
    multi: bool,
    /// A section break has been written.
    broken: bool,
    /// Body length when the open section began.
    start: usize,
    /// Body offset of the paragraph being written.
    paragraph_start: usize,
    /// Body offsets of the last paragraph `start_paragraph` wrote.
    paragraph: Option<(usize, usize)>,
    /// The break that began the open section, while it can be taken back.
    last_break: Option<Break>,
    /// Whether the section that references each footnote (by plan index)
    /// is set in columns.
    notes: Vec<Option<bool>>,
}

/// A written section break and the state before it.
#[derive(Debug, Clone, Copy)]
struct Break {
    /// Body offset and length of the inserted XML.
    at: usize,
    len: usize,
    /// The section the break ended.
    multi: bool,
    broken: bool,
    start: usize,
}

impl Emitter<'_> {
    /// Width of the open section's text column, in twips.
    pub(crate) fn column_width_twips(&self) -> u32 {
        match &self.layout.columns {
            Some(sections) if self.section.multi => sections.column_width_twips,
            _ => self.layout.text_width_twips,
        }
    }

    /// Index of the first top-level body block set in columns. Without a
    /// rule, `after-rule` starts the columns at the body.
    pub(crate) fn column_start(&self) -> Option<usize> {
        let sections = self.layout.columns.as_ref()?;
        Some(match sections.start {
            ColumnStart::Body => 0,
            ColumnStart::AfterRule => self
                .doc
                .blocks
                .iter()
                .position(|block| matches!(block, Block::ThematicBreak { .. }))
                .map_or(0, |index| index + 1),
        })
    }

    /// Opens the section a top-level body block belongs to: a figure spans
    /// the columns, everything else is set in them.
    pub(crate) fn column_section_for(&mut self, block: &Block) {
        self.begin_section(!matches!(block, Block::Figure { .. }));
    }

    /// Writes the final section's properties. A break followed by nothing
    /// (blocks that write nothing to Word, such as raw HTML) is taken back,
    /// so the document does not end in an empty section.
    pub(crate) fn finish_sections(&mut self) {
        if !self.section_has_content() {
            self.undo_break();
        }
        if let Some(sections) = &self.layout.columns {
            let sect_pr =
                sections.sect_pr(self.section.multi, !self.section.broken);
            self.layout.sect_pr = sect_pr.to_owned();
        }
    }

    /// Records the section a footnote is referenced from, which sets the
    /// width its pictures and tables get (Word sets a footnote in the
    /// column its reference is in).
    pub(crate) fn mark_note_reference(&mut self, index: usize) {
        if self.layout.columns.is_none() || self.part != super::Part::Document {
            return;
        }
        let notes = &mut self.section.notes;
        if notes.len() <= index {
            notes.resize(index + 1, None);
        }
        notes[index].get_or_insert(self.section.multi);
    }

    /// Lays out the footnote with plan index `index` at the width of the
    /// section it is referenced from; returns the state to restore.
    pub(crate) fn enter_note(&mut self, index: usize) -> bool {
        let multi = self.section.notes.get(index).copied().flatten();
        std::mem::replace(&mut self.section.multi, multi.unwrap_or(false))
    }

    pub(crate) fn leave_note(&mut self, multi: bool) {
        self.section.multi = multi;
    }

    pub(crate) fn mark_paragraph_start(&mut self) {
        if self.part == super::Part::Document {
            self.section.paragraph_start = self.body.len();
        }
    }

    pub(crate) fn mark_paragraph_end(&mut self) {
        if self.part == super::Part::Document {
            let start = self.section.paragraph_start;
            self.section.paragraph = Some((start, self.body.len()));
        }
    }

    /// An empty open section changes its columns without a break, or
    /// takes back the break that began it when the section before it had
    /// the columns asked for.
    fn begin_section(&mut self, multi: bool) {
        if self.layout.columns.is_none() || self.section.multi == multi {
            return;
        }
        if self.section_has_content() {
            self.break_section();
        } else if self.section.last_break.is_some_and(|b| b.multi == multi) {
            self.undo_break();
            return;
        }
        self.section.multi = multi;
    }

    /// Whether the open section holds a paragraph, table or content
    /// control. Bookmarks, comments and whitespace (an empty div's anchor,
    /// raw XML without blocks) do not make a section.
    fn section_has_content(&self) -> bool {
        let open = &self.body[self.section.start..];
        ["<w:p>", "<w:p ", "<w:p/>", "<w:tbl>", "<w:tbl ", "<w:sdt>", "<w:sdt "]
            .iter()
            .any(|tag| open.contains(tag))
    }

    /// Removes the break that began the empty open section and reopens the
    /// section it ended. Anything written since (bookmarks, whitespace) is
    /// after the break and stays.
    fn undo_break(&mut self) {
        let Some(last) = self.section.last_break.take() else {
            return;
        };
        debug_assert!(last.at + last.len <= self.section.start);
        self.body.replace_range(last.at..last.at + last.len, "");
        self.section.multi = last.multi;
        self.section.broken = last.broken;
        self.section.start = last.start;
    }

    /// Ends the open section in its last paragraph when the body ends with
    /// one, else in a paragraph of its own.
    fn break_section(&mut self) {
        let Some(sections) = &self.layout.columns else {
            return;
        };
        let sect_pr = sections
            .sect_pr(self.section.multi, !self.section.broken)
            .to_owned();
        let before = self.body.len();
        // `start_paragraph` writes `<w:p>`, then any `w:pPr`; a section's
        // properties are the last child of `w:pPr`.
        let last = self.section.paragraph.filter(|&(start, end)| {
            end == self.body.len() && start >= self.section.start
        });
        let open = last.map(|(start, _)| start + "<w:p>".len());
        let props =
            open.filter(|&open| self.body[open..].starts_with("<w:pPr>"));
        let at = match (open, props) {
            (Some(open), Some(_)) => {
                let close = self.body[open..]
                    .find("</w:pPr>")
                    .expect("start_paragraph closes its properties");
                self.body.insert_str(open + close, &sect_pr);
                open + close
            }
            (Some(open), None) => {
                let props = format!("<w:pPr>{sect_pr}</w:pPr>");
                self.body.insert_str(open, &props);
                open
            }
            (None, _) => {
                self.body.push_str(&format!(
                    "<w:p><w:pPr>{BREAK_PARAGRAPH}{sect_pr}</w:pPr></w:p>"
                ));
                before
            }
        };
        self.section.last_break = Some(Break {
            at,
            len: self.body.len() - before,
            multi: self.section.multi,
            broken: self.section.broken,
            start: self.section.start,
        });
        self.section.broken = true;
        self.section.start = self.body.len();
    }
}
