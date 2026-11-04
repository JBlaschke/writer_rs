//! Design header/footer parts, the page background picture, and shared
//! first-page section references.

use std::path::Path;

use crate::design::{
    Align, Background, BackgroundPages, Color, Design, Fit, Furniture, Line,
    Pt, Rule, RulePosition, Segment, Slot, VerticalAlign, segments,
};
use crate::error::{Error, Result};

use super::images::{InlineDrawing, normal_scale, picture_graphic};
use super::package::Relationship;
use super::reference::Reference;
use super::validate::parse_xml;
use super::xml::{self, PPR, Props, RPR, escape};
use super::{Emitter, Ids, Renderer, StyleKind, StyleTable};

/// One generated furniture XML part, its local relationships, and media.
pub(crate) struct FurniturePart {
    pub(crate) name: String,
    pub(crate) xml: String,
    pub(crate) rels: Vec<Relationship>,
    pub(crate) media: Vec<(String, Vec<u8>)>,
}

/// Shared font, style, drawing-ID and geometry state for one part.
pub(crate) struct PartContext<'a> {
    design: &'a Design,
    styles: &'a mut StyleTable,
    ids: &'a mut Ids,
    renderer: &'a mut dyn Renderer,
    text_width: u32,
    body_size: Pt,
    /// The document defaults' space after a paragraph, in twips.
    default_after: u32,
    page_size: (u32, u32),
}

/// Generates all declared furniture and the page background picture when the
/// reference has no body section.
pub(crate) fn generate(
    emitter: &mut Emitter<'_>, reference: Option<&Reference>,
) -> Result<()> {
    let design = emitter.design;
    if !design.declared
        || reference.is_some_and(|reference| reference.sect_pr.is_some())
        || (design.page.header.is_none()
            && design.page.footer.is_none()
            && design.page.background.is_none())
    {
        return Ok(());
    }
    let first_page = super::design::first_page(design);
    let styles_xml = emitter.styles.to_xml();
    let body_size = Pt(11.0 * normal_scale(&styles_xml));
    let mut context = PartContext {
        design,
        styles: &mut emitter.styles,
        ids: &mut emitter.ids,
        renderer: emitter.renderer,
        text_width: emitter.layout.text_width_twips,
        body_size,
        default_after: default_after(&styles_xml),
        page_size: super::design::page_size(design, &emitter.config.pdf.paper),
    };
    let background = design.page.background.as_ref();
    let first_only = background
        .is_some_and(|background| background.pages == BackgroundPages::First);
    let mut headers = Vec::new();
    let mut footers = Vec::new();
    for (header, slot, references) in [
        (true, design.page.header.as_ref(), &mut headers),
        (false, design.page.footer.as_ref(), &mut footers),
    ] {
        let plan = match slot {
            Some(slot) => plan_slot(slot, first_page, header && first_only),
            // The background picture needs a header part of its own when no
            // header slot is declared.
            None if header && background.is_some() => {
                let pages = if first_only {
                    vec!["first"]
                } else if first_page {
                    vec!["first", "default"]
                } else {
                    vec!["default"]
                };
                vec![(pages, None)]
            }
            None => continue,
        };
        for (pages, furniture) in plan {
            let (number, id) = emitter.package.reserve_furniture(header);
            let part = if header {
                let picture = background
                    .filter(|_| !first_only || pages.contains(&"first"));
                header_part(furniture, picture, number, &mut context)?
            } else {
                footer_part(furniture, number, &mut context)?
            };
            emitter.package.install_furniture(part);
            for page in pages {
                references.push((page, id.clone()));
            }
        }
    }
    // titlePg affects both slots. Alias the built-in footer on page one when
    // only the header differs, so it does not disappear on the first page.
    if design.page.footer.is_none()
        && let Some((_, id)) = emitter.package.footer()
    {
        if first_page {
            footers.push(("first", id.into()));
        }
        footers.push(("default", id.into()));
    }
    // The reference settings still apply without a reference section. Give
    // even pages the rest furniture when Word enables separate even headers.
    if even_and_odd_headers(reference) {
        for references in [&mut headers, &mut footers] {
            let rest = references
                .iter()
                .find(|(page, _)| *page == "default")
                .map(|(_, id)| id.clone());
            if let Some(id) = rest {
                references.push(("even", id));
            }
        }
    }
    emitter.layout.sect_pr = super::design::section(
        design, &emitter.config.pdf.paper, &headers, &footers,
    );
    Ok(())
}

