//! Reproducible hand-written reference fixture and preservation checks.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use writer::assets::registry::AssetRegistry;
use writer::config::Config;
use writer::docx::xml::{
    CT_APP, CT_DOCUMENT, CT_FONT_TABLE, CT_HEADER, CT_NUMBERING, CT_RELS,
    CT_SETTINGS, CT_STYLES, CT_THEME, CT_WEB_SETTINGS, NS_A, NS_R, NS_RELS,
    NS_W, REL_APP, REL_FONT_TABLE, REL_HEADER, REL_NUMBERING,
    REL_OFFICE_DOCUMENT, REL_SETTINGS, REL_STYLES, REL_THEME, REL_WEB_SETTINGS,
    XML_HEAD,
};
use writer::docx::{self, DocxOptions, Picture, Renderer, ValidateOptions};
use writer::error::{Error, Result};
use writer::ir::{Document, MathDialect};
use zip::{
    CompressionMethod, DateTime, System, ZipArchive, ZipWriter,
    write::SimpleFileOptions,
};

const HEADER: &str = r#"<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t xml:space="preserve">REFERENCE HEADER</w:t></w:r></w:p></w:hdr>"#;

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/docx/reference.docx")
}

fn reference_fixture() -> Vec<u8> {
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
        ("[Content_Types].xml", format!(r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="{CT_RELS}"/><Default Extension="xml" ContentType="application/xml"/>{types}</Types>"#)),
        ("_rels/.rels", format!(r#"<Relationships xmlns="{NS_RELS}"><Relationship Id="rId1" Type="{REL_OFFICE_DOCUMENT}" Target="word/document.xml"/><Relationship Id="rId2" Type="{REL_APP}" Target="docProps/app.xml"/></Relationships>"#)),
        ("docProps/app.xml", r#"<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>REFERENCE APP</Application></Properties>"#.into()),
        ("word/document.xml", format!(r#"<w:document xmlns:w="{NS_W}" xmlns:r="{NS_R}"><w:body><w:p><w:pPr><w:sectPr><w:pgSz w:w="12240" w:h="15840"/></w:sectPr></w:pPr><w:r><w:t xml:space="preserve">REFERENCE BODY</w:t></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rId100"/><w:pgSz w:w="8391" w:h="11906"/><w:pgMar w:top="1134" w:right="1134" w:bottom="1134" w:left="1134" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#)),
        ("word/_rels/document.xml.rels", format!(r#"<Relationships xmlns="{NS_RELS}">{rels}</Relationships>"#)),
        ("word/styles.xml", reference_styles()),
        ("word/numbering.xml", reference_numbering()),
        ("word/settings.xml", reference_settings()),
        ("word/fontTable.xml", format!(r#"<w:fonts xmlns:w="{NS_W}"><w:font w:name="Reference Font"/></w:fonts>"#)),
        ("word/header1.xml", HEADER.into()),
        ("word/theme/theme1.xml", format!(r#"<a:theme xmlns:a="{NS_A}" name="Reference Theme"><a:themeElements/></a:theme>"#)),
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
    zip.finish().unwrap().into_inner()
}

fn reference_styles() -> String {
    format!(
        r#"<w:styles xmlns:w="{NS_W}"><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:rPr><w:color w:val="FF0000"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Callout"><w:name w:val="Callout"/><w:basedOn w:val="Normal"/><w:pPr><w:shd w:val="clear" w:fill="F0F0FF"/></w:pPr></w:style><w:style w:type="table" w:styleId="Table"><w:name w:val="Table"/><w:basedOn w:val="TableNormal"/></w:style></w:styles>"#
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
    let document: Document = serde_json::from_value(serde_json::json!({
        "meta": { "title": "Reference preservation" },
        "blocks": [{
            "type": "heading", "level": 1, "number": "1", "attrs": {},
            "inlines": [{ "type": "text", "text": "One heading" }]
        }, {
            "type": "paragraph",
            "inlines": [{ "type": "text", "text": "One paragraph" }]
        }],
        "features": {}, "assets": [],
        "source": { "path": "/document.md", "kind": "markdown", "base_dir": "/" }
    })).unwrap();
    let config = Config::default();
    let output = docx::write(
        &document,
        &AssetRegistry::new(),
        &mut NoRenderer,
        &DocxOptions {
            config: &config,
            reference: Some(reference),
            source_date_epoch: None,
            all_styles: false,
        },
    )
    .unwrap();
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
