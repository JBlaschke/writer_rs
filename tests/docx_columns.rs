//! Multi-column Word sections from `[design.page] columns` (design.md
//! §7.5): full-width title block, lead and figures, furniture on every
//! section, column-width images, and the default single section.

use std::io::{Cursor, Read};
use std::path::Path;

use writer::docx::{DocxOptions, Picture, Renderer, ValidateOptions};
use writer::error::{Error, Result};
use writer::ir::MathDialect;
use writer::pipeline::{self, BuildOptions};

#[path = "common/docx.rs"]
mod common_docx;

const PNG: &[u8] = include_bytes!("fixtures/docx/images/dpi144.png");
/// A4 with Word's one-inch margins.
const TEXT_WIDTH: u64 = 11_906 - 2 * 1_440;
/// Two columns 7 mm (397 twips) apart.
const COLUMN_WIDTH: u64 = (TEXT_WIDTH - 397) / 2;
const EMU_PER_TWIP: u64 = 635;

struct NoRenderer;

impl Renderer for NoRenderer {
    fn equation(
        &mut self, _: &str, _: MathDialect, _: bool,
    ) -> Result<Picture> {
        Err(Error::Other("no equations in this fixture".into()))
    }
    fn image(&mut self, _: &[u8], _: &str, _: Option<f64>) -> Result<Picture> {
        Err(Error::Other("PNG images need no renderer".into()))
    }
    fn typst_mathml(&mut self, _: &str, _: bool) -> Result<String> {
        Err(Error::Other("no equations in this fixture".into()))
    }
}

const FURNITURE: &str = concat!(
    "[design.page.header.first]\nlines = ['First page']\n",
    "[design.page.header.rest]\nlines = ['Later pages']\n",
    "[design.page.footer]\nlines = ['Footer']\n",
);

const BODY: &str = concat!(
    "---\ntitle: Columns\n---\n\n",
    "Lead paragraph above the rule.\n\n---\n\n",
    "First body paragraph with a note.[^n]\n\n",
    "Second body paragraph ![](pic.png){width=100%}\n\n",
    "![A wide figure](pic.png){#fig-wide width=100%}\n\n",
    "Text after the figure.\n\n",
    "Table: A captioned table {#tbl-a}\n\n",
    "| a | b |\n|---|---|\n| 1 | 2 |\n\n",
    "Closing paragraph.\n\n",
    "[^n]: The note.\n",
);

/// Builds `body` with `design` (TOML under `[design.page]`) and `extra`
/// configuration, validating the package.
fn build(page: &str, extra: &str, body: &str) -> Vec<u8> {
    build_with(page, extra, body, None)
}

fn build_with(
    page: &str, extra: &str, body: &str, reference: Option<&Path>,
) -> Vec<u8> {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("writer.toml"),
        format!("[design.page]\n{page}\n{extra}"),
    )
    .unwrap();
    std::fs::write(dir.path().join("pic.png"), PNG).unwrap();
    let input = dir.path().join("doc.md");
    std::fs::write(&input, body).unwrap();
    let prepared = pipeline::prepare(&BuildOptions::new(input)).unwrap();
    let output = writer::docx::write(
        &prepared.document,
        &prepared.assets,
        &mut NoRenderer,
        &DocxOptions {
            design: &prepared.design,
            config: &prepared.config,
            reference,
            source_date_epoch: Some("0".into()),
            all_styles: false,
        },
    )
    .unwrap();
    let violations = writer::docx::validate(
        &output.bytes,
        &ValidateOptions { builtin_styles: true },
    );
    assert!(violations.is_empty(), "{violations:#?}");
    common_docx::read_back(&output.bytes);
    output.bytes
}

fn document(bytes: &[u8]) -> String {
    part(bytes, "word/document.xml")
}

fn part(bytes: &[u8], name: &str) -> String {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    archive.by_name(name).unwrap().read_to_string(&mut xml).unwrap();
    xml
}