/// One part per distinct furniture; `split_first` separates an identical
/// first page anyway, so only its part can carry the background picture.
fn plan_slot(
    slot: &Slot, first_page: bool, split_first: bool,
) -> Vec<(Vec<&'static str>, Option<&Furniture>)> {
    if slot.first == slot.rest && !split_first {
        let pages = if first_page {
            vec!["first", "default"]
        } else {
            vec!["default"]
        };
        vec![(pages, slot.rest.as_ref())]
    } else {
        vec![
            (vec!["first"], slot.first.as_ref()),
            (vec!["default"], slot.rest.as_ref()),
        ]
    }
}

pub(crate) fn header_part(
    furniture: Option<&Furniture>, background: Option<&Background>,
    number: usize, context: &mut PartContext<'_>,
) -> Result<FurniturePart> {
    furniture_part(furniture, background, true, number, context)
}

pub(crate) fn footer_part(
    furniture: Option<&Furniture>, number: usize, context: &mut PartContext<'_>,
) -> Result<FurniturePart> {
    furniture_part(furniture, None, false, number, context)
}

fn furniture_part(
    furniture: Option<&Furniture>, background: Option<&Background>,
    header: bool, number: usize, context: &mut PartContext<'_>,
) -> Result<FurniturePart> {
    let name = if header { "header" } else { "footer" };
    let tag = if header { "hdr" } else { "ftr" };
    let mut part = FurniturePart {
        name: format!("word/{name}{number}.xml"),
        xml: String::new(),
        rels: Vec::new(),
        media: Vec::new(),
    };
    // The furniture comes first so its relationships and drawing ids do not
    // depend on whether a background is declared.
    let body = furniture
        .map(|furniture| {
            furniture_paragraphs(furniture, header, context, &mut part)
        })
        .transpose()?
        .unwrap_or_else(|| "<w:p/>".into());
    let picture = background
        .map(|background| background_run(background, context, &mut part))
        .transpose()?;
    let body = match picture {
        Some(run) => with_anchor(&body, &run),
        None => body,
    };
    part.xml = format!(
        "{}<w:{tag} xmlns:w=\"{}\" xmlns:r=\"{}\" xmlns:a=\"{}\" xmlns:pic=\"{}\" xmlns:wp=\"{}\">{body}</w:{tag}>",
        xml::XML_HEAD,
        xml::NS_W,
        xml::NS_R,
        xml::NS_A,
        xml::NS_PIC,
        xml::NS_WP,
    );
    Ok(part)
}

fn furniture_paragraphs(
    furniture: &Furniture, header: bool, context: &mut PartContext<'_>,
    part: &mut FurniturePart,
) -> Result<String> {
    let mut xml = String::new();
    let name = if header {
        super::styles::HEADER
    } else {
        "Footer"
    };
    let style = context.styles.ensure(name, StyleKind::Paragraph);
    if let Some(rule) =
        furniture.rule.filter(|rule| rule.position == RulePosition::Above)
    {
        xml.push_str(&rule_paragraph(rule, &style));
    }
    if furniture.image.is_some() {
        xml.push_str(&image_paragraph(furniture, &style, context, part)?);
    }
    let size = furniture
        .size
        .unwrap_or(Pt(context.body_size.0 * if header { 1.0 } else { 0.75 }));
    for index in 0..furniture.lines.len().max(furniture.right_lines.len()) {
        let mut props = Props::new("w:pPr", PPR);
        props
            .set("w:pStyle", property("w:pStyle", &[("w:val", style.clone())]));
        props.set(
            "w:jc",
            property("w:jc", &[("w:val", alignment(furniture.align).into())]),
        );
        let mut runs = String::new();
        if let Some(line) = furniture.lines.get(index) {
            runs.push_str(&line_runs(line, furniture, size, context.design));
        }
        if !furniture.right_lines.is_empty() {
            props.set(
                "w:tabs",
                format!(
                    "<w:tabs><w:tab w:val=\"right\" w:pos=\"{}\"/></w:tabs>",
                    context.text_width,
                ),
            );
            if let Some(line) = furniture.right_lines.get(index) {
                runs.push_str("<w:r><w:tab/></w:r>");
                runs.push_str(&line_runs(
                    line, furniture, size, context.design,
                ));
            }
        }
        xml.push_str(&format!("<w:p>{}{runs}</w:p>", props.to_xml()));
    }
    if let Some(rule) =
        furniture.rule.filter(|rule| rule.position == RulePosition::Below)
    {
        xml.push_str(&rule_paragraph(rule, &style));
    }
    Ok(if xml.is_empty() { "<w:p/>".into() } else { xml })
}

