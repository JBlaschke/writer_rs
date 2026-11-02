//! Design header/footer parts and shared first-page section references.

use crate::design::{
    Align, Color, Design, Furniture, Line, Pt, Rule, RulePosition, Segment,
    Slot, segments,
};
use crate::error::{Error, Result};

use super::images::{InlineDrawing, normal_scale};
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
}

/// Generates all declared furniture when the reference has no body section.
pub(crate) fn generate(
    emitter: &mut Emitter<'_>, reference: Option<&Reference>,
) -> Result<()> {
    let design = emitter.design;
    if !design.declared
        || reference.is_some_and(|reference| reference.sect_pr.is_some())
        || (design.page.header.is_none() && design.page.footer.is_none())
    {
        return Ok(());
    }
    let first_page = [&design.page.header, &design.page.footer]
        .into_iter()
        .flatten()
        .any(|slot| slot.first != slot.rest);
    let body_size = Pt(11.0 * normal_scale(&emitter.styles.to_xml()));
    let mut context = PartContext {
        design,
        styles: &mut emitter.styles,
        ids: &mut emitter.ids,
        renderer: emitter.renderer,
        text_width: emitter.layout.text_width_twips,
        body_size,
    };
    let mut headers = Vec::new();
    let mut footers = Vec::new();
    for (header, slot, references) in [
        (true, design.page.header.as_ref(), &mut headers),
        (false, design.page.footer.as_ref(), &mut footers),
    ] {
        let Some(slot) = slot else {
            continue;
        };
        for (pages, furniture) in plan_slot(slot, first_page) {
            let (number, id) = emitter.package.reserve_furniture(header);
            let part = if header {
                header_part(furniture, number, &mut context)?
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

fn plan_slot(
    slot: &Slot, first_page: bool,
) -> Vec<(Vec<&'static str>, Option<&Furniture>)> {
    if slot.first == slot.rest {
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
    furniture: Option<&Furniture>, number: usize, context: &mut PartContext<'_>,
) -> Result<FurniturePart> {
    furniture_part(furniture, true, number, context)
}

pub(crate) fn footer_part(
    furniture: Option<&Furniture>, number: usize, context: &mut PartContext<'_>,
) -> Result<FurniturePart> {
    furniture_part(furniture, false, number, context)
}

fn furniture_part(
    furniture: Option<&Furniture>, header: bool, number: usize,
    context: &mut PartContext<'_>,
) -> Result<FurniturePart> {
    let name = if header { "header" } else { "footer" };
    let tag = if header { "hdr" } else { "ftr" };
    let mut part = FurniturePart {
        name: format!("word/{name}{number}.xml"),
        xml: String::new(),
        rels: Vec::new(),
        media: Vec::new(),
    };
    let body = if let Some(furniture) = furniture {
        furniture_paragraphs(furniture, header, context, &mut part)?
    } else {
        "<w:p/>".into()
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
    if let Some(rule) =
        furniture.rule.filter(|rule| rule.position == RulePosition::Above)
    {
        xml.push_str(&rule_paragraph(rule));
    }
    if furniture.image.is_some() {
        xml.push_str(&image_paragraph(furniture, context, part)?);
    }
    let style = context
        .styles
        .ensure(if header { "Header" } else { "Footer" }, StyleKind::Paragraph);
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
        xml.push_str(&rule_paragraph(rule));
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
            match segment {
                Segment::Text(text) => text_run(&text),
                Segment::Page => xml::field("PAGE", &text_run("1")),
                Segment::Pages => xml::field("NUMPAGES", &text_run("1")),
            }
        })
        .collect()
}

fn rule_paragraph(rule: Rule) -> String {
    let side = match rule.position {
        RulePosition::Above => "top",
        RulePosition::Below => "bottom",
    };
    let border = format!(
        "<w:pBdr><w:{side} w:val=\"single\" w:sz=\"{}\" w:space=\"1\" w:color=\"{}\"/></w:pBdr>",
        super::design::eighths(rule.width),
        hex(rule.color),
    );
    let mut props = Props::new("w:pPr", PPR);
    props.set("w:pBdr", border);
    format!(
        "<w:p>{}<w:r><w:rPr><w:sz w:val=\"2\"/></w:rPr><w:t/></w:r></w:p>",
        props.to_xml()
    )
}

fn image_paragraph(
    furniture: &Furniture, context: &mut PartContext<'_>,
    part: &mut FurniturePart,
) -> Result<String> {
    let image = furniture.image.as_ref().expect("image is present");
    let bytes = std::fs::read(&image.path)?;
    let mime = mime_guess::from_path(&image.path).first_or_octet_stream();
    let (raster, svg, natural_width, natural_height) = match mime.essence_str()
    {
        "image/png" | "image/jpeg" => {
            let size = imagesize::blob_size(&bytes).map_err(|error| {
                Error::Config(format!(
                    "invalid design furniture image {}: {error}",
                    image.path.display()
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
            let rendered = context.renderer.image(
                &bytes,
                other,
                image.width.map(|pt| pt.0),
            )?;
            let svg = (other == "image/svg+xml").then_some(bytes);
            (("png", rendered.png), svg, rendered.width_pt, rendered.height_pt)
        }
    };
    if natural_width <= 0.0 || natural_height <= 0.0 {
        return Err(Error::Config(
            "design furniture image has an empty size".into(),
        ));
    }
    let width = image.width.map_or(natural_width, |pt| pt.0);
    let height = width * natural_height / natural_width;
    let width = (width * 12700.0).round().max(1.0) as u64;
    let height = (height * 12700.0).round().max(1.0) as u64;
    let (extension, bytes) = raster;
    let (rel, name) = image_rel(part, extension, bytes);
    let svg = svg.map(|bytes| {
        let (id, _) = image_rel(part, "svg", bytes);
        format!(
            "<a:extLst><a:ext uri=\"{{96DAC541-7B7A-43D3-8B79-37D633B846F1}}\"><asvg:svgBlip xmlns:asvg=\"http://schemas.microsoft.com/office/drawing/2016/SVG/main\" r:embed=\"{id}\"/></a:ext></a:extLst>"
        )
    }).unwrap_or_default();
    let id = context.ids.doc_pr;
    context.ids.doc_pr += 1;
    let drawing = InlineDrawing {
        props: "",
        width,
        height,
        id,
        title: "",
        svg: &svg,
        alt: "",
        name: &name,
        rel: &rel,
    }
    .to_xml();
    let mut props = Props::new("w:pPr", PPR);
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
