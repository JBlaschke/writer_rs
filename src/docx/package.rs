//! DOCX ZIP package and document properties (docx-output.md §1).
//! Owner: T04. Stub created by T00.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::io::{Cursor, Write};

use time::{
    OffsetDateTime, UtcOffset, format_description::well_known::Rfc3339,
};
use zip::{
    CompressionMethod, DateTime, System, ZipWriter, write::SimpleFileOptions,
};

use crate::error::{Error, Result};
use crate::ir::DocMeta;

use super::Part;
use super::xml::{
    CT_APP, CT_CORE, CT_RELS, NS_RELS, REL_APP, REL_CORE, REL_FOOTER,
    REL_FOOTNOTES, REL_HYPERLINK, REL_IMAGE, REL_NUMBERING,
    REL_OFFICE_DOCUMENT, REL_SETTINGS, REL_STYLES, XML_HEAD, escape,
    single_line,
};

/// A relationship retained from a reference document.
#[derive(Debug, Clone)]
pub(crate) struct Relationship {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) target: String,
    pub(crate) external: bool,
}

#[derive(Debug, Default)]
struct Rels {
    entries: Vec<Relationship>,
    next: u64,
}

impl Rels {
    fn new(entries: Vec<Relationship>) -> Self {
        let next = entries
            .iter()
            .filter_map(|rel| {
                let suffix = rel.id.strip_prefix("rId")?;
                (!suffix.is_empty()
                    && suffix.bytes().all(|b| b.is_ascii_digit()))
                .then(|| suffix.parse::<u64>().ok())
                .flatten()
            })
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        Self { entries, next }
    }

    fn add(&mut self, kind: &str, target: &str, external: bool) -> String {
        if let Some(rel) = self.entries.iter().find(|rel| {
            rel.kind == kind && rel.target == target && rel.external == external
        }) {
            return rel.id.clone();
        }
        let id = format!("rId{}", self.next);
        self.next += 1;
        self.entries.push(Relationship {
            id: id.clone(),
            kind: kind.into(),
            target: target.into(),
            external,
        });
        id
    }

    fn to_xml(&self) -> String {
        let mut xml = format!(r#"{XML_HEAD}<Relationships xmlns="{NS_RELS}">"#);
        for rel in &self.entries {
            let mode = if rel.external {
                r#" TargetMode="External""#
            } else {
                ""
            };
            let _ = write!(
                xml,
                r#"<Relationship Id="{}" Type="{}" Target="{}"{mode}/>"#,
                escape(&rel.id),
                escape(&rel.kind),
                escape(&rel.target),
            );
        }
        xml.push_str("</Relationships>");
        xml
    }
}

/// One media part, indexed in first-use order as in presenter's `Media`.
#[derive(Debug)]
struct Image {
    name: String,
    bytes: Vec<u8>,
    content_type: &'static str,
}

#[derive(Debug, Default)]
struct Media {
    numbers: HashMap<String, usize>,
    parts: Vec<Image>,
    next: usize,
}

impl Media {
    fn number_of(
        &mut self, key: &str, ext: &str, bytes: Vec<u8>,
        copied: &BTreeMap<String, (Vec<u8>, String)>,
    ) -> usize {
        if let Some(index) = self.numbers.get(key) {
            return *index;
        }
        self.next += 1;
        while copied.contains_key(&format!("word/media/w{}.{ext}", self.next)) {
            self.next += 1;
        }
        self.parts.push(Image {
            name: format!("w{}.{ext}", self.next),
            bytes,
            content_type: match ext {
                "png" => "image/png",
                "jpeg" => "image/jpeg",
                "svg" => "image/svg+xml",
                _ => unreachable!("DOCX media is PNG, JPEG or SVG"),
            },
        });
        let index = self.parts.len() - 1;
        self.numbers.insert(key.into(), index);
        index
    }
}

/// Generated and copied OPC parts, relationships and first-use media.
#[derive(Debug)]
pub struct Package {
    parts: BTreeMap<String, (Vec<u8>, String)>,
    copied: BTreeMap<String, (Vec<u8>, String)>,
    defaults: BTreeMap<String, String>,
    doc_rels: Rels,
    note_rels: Rels,
    media: Media,
    footer: Option<(String, String)>,
}

impl Package {
    /// Reserves generated relationships before content is emitted.
    pub(crate) fn new(
        kept_rels: Vec<Relationship>,
        copied: BTreeMap<String, (Vec<u8>, String)>, has_footnotes: bool,
        builtin_footer: bool,
    ) -> Self {
        let mut doc_rels = Rels::new(kept_rels);
        doc_rels.add(REL_STYLES, "styles.xml", false);
        doc_rels.add(REL_NUMBERING, "numbering.xml", false);
        doc_rels.add(REL_SETTINGS, "settings.xml", false);
        if has_footnotes {
            doc_rels.add(REL_FOOTNOTES, "footnotes.xml", false);
        }
        let footer = builtin_footer.then(|| {
            let mut number = 1;
            while copied.contains_key(&format!("word/footer{number}.xml")) {
                number += 1;
            }
            let target = format!("footer{number}.xml");
            let id = doc_rels.add(REL_FOOTER, &target, false);
            (format!("word/{target}"), id)
        });
        Self {
            parts: BTreeMap::new(),
            copied,
            defaults: BTreeMap::new(),
            doc_rels,
            note_rels: Rels::new(Vec::new()),
            media: Media::default(),
            footer,
        }
    }