fn line_runs(
    line: &Line, furniture: &Furniture, size: Pt, design: &Design,
) -> String {
    let mut props = Props::new("w:rPr", RPR);
    if let Some(font) = &design.fonts.body {
        props.set(
            "w:rFonts",
            property(
                "w:rFonts",
                &[
                    ("w:ascii", font.family.clone()),
                    ("w:hAnsi", font.family.clone()),
                    ("w:cs", font.family.clone()),
                ],
            ),
        );
    }
    let bold = line.weight.unwrap_or(furniture.weight).0 >= 600;
    let italic = line.italic.unwrap_or(furniture.italic);
    for (tag, enabled) in
        [("w:b", bold), ("w:bCs", bold), ("w:i", italic), ("w:iCs", italic)]
    {
        props.set(
            tag,
            if enabled {
                format!("<{tag}/>")
            } else {
                format!("<{tag} w:val=\"0\"/>")
            },
        );
    }
    if let Some(color) = line.color.or(furniture.color) {
        props.set("w:color", property("w:color", &[("w:val", hex(color))]));
    }
    let size =
        super::design::half_points(line.size.unwrap_or(size)).to_string();
    props.set("w:sz", property("w:sz", &[("w:val", size.clone())]));
    props.set("w:szCs", property("w:szCs", &[("w:val", size)]));
    let props = props.to_xml();
    segments(&line.text)
        .into_iter()
        .map(|segment| {
            let text_run = |text: &str| {
                format!(
                    "<w:r>{props}<w:t xml:space=\"preserve\">{}</w:t></w:r>",
                    escape(text)
                )
            };
            // Word formats a recomputed number like the field's own runs, so
            // they carry the line's properties as well as the cached result.
            match segment {
                Segment::Text(text) => text_run(&text),
                Segment::Page => {
                    xml::formatted_field("PAGE", &props, &text_run("1"))
                }
                Segment::Pages => {
                    xml::formatted_field("NUMPAGES", &props, &text_run("1"))
                }
            }
        })
        .collect()
}

/// One-point run properties for the rule's empty run, its paragraph mark and
/// the image paragraph's mark and drawing run.
const POINT_RUN: &str =
    "<w:rPr><w:sz w:val=\"2\"/><w:szCs w:val=\"2\"/></w:rPr>";

/// The furniture's own paragraphs (rule and image) in the slot's style, with
/// their spacing set explicitly so that neither the style nor the document
/// defaults (paragraph spacing, line height, body-size paragraph mark) move
/// the lines away from them.
fn furniture_props(style: &str, after: u32, line: (u32, &str)) -> Props {
    let mut props = Props::new("w:pPr", PPR);
    props.set("w:pStyle", property("w:pStyle", &[("w:val", style.into())]));
    props.set(
        "w:spacing",
        property(
            "w:spacing",
            &[
                ("w:before", "0".into()),
                ("w:after", after.to_string()),
                ("w:line", line.0.to_string()),
                ("w:lineRule", line.1.into()),
            ],
        ),
    );
    // A style with space keys sets contextualSpacing, which would drop the
    // image's gap before a line of the same style.
    props.set("w:contextualSpacing", "<w:contextualSpacing w:val=\"0\"/>");
    props.set("w:rPr", POINT_RUN);
    props
}

