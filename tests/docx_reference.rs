//! Reproducible hand-written reference fixture and preservation checks.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use writer::assets::registry::AssetRegistry;
use writer::config::Config;
use writer::docx::xml::{
    CT_APP, CT_DOCUMENT, CT_FONT_TABLE, CT_HEADER, CT_NUMBERING, CT_RELS,
    CT_SETTINGS, CT_STYLES, CT_THEME, CT_WEB_SETTINGS, NS_A, NS_PIC, NS_R,
    NS_RELS, NS_W, NS_WP, REL_APP, REL_FONT_TABLE, REL_HEADER, REL_IMAGE,
    REL_NUMBERING, REL_OFFICE_DOCUMENT, REL_SETTINGS, REL_STYLES, REL_THEME,
    REL_WEB_SETTINGS, XML_HEAD,
};
use writer::docx::{self, DocxOptions, Picture, Renderer, ValidateOptions};
use writer::error::{Error, Result};
use writer::ir::{Document, MathDialect};
use zip::{
    CompressionMethod, DateTime, System, ZipArchive, ZipWriter,
    write::SimpleFileOptions,
};

#[path = "common/math_xml.rs"]
mod math_xml;

const PNG: &[u8] = include_bytes!("fixtures/md/images/plot.png");
const BODY_PNG: &[u8] = include_bytes!("fixtures/docx/images/dpi144.png");
const COPIED_DRAWING_ID: u32 = 9_001;
const COPIED_BOOKMARK_ID: u32 = 12_001;

fn reference_header() -> String {
    format!(
        r#"<w:hdr xmlns:w="{NS_W}" xmlns:r="{NS_R}" xmlns:a="{NS_A}" xmlns:wp="{NS_WP}" xmlns:pic="{NS_PIC}"><w:p><w:bookmarkStart w:id="{COPIED_BOOKMARK_ID}" w:name="ReferenceHeader"/><w:r><w:t xml:space="preserve">REFERENCE HEADER</w:t></w:r><w:r><w:drawing><wp:inline><wp:extent cx="1219200" cy="609600"/><wp:docPr id="{COPIED_DRAWING_ID}" name="Reference header picture" descr="Reference PNG"/><wp:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect="1"/></wp:cNvGraphicFramePr><a:graphic><a:graphicData uri="{NS_PIC}"><pic:pic><pic:nvPicPr><pic:cNvPr id="0" name="header-image.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rIdHeaderImage"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1219200" cy="609600"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r><w:bookmarkEnd w:id="{COPIED_BOOKMARK_ID}"/></w:p></w:hdr>"#
    )
}