    /// Returns the built-in footer's part path and document relationship id.
    pub(crate) fn footer(&self) -> Option<(&str, &str)> {
        self.footer.as_ref().map(|(path, id)| (path.as_str(), id.as_str()))
    }

    /// Retains content type Defaults used by copied reference parts.
    pub(crate) fn set_defaults(&mut self, defaults: BTreeMap<String, String>) {
        self.defaults = defaults;
    }

    /// Adds or replaces a generated part.
    pub(crate) fn insert(
        &mut self, path: impl Into<String>, bytes: impl Into<Vec<u8>>,
        content_type: impl Into<String>,
    ) {
        self.parts.insert(path.into(), (bytes.into(), content_type.into()));
    }

    fn rels(&mut self, part: Part) -> &mut Rels {
        match part {
            Part::Document => &mut self.doc_rels,
            Part::Footnotes => &mut self.note_rels,
        }
    }

    /// Returns one external hyperlink relationship per target and part.
    pub fn hyperlink_rel(&mut self, part: Part, url: &str) -> String {
        self.rels(part).add(REL_HYPERLINK, url, true)
    }

    /// Adds media once per caller deduplication key; returns its rel and name.
    ///
    /// `ext` is `png`, `jpeg` or `svg`; the key includes a rendition variant
    /// when an asset has both an SVG and a PNG fallback.
    pub fn image_rel(
        &mut self, part: Part, key: &str, ext: &str, bytes: Vec<u8>,
    ) -> (String, String) {
        let index = self.media.number_of(key, ext, bytes, &self.copied);
        let name = self.media.parts[index].name.clone();
        let id =
            self.rels(part).add(REL_IMAGE, &format!("media/{name}"), false);
        (id, name)
    }

    /// Completes the metadata and relationships and writes the deterministic ZIP.
    pub(crate) fn finish(
        mut self, meta: &DocMeta, effective_lang: Option<&str>,
        timestamp: Option<&str>,
    ) -> Result<Vec<u8>> {
        self.insert(
            "docProps/core.xml",
            core_properties(meta, effective_lang, timestamp),
            CT_CORE,
        );
        self.insert("docProps/app.xml", app_properties(), CT_APP);
        self.insert("_rels/.rels", package_rels(), CT_RELS);
        self.insert(
            "word/_rels/document.xml.rels",
            self.doc_rels.to_xml(),
            CT_RELS,
        );
        if !self.note_rels.entries.is_empty() {
            self.insert(
                "word/_rels/footnotes.xml.rels",
                self.note_rels.to_xml(),
                CT_RELS,
            );
        }
        let content_types = self.content_types();
        let mut entries =
            vec![("[Content_Types].xml".into(), content_types.into_bytes())];
        for path in [
            "_rels/.rels", "docProps/core.xml", "docProps/app.xml",
            "word/document.xml", "word/_rels/document.xml.rels",
            "word/styles.xml", "word/numbering.xml", "word/settings.xml",
            "word/footnotes.xml", "word/_rels/footnotes.xml.rels",
        ] {
            if let Some((bytes, _)) = self.parts.remove(path) {
                entries.push((path.into(), bytes));
            }
        }
        if let Some((path, _)) = self.footer
            && let Some((bytes, _)) = self.parts.remove(&path)
        {
            entries.push((path, bytes));
        }
        for (path, (bytes, _)) in self.copied.into_iter().chain(self.parts) {
            entries.push((path, bytes));
        }
        for image in self.media.parts {
            entries.push((format!("word/media/{}", image.name), image.bytes));
        }
        zip_parts(&entries)
    }