/// Every `w:sectPr` in document order.
fn sections(xml: &str) -> Vec<&str> {
    xml.match_indices("<w:sectPr>")
        .map(|(start, _)| {
            let end = xml[start..].find("</w:sectPr>").unwrap();
            &xml[start..start + end + "</w:sectPr>".len()]
        })
        .collect()
}

/// Column count (1 when `w:num` is absent) and whether the section is
/// continuous.
fn layout(section: &str) -> (u32, bool) {
    let cols = section.split_once("<w:cols ").unwrap().1;
    let count = cols
        .split_once("w:num=\"")
        .filter(|(before, _)| !before.contains("/>"))
        .map_or(1, |(_, rest)| {
            rest.split('"').next().unwrap().parse().unwrap()
        });
    (count, section.contains("<w:type w:val=\"continuous\"/>"))
}

/// The header and footer references of a section.
fn furniture(section: &str) -> Vec<&str> {
    section
        .match_indices("Reference ")
        .map(|(start, _)| {
            let open = section[..start].rfind('<').unwrap();
            let end = section[start..].find("/>").unwrap();
            &section[open..start + end + 2]
        })
        .collect()
}

fn extents(xml: &str) -> Vec<u64> {
    xml.match_indices("<wp:extent cx=\"")
        .map(|(start, marker)| {
            let rest = &xml[start + marker.len()..];
            rest.split('"').next().unwrap().parse().unwrap()
        })
        .collect()
}

#[test]
fn body_columns_with_full_width_title_and_figures() {
    let page = "columns = { count = 2, gap = '7mm' }";
    let xml = document(&build(page, FURNITURE, BODY));
    let sections = sections(&xml);
    let layouts = sections.iter().map(|s| layout(s)).collect::<Vec<_>>();
    // Title block; body; figure; body; captioned table; body to the end.
    assert_eq!(
        layouts,
        [(1, false), (2, true), (1, true), (2, true), (1, true), (2, true)]
    );
    assert!(sections.iter().all(|s| s.contains("w:space=\"397\"")));
    // The title block's section ends in the title paragraph itself.
    assert!(xml.contains("<w:pStyle w:val=\"Title\"/><w:sectPr>"));
    // The figure's section ends in its caption, after the image.
    let caption = xml.find("<w:pStyle w:val=\"ImageCaption\"/>").unwrap();
    assert!(xml[caption..].starts_with(&format!(
        "<w:pStyle w:val=\"ImageCaption\"/>{}",
        sections[2]
    )));
    // A table cannot hold the break: a one-point paragraph follows it.
    assert!(xml.contains(&format!(
        "</w:tbl><w:p><w:pPr><w:spacing w:before=\"0\" w:after=\"0\" w:line=\"20\" w:lineRule=\"exact\"/><w:rPr><w:sz w:val=\"2\"/><w:szCs w:val=\"2\"/></w:rPr>{}</w:pPr></w:p>",
        sections[4]
    )));
    // The final section is the body-level one.
    assert!(xml.ends_with(&format!("{}</w:body></w:document>", sections[5])));
}

#[test]
fn every_section_repeats_the_furniture_and_footnotes_run_on() {
    let page = "columns = { count = 2, gap = '7mm' }";
    let xml = document(&build(page, FURNITURE, BODY));
    let sections = sections(&xml);
    let first = furniture(sections[0]);
    assert_eq!(first.len(), 4, "{first:?}");
    assert!(
        first.iter().any(|r| r.contains("headerReference w:type=\"first\""))
    );
    assert!(
        first.iter().any(|r| r.contains("headerReference w:type=\"default\""))
    );
    assert!(
        first.iter().any(|r| r.contains("footerReference w:type=\"default\""))
    );
    for section in &sections {
        assert_eq!(furniture(section), first);
        assert!(section.contains("<w:numRestart w:val=\"continuous\"/>"));
    }
    // Page one is the first section's; a later section starting at the top
    // of a page must not show the first-page furniture.
    assert!(sections[0].contains("<w:titlePg/>"));
    assert!(sections[1..].iter().all(|s| !s.contains("<w:titlePg/>")));
}