fn reference_theme() -> String {
    let colors = [
        ("dk1", "000000"),
        ("lt1", "FFFFFF"),
        ("dk2", "222222"),
        ("lt2", "EEEEEE"),
        ("accent1", "4472C4"),
        ("accent2", "ED7D31"),
        ("accent3", "A5A5A5"),
        ("accent4", "FFC000"),
        ("accent5", "5B9BD5"),
        ("accent6", "70AD47"),
        ("hlink", "0563C1"),
        ("folHlink", "954F72"),
    ]
    .into_iter()
    .map(|(name, color)| {
        format!(r#"<a:{name}><a:srgbClr val="{color}"/></a:{name}>"#)
    })
    .collect::<String>();
    let fill = r#"<a:solidFill><a:schemeClr val="phClr"/></a:solidFill>"#;
    let lines =
        format!(r#"<a:ln w="12700">{fill}<a:prstDash val="solid"/></a:ln>"#)
            .repeat(3);
    let effects = "<a:effectStyle><a:effectLst/></a:effectStyle>".repeat(3);
    let background = fill.repeat(3);
    format!(
        r#"<a:theme xmlns:a="{NS_A}" xmlns:r="{NS_R}" name="Reference Theme"><a:themeElements><a:clrScheme name="Reference">{colors}</a:clrScheme><a:fontScheme name="Reference"><a:majorFont><a:latin typeface="Reference Font"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Reference Font"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme><a:fmtScheme name="Reference"><a:fillStyleLst><a:blipFill dpi="0" rotWithShape="1"><a:blip r:embed="rIdThemeImage"/><a:stretch><a:fillRect/></a:stretch></a:blipFill>{fill}{fill}</a:fillStyleLst><a:lnStyleLst>{lines}</a:lnStyleLst><a:effectStyleLst>{effects}</a:effectStyleLst><a:bgFillStyleLst>{background}</a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>"#
    )
}

fn image_relationship(id: &str, target: &str) -> String {
    format!(
        r#"<Relationships xmlns="{NS_RELS}"><Relationship Id="{id}" Type="{REL_IMAGE}" Target="{target}"/></Relationships>"#
    )
}

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/docx/reference.docx")
}

fn localized_fixture_path() -> PathBuf {
    fixture_path().with_file_name("reference-localized.docx")
}

fn reference_fixture() -> Vec<u8> {
    reference_fixture_with_heading_id("Heading1")
}

fn localized_reference_fixture() -> Vec<u8> {
    reference_fixture_with_heading_id("berschrift1")
}

fn reference_fixture_with_heading_id(heading_id: &str) -> Vec<u8> {
    let types = [
        ("docProps/app.xml", CT_APP),
        ("word/document.xml", CT_DOCUMENT),
        ("word/styles.xml", CT_STYLES),
        ("word/numbering.xml", CT_NUMBERING),
        ("word/settings.xml", CT_SETTINGS),
        ("word/header1.xml", CT_HEADER),
        ("word/theme/theme1.xml", CT_THEME),
        ("word/fontTable.xml", CT_FONT_TABLE),
        ("word/webSettings.xml", CT_WEB_SETTINGS),
    ]
    .into_iter()
    .map(|(path, content_type)| {
        format!(
            r#"<Override PartName="/{path}" ContentType="{content_type}"/>"#
        )
    })
    .collect::<String>();
    let rels = [
        ("rId1", REL_STYLES, "styles.xml"),
        ("rId2", REL_NUMBERING, "numbering.xml"),
        ("rId3", REL_SETTINGS, "settings.xml"),
        ("rId4", REL_THEME, "theme/theme1.xml"),
        ("rId5", REL_FONT_TABLE, "fontTable.xml"),
        ("rId6", REL_WEB_SETTINGS, "webSettings.xml"),
        ("rId100", REL_HEADER, "header1.xml"),
    ]
    .into_iter()
    .map(|(id, kind, target)| {
        format!(r#"<Relationship Id="{id}" Type="{kind}" Target="{target}"/>"#)
    })
    .collect::<String>();
    let mut parts = vec![
        ("[Content_Types].xml", format!(r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="{CT_RELS}"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/>{types}</Types>"#)),
        ("_rels/.rels", format!(r#"<Relationships xmlns="{NS_RELS}"><Relationship Id="rId1" Type="{REL_OFFICE_DOCUMENT}" Target="word/document.xml"/><Relationship Id="rId2" Type="{REL_APP}" Target="docProps/app.xml"/></Relationships>"#)),
        ("docProps/app.xml", r#"<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>REFERENCE APP</Application></Properties>"#.into()),
        ("word/document.xml", format!(r#"<w:document xmlns:w="{NS_W}" xmlns:r="{NS_R}"><w:body><w:p><w:pPr><w:sectPr><w:pgSz w:w="12240" w:h="15840"/></w:sectPr></w:pPr><w:r><w:t xml:space="preserve">REFERENCE BODY</w:t></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rId100"/><w:pgSz w:w="8391" w:h="11906"/><w:pgMar w:top="1134" w:right="1134" w:bottom="1134" w:left="1134" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#)),
        ("word/_rels/document.xml.rels", format!(r#"<Relationships xmlns="{NS_RELS}">{rels}</Relationships>"#)),
        ("word/styles.xml", reference_styles(heading_id)),
        ("word/numbering.xml", reference_numbering()),
        ("word/settings.xml", reference_settings()),
        ("word/fontTable.xml", format!(r#"<w:fonts xmlns:w="{NS_W}"><w:font w:name="Reference Font"/></w:fonts>"#)),
        ("word/header1.xml", reference_header()),
        ("word/_rels/header1.xml.rels", image_relationship("rIdHeaderImage", "media/header-image.png")),
        ("word/theme/theme1.xml", reference_theme()),
        ("word/theme/_rels/theme1.xml.rels", image_relationship("rIdThemeImage", "../media/theme-fill.png")),
        ("word/webSettings.xml", format!(r#"<w:webSettings xmlns:w="{NS_W}"><w:allowPNG/></w:webSettings>"#)),
    ];
    // Every source part is hand-written; only OPC compression is shared with writer.
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(6))
        .last_modified_time(DateTime::default())
        .system(System::Unix)
        .unix_permissions(0o644);
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, xml) in &mut parts {
        zip.start_file(*name, options).unwrap();
        zip.write_all(format!("{XML_HEAD}{xml}").as_bytes()).unwrap();
    }
    for name in ["word/media/header-image.png", "word/media/theme-fill.png"] {
        zip.start_file(name, options).unwrap();
        zip.write_all(PNG).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn reference_styles(heading_id: &str) -> String {
    format!(
        r#"<w:styles xmlns:w="{NS_W}"><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style><w:style w:type="paragraph" w:styleId="{heading_id}"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:rPr><w:color w:val="FF0000"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Callout"><w:name w:val="Callout"/><w:basedOn w:val="Normal"/><w:pPr><w:shd w:val="clear" w:fill="F0F0FF"/></w:pPr></w:style><w:style w:type="table" w:styleId="Table"><w:name w:val="Table"/><w:basedOn w:val="TableNormal"/></w:style></w:styles>"#
    )
}

fn reference_numbering() -> String {
    format!(
        r#"<w:numbering xmlns:w="{NS_W}"><w:abstractNum w:abstractNumId="5"><w:multiLevelType w:val="multilevel"/><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/></w:lvl></w:abstractNum><w:abstractNum w:abstractNumId="991"><w:multiLevelType w:val="singleLevel"/><w:lvl w:ilvl="0"><w:start w:val="99"/><w:numFmt w:val="decimal"/><w:lvlText w:val="REFERENCE 991"/></w:lvl></w:abstractNum><w:num w:numId="5"><w:abstractNumId w:val="5"/></w:num><w:num w:numId="1000"><w:abstractNumId w:val="991"/></w:num></w:numbering>"#
    )
}

fn reference_settings() -> String {
    format!(
        r#"<w:settings xmlns:w="{NS_W}" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" mc:Ignorable="w14"><w:updateFields w:val="false"/><w:footnotePr><w:numRestart w:val="continuous"/></w:footnotePr><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="14"/></w:compat><w:rsids><w:rsidRoot w:val="12345678"/><w:rsid w:val="12345678"/></w:rsids><w14:docId w14:val="42"/></w:settings>"#
    )
}

#[test]
#[ignore = "explicitly regenerates the committed binary fixture"]
fn regenerate_reference_fixture() {
    let path = fixture_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, reference_fixture()).unwrap();
}

#[test]
#[ignore = "explicitly regenerates the committed localized binary fixture"]
fn regenerate_localized_reference_fixture() {
    let path = localized_fixture_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, localized_reference_fixture()).unwrap();
}

#[test]
fn localized_reference_fixture_is_up_to_date() {
    assert_eq!(
        std::fs::read(localized_fixture_path()).unwrap(),
        localized_reference_fixture()
    );
}

#[test]
fn reference_fixture_is_up_to_date() {
    assert_eq!(std::fs::read(fixture_path()).unwrap(), reference_fixture());
}

struct NoRenderer;

impl Renderer for NoRenderer {
    fn equation(
        &mut self, _: &str, _: MathDialect, _: bool,
    ) -> Result<Picture> {
        Err(Error::Other("unexpected equation rendering".into()))
    }

    fn image(&mut self, _: &[u8], _: &str, _: Option<f64>) -> Result<Picture> {
        Err(Error::Other("unexpected image rendering".into()))
    }

    fn typst_mathml(&mut self, _: &str, _: bool) -> Result<String> {
        Err(Error::Other("unexpected math conversion".into()))
    }
}

fn render_reference() -> Vec<u8> {
    render_with_reference(&fixture_path())
}

fn render_with_reference(reference: &Path) -> Vec<u8> {
    let mut assets = AssetRegistry::new();
    let image_id =
        assets.register_bytes(BODY_PNG.to_vec(), "image/png", "body-image.png");
    let document: Document = serde_json::from_value(serde_json::json!({
        "meta": { "title": "Reference preservation" },
        "blocks": [{
            "type": "heading", "level": 1, "number": "1", "attrs": {"id": "generated-heading"},
            "inlines": [{ "type": "text", "text": "One heading" }]
        }, {
            "type": "paragraph",
            "inlines": [{ "type": "text", "text": "One paragraph" }]
        }, {
            "type": "paragraph", "inlines": [{ "type": "image",
                "url": format!("asset:{image_id}"), "attrs": {},
                "alt": [{ "type": "text", "text": "Generated body PNG" }],
                "title": null
            }]
        }],
        "features": {}, "assets": [],
        "source": { "path": "/document.md", "kind": "markdown", "base_dir": "/" }
    })).unwrap();
    let config = Config::default();
    let output = docx::write(
        &document,
        &assets,
        &mut NoRenderer,
        &DocxOptions {
            config: &config,
            reference: Some(reference),
            source_date_epoch: None,
            all_styles: false,
        },
    )
    .unwrap();
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let violations = docx::validate(
        &output.bytes,
        &ValidateOptions { builtin_styles: false },
    );
    assert!(violations.is_empty(), "{violations:#?}");
    output.bytes
}

fn read_parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..archive.len())
        .map(|i| {
            let mut entry = archive.by_index(i).unwrap();
            let name = entry.name().unwrap().into_owned();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            (name, bytes)
        })
        .collect()
}

#[test]
fn reference_is_honoured() {
    let output = read_parts(&render_reference());
    let original = read_parts(&std::fs::read(fixture_path()).unwrap());
    let xml = |name| std::str::from_utf8(&output[name]).unwrap();
    let styles = xml("word/styles.xml");
    assert!(styles.contains(r#"<w:color w:val="FF0000"/>"#));
    assert!(styles.contains(r#"<w:shd w:val="clear" w:fill="F0F0FF"/>"#));
    assert!(styles.contains(r#"w:styleId="TableNormal""#));
    for path in [
        "word/header1.xml", "word/theme/theme1.xml", "word/fontTable.xml",
        "word/webSettings.xml",
    ] {
        assert_eq!(output[path], original[path], "{path}");
    }
    let relationships = xml("word/_rels/document.xml.rels");
    assert!(relationships.contains(r#"Id="rId100""#));
    assert!(relationships.contains(r#"Id="rId101""#));
    let document = xml("word/document.xml");
    assert!(document.contains(r#"<w:pgSz w:w="8391" w:h="11906"/>"#));
    assert!(document.contains(r#"w:left="1134""#));
    assert!(!document.contains("REFERENCE BODY"));
    assert!(!output.keys().any(|path| path.starts_with("word/footer")));
    let settings = xml("word/settings.xml");
    assert!(settings.contains(r#"<w:updateFields w:val="true"/>"#));
    assert!(settings.contains(r#"<w:rsidRoot w:val="12345678"/>"#));
    assert!(settings.contains(r#"w:val="14""#));
    assert!(settings.contains("xmlns:w14="));
    for dropped in ["w:footnotePr", "w14:docId", "mc:Ignorable"] {
        assert!(!settings.contains(dropped), "{dropped}");
    }
    let numbering = xml("word/numbering.xml");
    assert!(numbering.contains(r#"<w:abstractNum w:abstractNumId="5">"#));
    assert!(numbering.contains(r#"<w:num w:numId="5">"#));
    assert_eq!(
        numbering.matches(r#"<w:abstractNum w:abstractNumId="991">"#).count(),
        1
    );
    assert!(!numbering.contains("REFERENCE 991"));
    assert!(!numbering.contains(r#"<w:num w:numId="1000">"#));
    assert!(
        xml("docProps/app.xml").contains("<Application>writer</Application>")
    );
}

#[test]
fn reference_output_validates() {
    assert_eq!(
        docx::validate(
            &render_reference(),
            &ValidateOptions { builtin_styles: false }
        ),
        []
    );
}

#[test]
fn section_inherits_extra_document_root_namespaces() {
    let mut parts = read_parts(&reference_fixture());
    let namespace = "http://schemas.microsoft.com/office/word/2010/wordml";
    let marker = r#"<w14:sectionMarker w14:val="kept"/>"#;
    let source = String::from_utf8(parts["word/document.xml"].clone())
        .unwrap()
        .replace(
            "<w:document ",
            &format!(r#"<w:document xmlns:w14="{namespace}" "#),
        )
        .replace(
            "</w:sectPr></w:body>",
            &format!("{marker}</w:sectPr></w:body>"),
        );
    parts.insert("word/document.xml".into(), source.into_bytes());
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(6))
        .last_modified_time(DateTime::default())
        .system(System::Unix)
        .unix_permissions(0o644);
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (path, bytes) in parts {
        zip.start_file(path, options).unwrap();
        zip.write_all(&bytes).unwrap();
    }
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), zip.finish().unwrap().into_inner()).unwrap();
    let output = render_with_reference(file.path());
    let parts = read_parts(&output);
    let document = std::str::from_utf8(&parts["word/document.xml"]).unwrap();
    let root_end = document.find("><w:body>").unwrap();
    assert!(
        document[..root_end].contains(&format!(r#"xmlns:w14="{namespace}""#))
    );
    assert!(document.contains(marker));
    assert_eq!(
        docx::validate(&output, &ValidateOptions { builtin_styles: false }),
        []
    );
}

#[test]
fn copied_header_theme_image_closure_is_exact_and_generated_ids_follow_its_maxima()
 {
    let original = read_parts(&std::fs::read(fixture_path()).unwrap());
    let output = read_parts(&render_reference());
    let copied = [
        "word/header1.xml",
        "word/_rels/header1.xml.rels",
        "word/theme/theme1.xml",
        "word/theme/_rels/theme1.xml.rels",
        "word/media/header-image.png",
        "word/media/theme-fill.png",
        "word/fontTable.xml",
        "word/webSettings.xml",
    ];
    let mut drawing_max = 0;
    let mut bookmark_max = 0;
    for name in copied {
        assert_eq!(
            output[name], original[name],
            "copied closure changed: {name}"
        );
        if !name.ends_with(".xml") {
            continue;
        }
        let root =
            math_xml::parse(std::str::from_utf8(&original[name]).unwrap());
        for drawing in root.descendants("wp:docPr") {
            drawing_max = drawing_max
                .max(drawing.attr("id").unwrap().parse::<u32>().unwrap());
        }
        for bookmark in root.descendants("w:bookmarkStart") {
            bookmark_max = bookmark_max
                .max(bookmark.attr("w:id").unwrap().parse::<u32>().unwrap());
        }
    }
    assert_eq!(drawing_max, COPIED_DRAWING_ID);
    assert_eq!(bookmark_max, COPIED_BOOKMARK_ID);
    let body = math_xml::parse(
        std::str::from_utf8(&output["word/document.xml"]).unwrap(),
    );
    let drawings = body.descendants("wp:docPr");
    assert!(!drawings.is_empty(), "exercise generated drawing IDs");
    for drawing in drawings {
        assert!(
            drawing.attr("id").unwrap().parse::<u32>().unwrap() > drawing_max
        );
    }
    let bookmarks = body.descendants("w:bookmarkStart");
    assert!(!bookmarks.is_empty(), "exercise generated bookmark IDs");
    for bookmark in bookmarks {
        assert!(
            bookmark.attr("w:id").unwrap().parse::<u32>().unwrap()
                > bookmark_max
        );
    }
    let header = math_xml::parse(
        std::str::from_utf8(&output["word/header1.xml"]).unwrap(),
    );
    assert_eq!(
        header.descendants("a:blip")[0].attr("r:embed"),
        Some("rIdHeaderImage")
    );
    let theme = math_xml::parse(
        std::str::from_utf8(&output["word/theme/theme1.xml"]).unwrap(),
    );
    assert_eq!(theme.descendants("a:blipFill").len(), 1);
    assert_eq!(
        theme.descendants("a:blip")[0].attr("r:embed"),
        Some("rIdThemeImage")
    );
    assert_eq!(
        output.keys().filter(|name| name.starts_with("word/media/")).count(),
        3
    );
}

#[test]
fn localized_heading_style_is_looked_up_by_name_without_a_duplicate_definition()
{
    let output = read_parts(&render_with_reference(&localized_fixture_path()));
    let styles = math_xml::parse(
        std::str::from_utf8(&output["word/styles.xml"]).unwrap(),
    );
    let headings: Vec<_> = styles
        .descendants("w:style")
        .into_iter()
        .filter(|style| {
            style.attr("w:type") == Some("paragraph")
                && style
                    .child("w:name")
                    .and_then(|name| name.attr("w:val"))
                    .is_some_and(|name| name.eq_ignore_ascii_case("heading 1"))
        })
        .collect();
    assert_eq!(headings.len(), 1);
    assert_eq!(headings[0].attr("w:styleId"), Some("berschrift1"));
    assert_eq!(
        headings[0].descendants("w:color")[0].attr("w:val"),
        Some("FF0000")
    );
    let body = math_xml::parse(
        std::str::from_utf8(&output["word/document.xml"]).unwrap(),
    );
    let paragraph = body
        .descendants("w:p")
        .into_iter()
        .find(|paragraph| paragraph.text_content().contains("One heading"))
        .unwrap();
    assert_eq!(
        paragraph
            .child("w:pPr")
            .unwrap()
            .child("w:pStyle")
            .unwrap()
            .attr("w:val"),
        Some("berschrift1")
    );
    assert!(
        !styles
            .descendants("w:style")
            .iter()
            .any(|style| style.attr("w:styleId") == Some("Heading1"))
    );
}