    fn content_types(&self) -> String {
        let mut defaults = self.defaults.clone();
        defaults.insert("rels".into(), CT_RELS.into());
        defaults.insert("xml".into(), "application/xml".into());
        for image in &self.media.parts {
            let extension = image.name.rsplit('.').next().unwrap_or("");
            defaults.insert(extension.into(), image.content_type.into());
        }
        let mut overrides = BTreeMap::new();
        for (path, (_, content_type)) in self.parts.iter().chain(&self.copied) {
            if path.ends_with(".rels") {
                continue;
            }
            let extension = path.rsplit('.').next().unwrap_or("");
            let copied_default = self.copied.contains_key(path)
                && defaults.get(extension) == Some(content_type);
            if !copied_default {
                overrides.insert(part_uri(path), content_type.clone());
            }
        }
        let mut xml = format!(
            r#"{XML_HEAD}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#
        );
        for (extension, content_type) in defaults {
            let _ = write!(
                xml,
                r#"<Default Extension="{}" ContentType="{}"/>"#,
                escape(&extension),
                escape(&content_type),
            );
        }
        for (path, content_type) in overrides {
            let _ = write!(
                xml,
                r#"<Override PartName="{}" ContentType="{}"/>"#,
                escape(&path),
                escape(&content_type),
            );
        }
        xml.push_str("</Types>");
        xml
    }
}

// OPC part names are URI paths; ZIP member names contain decoded UTF-8 bytes.
fn part_uri(path: &str) -> String {
    let mut uri = String::from("/");
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@/".contains(&byte)
        {
            uri.push(char::from(byte));
        } else {
            let _ = write!(uri, "%{byte:02X}");
        }
    }
    uri
}

/// ZIPs ordered parts with writer's reproducible OPC options.
pub(crate) fn zip_parts(entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let deflated = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(6))
        .last_modified_time(DateTime::default())
        .system(System::Unix)
        .unix_permissions(0o644);
    let stored = deflated
        .compression_method(CompressionMethod::Stored)
        .compression_level(None);
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (path, bytes) in entries {
        let options = if path.starts_with("word/media/")
            && (path.ends_with(".png")
                || path.ends_with(".jpeg")
                || path.ends_with(".jpg"))
        {
            stored
        } else {
            deflated
        };
        zip.start_file(path, options).map_err(std::io::Error::from)?;
        zip.write_all(bytes)?;
    }
    Ok(zip.finish().map_err(std::io::Error::from)?.into_inner())
}

fn package_rels() -> String {
    format!(
        r#"{XML_HEAD}<Relationships xmlns="{NS_RELS}"><Relationship Id="rId1" Type="{REL_OFFICE_DOCUMENT}" Target="word/document.xml"/><Relationship Id="rId2" Type="{REL_CORE}" Target="docProps/core.xml"/><Relationship Id="rId3" Type="{REL_APP}" Target="docProps/app.xml"/></Relationships>"#
    )
}

fn core_properties(
    meta: &DocMeta, effective_lang: Option<&str>, timestamp: Option<&str>,
) -> String {
    let mut xml = format!(
        r#"{XML_HEAD}<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:dcmitype="http://purl.org/dc/dcmitype/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">"#
    );
    let authors = meta
        .authors
        .iter()
        .map(|a| a.name.as_str())
        .collect::<Vec<_>>()
        .join("; ");
    let keywords = meta.keywords.join(", ");
    for (name, value) in [
        ("dc:title", meta.title.as_deref()),
        ("dc:creator", Some(authors.as_str())),
        ("cp:keywords", Some(keywords.as_str())),
        ("dc:description", meta.description.as_deref()),
        ("dc:language", effective_lang),
    ] {
        if let Some(value) = value {
            let value = single_line(value);
            if !value.is_empty() {
                let _ = write!(xml, "<{name}>{}</{name}>", escape(&value));
            }
        }
    }
    if let Some(timestamp) = timestamp {
        for name in ["dcterms:created", "dcterms:modified"] {
            let _ = write!(
                xml,
                r#"<{name} xsi:type="dcterms:W3CDTF">{timestamp}</{name}>"#,
            );
        }
    }
    xml.push_str("</cp:coreProperties>");
    xml
}