#[test]
fn images_take_the_column_width_and_figures_the_page_width() {
    let page = "columns = { count = 2, gap = '7mm' }";
    let xml = document(&build(page, FURNITURE, BODY));
    assert_eq!(
        extents(&xml),
        [COLUMN_WIDTH * EMU_PER_TWIP, TEXT_WIDTH * EMU_PER_TWIP]
    );
}

#[test]
fn after_rule_keeps_the_lead_full_width_and_breaks_in_the_rule() {
    let page = "columns = { count = 2, gap = '7mm', start = 'after-rule' }";
    let xml = document(&build(page, "", BODY));
    let sections = sections(&xml);
    assert_eq!(layout(sections[0]), (1, false));
    assert_eq!(layout(sections[1]), (2, true));
    let lead = xml.find("Lead paragraph above the rule.").unwrap();
    let rule = xml.find("<w:pStyle w:val=\"HorizontalRule\"/>").unwrap();
    let first_break = xml.find("<w:sectPr>").unwrap();
    assert!(lead < rule && rule < first_break);
    assert!(xml[rule..].starts_with(&format!(
        "<w:pStyle w:val=\"HorizontalRule\"/>{}",
        sections[0]
    )));
    // Without furniture the built-in page-number footer is repeated.
    for section in &sections {
        assert_eq!(furniture(section), furniture(sections[0]));
        assert_eq!(furniture(section).len(), 1);
    }
}

#[test]
fn blocks_that_write_nothing_leave_no_empty_section() {
    let page = "columns = 2";
    let body = concat!(
        "---\ntitle: T\n---\n\n",
        "Para one.\n\n",
        "![First](pic.png){#fig-a}\n\n",
        "```{=html}\n<div>x</div>\n```\n\n",
        "![Second](pic.png){#fig-b}\n\n",
        "```{=latex}\n\\clearpage\n```\n",
    );
    let xml = document(&build(page, "", body));
    let sections = sections(&xml);
    // Title; body; both figures in one section, which ends the document.
    let layouts = sections.iter().map(|s| layout(s)).collect::<Vec<_>>();
    assert_eq!(layouts, [(1, false), (2, true), (1, true)]);
    // Only the body-level section follows the last caption.
    let caption = xml.rfind("<w:pStyle w:val=\"ImageCaption\"/>").unwrap();
    assert_eq!(xml[caption..].matches("<w:sectPr>").count(), 1);
    assert!(xml.ends_with(&format!("{}</w:body></w:document>", sections[2])));
}

#[test]
fn bookmarks_alone_do_not_make_a_section() {
    let page = "columns = 2";
    let body = concat!(
        "---\ntitle: T\n---\n\n",
        "See [here](#anchor) and [there](#end).\n\n",
        "![First](pic.png){#fig-a}\n\n",
        "::: {.x #anchor}\n:::\n\n",
        "![Second](pic.png){#fig-b}\n\n",
        "::: {.x #end}\n:::\n",
    );
    let xml = document(&build(page, "", body));
    // The empty divs write only their bookmarks.
    assert!(
        xml.contains("w:name=\"_anchor\"") && xml.contains("w:name=\"_end\"")
    );
    let sections = sections(&xml);
    let layouts = sections.iter().map(|s| layout(s)).collect::<Vec<_>>();
    assert_eq!(layouts, [(1, false), (2, true), (1, true)]);
    // Every break is in a paragraph of the text; none needs its own.
    assert!(!xml.contains("w:line=\"20\" w:lineRule=\"exact\""));
    let caption = xml.rfind("<w:pStyle w:val=\"ImageCaption\"/>").unwrap();
    assert_eq!(xml[caption..].matches("<w:sectPr>").count(), 1);
}