/// The rule is the border of an empty paragraph one point high (the
/// reference document's own construction), so it sits `w:space` (1 pt) from
/// the adjacent line as in the PDF and HTML.
fn rule_paragraph(rule: Rule, style: &str) -> String {
    let side = match rule.position {
        RulePosition::Above => "top",
        RulePosition::Below => "bottom",
    };
    let border = format!(
        "<w:pBdr><w:{side} w:val=\"single\" w:sz=\"{}\" w:space=\"1\" w:color=\"{}\"/></w:pBdr>",
        super::design::eighths(rule.width),
        hex(rule.color),
    );
    let mut props = furniture_props(style, 0, (20, "exact"));
    props.set("w:pBdr", border);
    format!("<w:p>{}<w:r>{POINT_RUN}<w:t/></w:r></w:p>", props.to_xml())
}

/// A decoded design picture: its natural size in points and registered media.
struct Picture {
    width: f64,
    height: f64,
    rel: String,
    name: String,
    svg: String,
}

/// Reads a furniture or background image, rendering unsupported formats
/// through the renderer, and registers its media with the part.
fn load_picture(
    path: &Path, width: Option<f64>, context: &mut PartContext<'_>,
    part: &mut FurniturePart,
) -> Result<Picture> {
    let bytes = std::fs::read(path)?;
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let (raster, svg, natural_width, natural_height) = match mime.essence_str()
    {
        "image/png" | "image/jpeg" => {
            let size = imagesize::blob_size(&bytes).map_err(|error| {
                Error::Config(format!(
                    "invalid design image {}: {error}",
                    path.display()
                ))
            })?;
            let ext = if mime.essence_str() == "image/png" {
                "png"
            } else {
                "jpeg"
            };
            (
                (ext, bytes),
                None,
                size.width as f64 * 0.75,
                size.height as f64 * 0.75,
            )
        }
        other => {
            let rendered = context.renderer.image(&bytes, other, width)?;
            let svg = (other == "image/svg+xml").then_some(bytes);
            (("png", rendered.png), svg, rendered.width_pt, rendered.height_pt)
        }
    };
    if natural_width <= 0.0 || natural_height <= 0.0 {
        return Err(Error::Config(format!(
            "design image {} has an empty size",
            path.display()
        )));
    }
    let (extension, bytes) = raster;
    let (rel, name) = image_rel(part, extension, bytes);
    let svg = svg.map(|bytes| {
        let (id, _) = image_rel(part, "svg", bytes);
        format!(
            "<a:extLst><a:ext uri=\"{{96DAC541-7B7A-43D3-8B79-37D633B846F1}}\"><asvg:svgBlip xmlns:asvg=\"http://schemas.microsoft.com/office/drawing/2016/SVG/main\" r:embed=\"{id}\"/></a:ext></a:extLst>"
        )
    }).unwrap_or_default();
    Ok(Picture { width: natural_width, height: natural_height, rel, name, svg })
}

fn emu(points: f64) -> u64 {
    (points * 12700.0).round().max(1.0) as u64
}

fn image_paragraph(
    furniture: &Furniture, style: &str, context: &mut PartContext<'_>,
    part: &mut FurniturePart,
) -> Result<String> {
    let image = furniture.image.as_ref().expect("image is present");
    let picture =
        load_picture(&image.path, image.width.map(|pt| pt.0), context, part)?;
    let width = image.width.map_or(picture.width, |pt| pt.0);
    let height = width * picture.height / picture.width;
    let id = context.ids.doc_pr;
    context.ids.doc_pr += 1;
    let drawing = InlineDrawing {
        props: POINT_RUN,
        width: emu(width),
        height: emu(height),
        id,
        title: "",
        svg: &picture.svg,
        alt: "",
        name: &picture.name,
        rel: &picture.rel,
    }
    .to_xml();
    // Single spacing and one-point runs make the line as high as the
    // picture. An unset gap keeps the document defaults' spacing after:
    // Word's own paragraph spacing.
    let after = image.gap.map_or(context.default_after, super::design::twips);
    let mut props = furniture_props(style, after, (240, "auto"));
    props.set(
        "w:jc",
        property(
            "w:jc",
            &[(
                "w:val",
                alignment(image.align.unwrap_or(furniture.align)).into(),
            )],
        ),
    );
    Ok(format!("<w:p>{}{drawing}</w:p>", props.to_xml()))
}