fn app_properties() -> String {
    format!(
        r#"{XML_HEAD}<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"><Application>writer</Application></Properties>"#
    )
}

/// Computes a reproducible UTC timestamp, omitting it when no valid source exists.
///
/// An explicit `[pdf] timestamp` must be RFC 3339. Otherwise a valid document
/// date wins over an unsigned `SOURCE_DATE_EPOCH`; malformed fallback values
/// are ignored. Fractions are dropped after UTC conversion.
pub fn core_timestamp(
    pdf_timestamp: &str, meta_date: Option<&str>,
    source_date_epoch: Option<&str>,
) -> Result<Option<String>> {
    if !pdf_timestamp.is_empty() {
        let value = OffsetDateTime::parse(pdf_timestamp, &Rfc3339).map_err(
            |error| {
                Error::Config(format!(
                    "invalid [pdf] timestamp {pdf_timestamp:?}: {error}"
                ))
            },
        )?;
        let utc = value.checked_to_offset(UtcOffset::UTC).ok_or_else(|| {
            Error::Config(
                "[pdf] timestamp is outside the UTC date range".into(),
            )
        })?;
        if !(0..=9999).contains(&utc.year()) {
            return Err(Error::Config(
                "[pdf] timestamp is outside the DOCX date range".into(),
            ));
        }
        return Ok(Some(format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            utc.year(),
            u8::from(utc.month()),
            utc.day(),
            utc.hour(),
            utc.minute(),
            utc.second(),
        )));
    }
    if let Some(date) = meta_date.filter(|date| valid_date(date)) {
        return Ok(Some(format!("{date}T00:00:00Z")));
    }
    let Some(epoch) = source_date_epoch
        .filter(|epoch| {
            !epoch.is_empty() && epoch.bytes().all(|b| b.is_ascii_digit())
        })
        .and_then(|epoch| epoch.parse::<i64>().ok())
    else {
        return Ok(None);
    };
    let (year, month, day) = civil_from_days(epoch / 86_400);
    if !(0..=9999).contains(&year) {
        return Ok(None);
    }
    let seconds = epoch % 86_400;
    Ok(Some(format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60,
    )))
}