#[test]
fn a_trailing_table_keeps_its_closing_paragraph() {
    let page = "columns = 2";
    let body = concat!(
        "---\ntitle: T\n---\n\n",
        "Para one.\n\n",
        "Table: Cap {#tbl-a}\n\n",
        "| a | b |\n|---|---|\n| 1 | 2 |\n\n",
        "```{=html}\n<div>x</div>\n```\n",
    );
    let xml = document(&build(page, "", body));
    let sections = sections(&xml);
    let layouts = sections.iter().map(|s| layout(s)).collect::<Vec<_>>();
    assert_eq!(layouts, [(1, false), (2, true), (1, true)]);
    assert!(xml.ends_with(&format!(
        "</w:tbl><w:p/>{}</w:body></w:document>",
        sections[2]
    )));
}

#[test]
fn footnotes_take_the_width_of_the_section_that_references_them() {
    let page = "columns = { count = 2, gap = '7mm', start = 'after-rule' }";
    let body = concat!(
        "---\ntitle: T\n---\n\n",
        "Lead with a note.[^lead]\n\n---\n\n",
        "Body with a note.[^body]\n\n",
        "![A wide figure](pic.png){#fig-wide width=100%}\n\n",
        "[^lead]: Lead note ![](pic.png){width=100%}\n",
        "[^body]: Body note ![](pic.png){width=100%}\n",
    );
    let bytes = build(page, "", body);
    // The body ends in the figure's full-width section, but the second
    // note is referenced from a column.
    assert_eq!(
        extents(&part(&bytes, "word/footnotes.xml")),
        [TEXT_WIDTH * EMU_PER_TWIP, COLUMN_WIDTH * EMU_PER_TWIP]
    );
}

#[test]
fn after_rule_without_a_rule_starts_at_the_body() {
    let page = "columns = { count = 3, start = 'after-rule' }";
    let body = "---\ntitle: T\n---\n\nOne.\n\nTwo.\n";
    let xml = document(&build(page, "", body));
    let sections = sections(&xml);
    assert_eq!(sections.len(), 2);
    assert_eq!(layout(sections[0]), (1, false));
    assert_eq!(layout(sections[1]), (3, true));
    // An unset gap is Word's default half inch.
    assert!(sections[1].contains("<w:cols w:num=\"3\" w:space=\"720\"/>"));
}

#[test]
fn a_body_without_a_title_block_starts_in_columns() {
    let xml = document(&build("columns = 2", "", "One.\n\nTwo.\n"));
    let sections = sections(&xml);
    assert_eq!(sections.len(), 1);
    assert_eq!(layout(sections[0]), (2, false));
}

#[test]
fn one_column_and_unset_columns_write_the_same_package() {
    let unset = build("margin = '1in'", FURNITURE, BODY);
    let one = build("margin = '1in'\ncolumns = 1", FURNITURE, BODY);
    assert_eq!(unset, one);
    let xml = document(&unset);
    let sections = sections(&xml);
    assert_eq!(sections.len(), 1);
    assert!(sections[0].contains("<w:numRestart w:val=\"eachSect\"/>"));
    assert!(sections[0].contains("<w:cols w:space=\"720\"/>"));
    assert!(!sections[0].contains("<w:type"));
}

#[test]
fn a_reference_section_wins_over_columns() {
    let dir = tempfile::tempdir().unwrap();
    let reference = dir.path().join("reference.docx");
    std::fs::write(&reference, build("margin = '1in'", "", BODY)).unwrap();
    let page = "columns = 2";
    let xml = document(&build_with(page, "", BODY, Some(&reference)));
    let sections = sections(&xml);
    assert_eq!(sections.len(), 1);
    assert_eq!(layout(sections[0]), (1, false));
    assert_eq!(extents(&xml), [TEXT_WIDTH * EMU_PER_TWIP; 2]);
}