/// The `w:after` of the document defaults' paragraph properties, in twips.
fn default_after(styles_xml: &str) -> u32 {
    let Ok(root) = parse_xml(styles_xml) else {
        return 0;
    };
    let mut node = &root;
    for name in ["w:docDefaults", "w:pPrDefault", "w:pPr", "w:spacing"] {
        match node.children.iter().find(|child| child.name == name) {
            Some(child) => node = child,
            None => return 0,
        }
    }
    node.attr("w:after").and_then(|after| after.parse().ok()).unwrap_or(0)
}

/// The page background as Word stores a "behind text" picture: a run with an
/// anchor relative to the page. `cover` crops with `a:srcRect`; `contain`
/// shrinks the extent and offsets it; `stretch` fills the page.
fn background_run(
    background: &Background, context: &mut PartContext<'_>,
    part: &mut FurniturePart,
) -> Result<String> {
    let image = &background.image;
    let picture = load_picture(&image.path, None, context, part)?;
    let (page_width, page_height) = context.page_size;
    let page = (f64::from(page_width) * 635.0, f64::from(page_height) * 635.0);
    let page_aspect = page.0 / page.1;
    let aspect = picture.width / picture.height;
    let wide = aspect > page_aspect;
    let horizontal = match image.align.horizontal {
        Align::Left => 0.0,
        Align::Right => 1.0,
        Align::Center | Align::Justify => 0.5,
    };
    let vertical = match image.align.vertical {
        VerticalAlign::Top => 0.0,
        VerticalAlign::Center => 0.5,
        VerticalAlign::Bottom => 1.0,
    };
    let (extent, offset, source_rect) = match image.fit {
        Fit::Stretch => (page, (0.0, 0.0), String::new()),
        Fit::Cover => {
            // Crop the overflow of the dimension the page does not bind, in
            // thousandths of a percent, keeping the aligned part.
            let (cut, keep) = if wide {
                (1.0 - page_aspect / aspect, horizontal)
            } else {
                (1.0 - aspect / page_aspect, vertical)
            };
            let percent = |fraction: f64| (fraction * 100_000.0).round() as u64;
            let (start, end) =
                (percent(cut * keep), percent(cut * (1.0 - keep)));
            let rect = if start + end == 0 {
                String::new()
            } else if wide {
                format!("<a:srcRect l=\"{start}\" r=\"{end}\"/>")
            } else {
                format!("<a:srcRect t=\"{start}\" b=\"{end}\"/>")
            };
            (page, (0.0, 0.0), rect)
        }
        Fit::Contain => {
            let extent = if wide {
                (page.0, page.0 / aspect)
            } else {
                (page.1 * aspect, page.1)
            };
            let offset = (
                (page.0 - extent.0) * horizontal,
                (page.1 - extent.1) * vertical,
            );
            (extent, offset, String::new())
        }
    };
    let id = context.ids.doc_pr;
    context.ids.doc_pr += 1;
    Ok(AnchoredDrawing {
        width: extent.0.round() as u64,
        height: extent.1.round() as u64,
        x: offset.0.round() as u64,
        y: offset.1.round() as u64,
        id,
        svg: &picture.svg,
        name: &picture.name,
        rel: &picture.rel,
        source_rect: &source_rect,
    }
    .to_xml())
}