fn valid_date(date: &str) -> bool {
    let b = date.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !b
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    {
        return false;
    }
    let year = date[..4].parse::<i64>().unwrap_or(0);
    let month = date[5..7].parse::<u32>().unwrap_or(0);
    let day = date[8..].parse::<u32>().unwrap_or(0);
    let days = match month {
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => 0,
    };
    day >= 1 && day <= days
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

#[cfg(test)]
mod tests {
    use super::super::xml::{CT_DOCUMENT, CT_STYLES};
    use super::*;
    use zip::ZipArchive;

    fn package() -> Package {
        Package::new(Vec::new(), BTreeMap::new(), false, false)
    }

    #[test]
    fn core_timestamp_policy() {
        for (explicit, date, epoch, expected) in [
            ("", None, Some("0"), Some("1970-01-01T00:00:00Z")),
            ("", None, None, None),
            (
                "2026-10-25T12:00:00.5+02:00",
                None,
                None,
                Some("2026-10-25T10:00:00Z"),
            ),
            ("", Some("2026-10-24"), Some("5"), Some("2026-10-24T00:00:00Z")),
            (
                "",
                Some("October 2026"),
                Some("86400"),
                Some("1970-01-02T00:00:00Z"),
            ),
            ("", None, Some("12a"), None),
            ("", None, Some("-1"), None),
            ("", None, Some("9223372036854775808"), None),
            ("", None, Some("253402300800"), None),
            ("", None, Some("253402300799"), Some("9999-12-31T23:59:59Z")),
            ("", Some("2000-02-29"), None, Some("2000-02-29T00:00:00Z")),
            (
                "",
                Some("1900-02-29"),
                Some("951782400"),
                Some("2000-02-29T00:00:00Z"),
            ),
        ] {
            assert_eq!(
                core_timestamp(explicit, date, epoch).unwrap().as_deref(),
                expected,
                "{explicit:?}, {date:?}, {epoch:?}"
            );
        }
        assert!(matches!(core_timestamp("yesterday", None, None),
            Err(Error::Config(message)) if message.starts_with("invalid [pdf] timestamp")));
    }

    #[test]
    fn explicit_timestamp_requires_a_four_digit_utc_year() {
        assert!(matches!(
            core_timestamp("0000-01-01T00:00:00+01:00", None, None),
            Err(Error::Config(message))
                if message == "[pdf] timestamp is outside the DOCX date range"
        ));
    }

    #[test]
    fn content_types_sorted() {
        let mut package = package();
        package.insert("word/styles.xml", "styles", CT_STYLES);
        package.insert("word/document.xml", "document", CT_DOCUMENT);
        package.image_rel(Part::Document, "svg", "svg", b"<svg/>".to_vec());
        package.image_rel(Part::Document, "png", "png", b"\x89PNG".to_vec());
        let xml = package.content_types();
        let defaults: Vec<_> = xml
            .match_indices("<Default ")
            .map(|(i, _)| &xml[i..xml[i..].find("/>").unwrap() + i + 2])
            .collect();
        assert_eq!(defaults.len(), 4);
        assert!(defaults[0].contains("Extension=\"png\""));
        assert!(defaults[1].contains("Extension=\"rels\""));
        assert!(defaults[2].contains("Extension=\"svg\""));
        assert!(defaults[3].contains("Extension=\"xml\""));
        assert!(
            xml.find("/word/document.xml").unwrap()
                < xml.find("/word/styles.xml").unwrap()
        );
        assert!(
            xml.rfind("<Default ").unwrap() < xml.find("<Override ").unwrap()
        );
        assert!(xml.starts_with(&format!("{XML_HEAD}<Types")));
    }

    #[test]
    fn copied_content_types_survive_writer_defaults() {
        let copied = [
            (
                "word/fonts/font.odttf".into(),
                (vec![1], "font/reference".into()),
            ),
            (
                "word/header space.xml".into(),
                (b"<header/>".to_vec(), "header/reference".into()),
            ),
        ]
        .into_iter()
        .collect();
        let mut package = Package::new(Vec::new(), copied, false, false);
        package.set_defaults(
            [
                ("odttf".into(), "font/reference".into()),
                ("xml".into(), "header/reference".into()),
            ]
            .into_iter()
            .collect(),
        );
        let xml = package.content_types();
        assert!(xml.contains(
            r#"<Default Extension="odttf" ContentType="font/reference"/>"#
        ));
        assert!(xml.contains(
            r#"<Default Extension="xml" ContentType="application/xml"/>"#
        ));
        assert!(xml.contains(r#"<Override PartName="/word/header%20space.xml" ContentType="header/reference"/>"#));
        assert!(!xml.contains(r#"PartName="/word/fonts/font.odttf""#));
    }

    #[test]
    fn rel_ids_follow_kept_max() {
        let kept = ["rId7", "rId12"]
            .map(|id| Relationship {
                id: id.into(),
                kind: "header".into(),
                target: format!("{id}.xml"),
                external: false,
            })
            .to_vec();
        let mut package = Package::new(kept, BTreeMap::new(), true, true);
        assert_eq!(package.doc_rels.entries[2].id, "rId13");
        assert_eq!(package.doc_rels.entries[2].kind, REL_STYLES);
        assert_eq!(package.doc_rels.entries[5].kind, REL_FOOTNOTES);
        assert_eq!(package.footer(), Some(("word/footer1.xml", "rId17")));
        assert_eq!(
            package.hyperlink_rel(Part::Document, "https://example.com/?a&b"),
            "rId18"
        );
        assert_eq!(
            package.hyperlink_rel(Part::Document, "https://example.com/?a&b"),
            "rId18"
        );
        assert_eq!(
            package.hyperlink_rel(Part::Footnotes, "https://example.com/?a&b"),
            "rId1"
        );
        assert!(package.doc_rels.to_xml().contains("a&amp;b"));
        assert_eq!(package.doc_rels.entries.len(), 8);
        assert!(package.doc_rels.to_xml().contains("TargetMode=\"External\""));
    }

    #[test]
    fn media_dedup_and_names() {
        let mut package = package();
        let (first, name) =
            package.image_rel(Part::Document, "plot:png", "png", vec![1]);
        assert_eq!(name, "w1.png");
        assert_eq!(
            package.image_rel(Part::Document, "plot:png", "png", vec![2]),
            (first, name.clone())
        );
        let (note, note_name) =
            package.image_rel(Part::Footnotes, "plot:png", "png", vec![3]);
        assert_eq!(note, "rId1");
        assert_eq!(note_name, name);
        assert_eq!(
            package.image_rel(Part::Document, "plot:svg", "svg", vec![4]).1,
            "w2.svg"
        );
        assert_eq!(package.media.parts.len(), 2);
        assert_eq!(package.media.parts[0].bytes, [1]);
    }

    #[test]
    fn zip_entries() {
        let mut package = Package::new(Vec::new(), BTreeMap::new(), true, true);
        for (path, content_type) in [
            ("word/document.xml", CT_DOCUMENT),
            ("word/styles.xml", CT_STYLES),
            ("word/numbering.xml", "numbering"),
            ("word/settings.xml", "settings"),
            ("word/footnotes.xml", "footnotes"),
            ("word/footer1.xml", "footer"),
        ] {
            package.insert(path, "<xml/>", content_type);
        }
        package.image_rel(Part::Footnotes, "png", "png", b"\x89PNG".to_vec());
        package.image_rel(Part::Document, "svg", "svg", b"<svg/>".to_vec());
        let bytes = package.finish(&DocMeta::default(), None, None).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(&bytes)).unwrap();
        let mut names = Vec::new();
        for index in 0..archive.len() {
            let entry = archive.by_index(index).unwrap();
            let name = entry.name().unwrap().into_owned();
            let png = name.ends_with(".png");
            assert!(!entry.is_dir());
            assert_eq!(entry.system(), System::Unix);
            assert_eq!(entry.unix_mode(), Some(0o100644));
            assert_eq!(entry.last_modified(), Some(DateTime::default()));
            assert_eq!(
                entry.compression(),
                if png {
                    CompressionMethod::Stored
                } else {
                    CompressionMethod::Deflated
                }
            );
            assert_eq!(
                entry.version_made_by(),
                if png { (1, 0) } else { (2, 0) }
            );
            names.push(name);
        }
        assert_eq!(
            names,
            [
                "[Content_Types].xml", "_rels/.rels", "docProps/core.xml",
                "docProps/app.xml", "word/document.xml",
                "word/_rels/document.xml.rels", "word/styles.xml",
                "word/numbering.xml", "word/settings.xml",
                "word/footnotes.xml", "word/_rels/footnotes.xml.rels",
                "word/footer1.xml", "word/media/w1.png", "word/media/w2.svg",
            ]
        );
    }

    #[test]
    fn core_properties_keep_order_and_omit_empty_values() {
        let meta = DocMeta {
            title: Some("A\n B & C".into()),
            authors: vec![
                crate::ir::Author {
                    name: "First".into(),
                    ..Default::default()
                },
                crate::ir::Author {
                    name: "Second".into(),
                    ..Default::default()
                },
            ],
            keywords: vec!["one".into(), "two".into()],
            description: Some("\n".into()),
            ..Default::default()
        };
        let xml =
            core_properties(&meta, Some("de-CH"), Some("1970-01-01T00:00:00Z"));
        assert!(xml.contains("<dc:title>A B &amp; C</dc:title>"));
        assert!(xml.contains("<dc:creator>First; Second</dc:creator>"));
        assert!(xml.contains("<cp:keywords>one, two</cp:keywords>"));
        assert!(!xml.contains("dc:description"));
        let mut previous = 0;
        for name in [
            "<dc:title>", "<dc:creator>", "<cp:keywords>", "<dc:language>",
            "<dcterms:created", "<dcterms:modified",
        ] {
            let position = xml.find(name).unwrap();
            assert!(position > previous);
            previous = position;
        }
    }
}