/// Opens the part's first paragraph, after its properties, with the anchored
/// picture so the furniture keeps its place. A part without furniture gets
/// one paragraph of Word's minimum exact line height (0.7 pt) instead.
fn with_anchor(body: &str, run: &str) -> String {
    let Some(rest) = body.strip_prefix("<w:p>") else {
        let mut props = Props::new("w:pPr", PPR);
        props.set(
            "w:spacing",
            property(
                "w:spacing",
                &[
                    ("w:before", "0".into()),
                    ("w:after", "0".into()),
                    ("w:line", "14".into()),
                    ("w:lineRule", "exact".into()),
                ],
            ),
        );
        let rest = body.strip_prefix("<w:p/>").unwrap_or(body);
        return format!("<w:p>{}{run}</w:p>{rest}", props.to_xml());
    };
    let split = match rest.strip_prefix("<w:pPr>") {
        Some(_) => {
            rest.find("</w:pPr>").map_or(0, |end| end + "</w:pPr>".len())
        }
        None => 0,
    };
    format!("<w:p>{}{run}{}", &rest[..split], &rest[split..])
}

/// A picture anchored behind the text relative to the page.
struct AnchoredDrawing<'a> {
    width: u64,
    height: u64,
    x: u64,
    y: u64,
    id: u32,
    svg: &'a str,
    name: &'a str,
    rel: &'a str,
    source_rect: &'a str,
}

impl AnchoredDrawing<'_> {
    fn to_xml(&self) -> String {
        let Self { width, height, x, y, id, svg, name, rel, source_rect } =
            self;
        format!(
            concat!(
                "<w:r><w:drawing><wp:anchor distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\" simplePos=\"0\" relativeHeight=\"0\" behindDoc=\"1\" locked=\"0\" layoutInCell=\"1\" allowOverlap=\"1\">",
                "<wp:simplePos x=\"0\" y=\"0\"/>",
                "<wp:positionH relativeFrom=\"page\"><wp:posOffset>{x}</wp:posOffset></wp:positionH>",
                "<wp:positionV relativeFrom=\"page\"><wp:posOffset>{y}</wp:posOffset></wp:positionV>",
                "<wp:extent cx=\"{width}\" cy=\"{height}\"/><wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/><wp:wrapNone/>",
                "<wp:docPr id=\"{id}\" name=\"Background {id}\" descr=\"\"/>",
                "<wp:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect=\"1\"/></wp:cNvGraphicFramePr>",
                "{graphic}</wp:anchor></w:drawing></w:r>",
            ),
            x = x,
            y = y,
            width = width,
            height = height,
            id = id,
            graphic =
                picture_graphic(name, rel, svg, *width, *height, source_rect),
        )
    }
}

fn image_rel(
    part: &mut FurniturePart, extension: &str, bytes: Vec<u8>,
) -> (String, String) {
    let hash = crate::assets::registry::content_hash(&bytes);
    let name = format!("writer-design-{hash}.{extension}");
    let id = format!("rId{}", part.rels.len() + 1);
    part.rels.push(Relationship {
        id: id.clone(),
        kind: xml::REL_IMAGE.into(),
        target: format!("media/{name}"),
        external: false,
    });
    part.media.push((name.clone(), bytes));
    (id, name)
}

fn alignment(align: Align) -> &'static str {
    match align {
        Align::Left => "left",
        Align::Center => "center",
        Align::Right => "right",
        Align::Justify => "both",
    }
}

fn hex(color: Color) -> String {
    color.to_string().trim_start_matches('#').to_ascii_uppercase()
}

fn property(name: &str, attrs: &[(&str, String)]) -> String {
    let attrs = attrs
        .iter()
        .map(|(key, value)| format!(" {key}=\"{}\"", escape(value)))
        .collect::<String>();
    format!("<{name}{attrs}/>")
}

fn even_and_odd_headers(reference: Option<&Reference>) -> bool {
    reference
        .and_then(|reference| reference.settings_xml.as_deref())
        .and_then(|settings| parse_xml(settings).ok())
        .and_then(|root| {
            root.children
                .into_iter()
                .find(|node| node.name == "w:evenAndOddHeaders")
        })
        .is_some_and(|setting| {
            matches!(setting.attr("w:val"), None | Some("1" | "true" | "on"))
        })
}
