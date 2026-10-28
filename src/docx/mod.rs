//! DOCX package construction and OOXML output (docx-output.md §1).
//! Owner: T04. Stub created by T00.

pub mod body;
pub mod custom_style;
pub mod fields;
pub mod footnotes;
pub mod images;
pub mod inline;
pub mod math;
pub mod numbering;
pub mod package;
pub mod reference;
pub mod styles;
pub mod tables;
pub mod toc;
pub mod validate;

use std::path::Path;

use crate::assets::registry::AssetRegistry;
use crate::config::Config;
use crate::error::{Diagnostics, Error, Result};
use crate::ir::{Document, MathDialect};
use crate::resolve::LangTag;

pub use numbering::Numbering;
pub use package::{Package, core_timestamp};
pub use styles::{StyleKind, StyleTable};
pub use validate::{ValidateOptions, Violation, validate};

/// A rendered picture and its dimensions in typographic points.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    /// PNG fallback bytes, rendered at two pixels per point.
    pub png: Vec<u8>,
    /// Optional vector image bytes.
    pub svg: Option<Vec<u8>>,
    /// Width in typographic points.
    pub width_pt: f64,
    /// Height in typographic points.
    pub height_pt: f64,
    /// Distance from the top edge to the baseline.
    pub baseline_pt: f64,
}

/// Implemented over `typst::render::{equation, image}` and
/// `typst::html::equation_mathml` by `pipeline::docx::WorldRenderer<'a>` (T41).
/// It borrows the session's World and never creates one; tests use a fake.
pub trait Renderer {
    /// `source` as the Typst emitter writes it: LaTeX is prepared by
    /// `latex_source::for_typst(src, &macros).0` (math.md §3.4).
    fn equation(
        &mut self, source: &str, dialect: MathDialect, display: bool,
    ) -> Result<Picture>;
    /// `Some(w)` renders at that display width; `None` uses the natural size.
    fn image(
        &mut self, bytes: &[u8], mime: &str, width_pt: Option<f64>,
    ) -> Result<Picture>;
    /// Exports a Typst equation to MathML.
    fn typst_mathml(&mut self, source: &str, display: bool) -> Result<String>;
}

/// Options for deterministic DOCX emission.
pub struct DocxOptions<'a> {
    /// Effective document configuration.
    pub config: &'a Config,
    /// Absolute reference path, resolved by pipeline preparation.
    pub reference: Option<&'a Path>,
    /// Environment value read once by the pipeline.
    pub source_date_epoch: Option<String>,
    /// Include the complete vocabulary, used when ejecting a reference file.
    pub all_styles: bool,
}

/// A generated DOCX package and any content diagnostics.
#[derive(Debug)]
pub struct DocxOutput {
    /// The complete ZIP package.
    pub bytes: Vec<u8>,
    /// Findings from emission and style resolution.
    pub diagnostics: Diagnostics,
}

/// Shared state extended by the body, inline, and specialised emitters.
/// `lang` is the effective language, falling back to English; `part` selects
/// whether generated XML goes to the document body or footnotes.
pub struct Emitter<'a> {
    /// Resolved input document.
    pub doc: &'a Document,
    /// Effective configuration.
    pub config: &'a Config,
    /// Registered input assets.
    pub assets: &'a AssetRegistry,
    /// Session renderer, borrowed from the pipeline.
    pub renderer: &'a mut dyn Renderer,
    /// Name-keyed styles and their definitions.
    pub styles: StyleTable,
    /// List numbering definitions and instances.
    pub numbering: Numbering,
    /// Section XML and available content dimensions.
    pub layout: PageLayout,
    /// Parts, relationships, and media being assembled.
    pub package: Package,
    /// Next identifiers in each document-wide namespace.
    pub ids: Ids,
    /// Findings from content emission.
    pub diagnostics: Diagnostics,
    /// The current paragraph-style context.
    pub containers: Vec<Container>,
    /// Part receiving generated runs and paragraphs.
    pub part: Part,
    /// Effective document language and region.
    pub lang: LangTag,
    /// XML children of the document body.
    pub body: String,
    /// Footnote elements, without their enclosing root or separators.
    pub notes: String,
    pub(crate) content: footnotes::ContentState<'a>,
    pub(crate) fields: fields::FieldState,
    pub(crate) math_state: math::State,
    pub(crate) table_width_twips: Option<u32>,
    pub(crate) deferred_error: Option<Error>,
}

/// Section properties and usable content area, in twentieths of a point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageLayout {
    /// Final body-level section properties.
    pub sect_pr: String,
    /// Width available for text and tables.
    pub text_width_twips: u32,
    /// Height available for text.
    pub text_height_twips: u32,
}

/// Next identifiers; each starts at one or above copied reference identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ids {
    /// Next bookmark identifier.
    pub bookmark: u32,
    /// Next drawing property identifier.
    pub doc_pr: u32,
    /// Next positive footnote identifier.
    pub footnote: u32,
}

impl Default for Ids {
    fn default() -> Self {
        Self { bookmark: 1, doc_pr: 1, footnote: 1 }
    }
}

/// Part whose relationships and XML are currently being emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// Main document part.
    Document,
    /// Footnotes part.
    Footnotes,
}

/// Paragraph style context shared by the content emitters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Container {
    /// Resolved paragraph style identifier.
    Style(String),
    /// Compact list item.
    TightList,
    /// Table cell.
    TableCell,
    /// Notebook cell output.
    CellOutput,
    /// Footnote body.
    Footnote,
}

impl Emitter<'_> {
    pub(crate) fn available_width_twips(&self) -> u32 {
        self.table_width_twips.unwrap_or(self.layout.text_width_twips).max(1)
    }

    fn buffer(&mut self) -> &mut String {
        match self.part {
            Part::Document => &mut self.body,
            Part::Footnotes => &mut self.notes,
        }
    }

    pub(crate) fn text_run(&mut self, text: &str) {
        self.buffer().push_str(&format!(
            "<w:r><w:t xml:space=\"preserve\">{}</w:t></w:r>",
            xml::escape(text),
        ));
    }

    pub(crate) fn text_paragraph(&mut self, text: &str, style: &str) {
        let style = self.styles.ensure(style, StyleKind::Paragraph);
        let props = xml::ParaProps { style: Some(style), ..Default::default() };
        self.start_paragraph(props);
        self.text_run(text);
        self.end_paragraph();
    }
}

/// Builds a deterministic DOCX package.
///
/// Content limitations become diagnostics. Errors are an invalid reference
/// document, an unknown asset id, an invalid explicit timestamp, or I/O. ZIP
/// writing errors are converted to [`crate::error::Error::Io`].
pub fn write(
    doc: &Document, assets: &AssetRegistry, renderer: &mut dyn Renderer,
    opts: &DocxOptions<'_>,
) -> Result<DocxOutput> {
    let timestamp = core_timestamp(
        &opts.config.pdf.timestamp,
        doc.meta.date.as_deref(),
        opts.source_date_epoch.as_deref(),
    )?;
    let reference =
        opts.reference.map(reference::Reference::load).transpose()?;
    let has_notes = preflight(doc, assets)?;
    let language = document_language(doc, opts.config);
    let mut emitter = prepare_emitter(
        doc,
        assets,
        renderer,
        opts,
        reference.as_ref(),
        language,
        has_notes,
    )?;
    let footer = emitter.package.footer().map(|(path, _)| path.to_owned());
    emitter.document();
    emitter.finish_footnotes();
    if let Some(error) = emitter.deferred_error.take() {
        return Err(error);
    }
    if opts.all_styles {
        for &(name, kind) in styles::VOCABULARY {
            emitter.styles.ensure(name, kind);
        }
    }
    if let Some(path) = footer {
        let style = emitter.styles.ensure("footer", StyleKind::Paragraph);
        emitter.package.insert(
            path,
            reference::footer_xml(&style).into_bytes(),
            xml::CT_FOOTER,
        );
    }
    finish_parts(&mut emitter, reference.as_ref(), has_notes)?;
    emitter.diagnostics.extend(emitter.styles.take_diagnostics());
    let bytes = emitter.package.finish(
        &doc.meta,
        Some(language),
        timestamp.as_deref(),
    )?;
    Ok(DocxOutput { bytes, diagnostics: emitter.diagnostics })
}

fn document_language<'a>(doc: &'a Document, config: &'a Config) -> &'a str {
    doc.meta
        .lang
        .as_deref()
        .filter(|lang| !lang.is_empty())
        .or_else(|| {
            (!config.document.lang.is_empty())
                .then_some(config.document.lang.as_str())
        })
        .unwrap_or("en")
}

fn prepare_emitter<'a>(
    doc: &'a Document, assets: &'a AssetRegistry,
    renderer: &'a mut dyn Renderer, opts: &DocxOptions<'a>,
    reference: Option<&reference::Reference>, language: &str, has_notes: bool,
) -> Result<Emitter<'a>> {
    let mut styles = match reference {
        Some(reference) => reference_result(
            StyleTable::from_reference(&reference.styles_xml),
            opts.reference,
        )?,
        None => StyleTable::builtin(),
    };
    styles.set_lang(language);
    let numbering = reference_result(
        Numbering::from_reference(
            reference.and_then(|r| r.numbering_xml.as_deref()),
        ),
        opts.reference,
    )?;
    let (kept, copied) = reference
        .map(|r| (r.kept_rels.clone(), r.copied_parts.clone()))
        .unwrap_or_default();
    let builtin_footer = reference.is_none_or(|r| r.sect_pr.is_none());
    let mut package = Package::new(kept, copied, has_notes, builtin_footer);
    if let Some(reference) = reference {
        package.set_defaults(reference.copied_defaults.clone());
    }
    let layout = reference::page_layout(
        reference,
        &opts.config.pdf.paper,
        package.footer().map(|(_, id)| id),
    );
    let lang = LangTag::effective(&doc.meta, opts.config)
        .unwrap_or_else(|| LangTag { lang: "en".into(), region: None });
    let mut ids = reference.map_or_else(Ids::default, |r| r.ids);
    let content = footnotes::ContentState::new(doc, &mut ids);
    Ok(Emitter {
        doc,
        config: opts.config,
        assets,
        renderer,
        styles,
        numbering,
        layout,
        package,
        ids,
        diagnostics: Diagnostics::new(),
        containers: Vec::new(),
        part: Part::Document,
        lang,
        body: String::new(),
        notes: String::new(),
        content,
        fields: fields::FieldState::default(),
        math_state: math::State::new(opts.config),
        table_width_twips: None,
        deferred_error: None,
    })
}

fn reference_result<T>(result: Result<T>, path: Option<&Path>) -> Result<T> {
    result.map_err(|error| match path {
        Some(path) => crate::error::Error::Resource {
            what: "reference document",
            path: path.to_owned(),
            message: error.to_string(),
        },
        None => error,
    })
}

fn finish_parts(
    emitter: &mut Emitter<'_>, reference: Option<&reference::Reference>,
    has_notes: bool,
) -> Result<()> {
    let namespaces = document_namespaces(reference);
    let document = format!(
        "{}<w:document{namespaces}><w:body>{}{}</w:body></w:document>",
        xml::XML_HEAD,
        emitter.body,
        emitter.layout.sect_pr,
    );
    emitter.package.insert(
        "word/document.xml",
        document.into_bytes(),
        xml::CT_DOCUMENT,
    );
    emitter.package.insert(
        "word/styles.xml",
        emitter.styles.to_xml().into_bytes(),
        xml::CT_STYLES,
    );
    emitter.package.insert(
        "word/numbering.xml",
        emitter.numbering.to_xml().into_bytes(),
        xml::CT_NUMBERING,
    );
    let settings = reference::settings_xml(
        reference.and_then(|r| r.settings_xml.as_deref()),
        emitter.config.docx.update_fields,
    )?;
    emitter.package.insert(
        "word/settings.xml",
        settings.into_bytes(),
        xml::CT_SETTINGS,
    );
    if has_notes || !emitter.notes.is_empty() {
        let notes = format!(
            "{}<w:footnotes{namespaces}><w:footnote w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>{}</w:footnotes>",
            xml::XML_HEAD,
            emitter.notes,
        );
        emitter.package.insert(
            "word/footnotes.xml",
            notes.into_bytes(),
            xml::CT_FOOTNOTES,
        );
    }
    Ok(())
}

fn document_namespaces(reference: Option<&reference::Reference>) -> String {
    let mut bindings =
        reference.map(|r| r.document_namespaces.clone()).unwrap_or_default();
    for (prefix, uri) in [
        ("w", xml::NS_W),
        ("m", xml::NS_M),
        ("r", xml::NS_R),
        ("o", xml::NS_O),
        ("v", xml::NS_V),
        ("w10", xml::NS_W10),
        ("a", xml::NS_A),
        ("pic", xml::NS_PIC),
        ("wp", xml::NS_WP),
    ] {
        bindings.insert(format!("xmlns:{prefix}"), uri.to_owned());
    }
    bindings
        .into_iter()
        .map(|(name, uri)| format!(" {name}=\"{}\"", xml::escape(&uri)))
        .collect()
}

fn preflight(doc: &Document, assets: &AssetRegistry) -> Result<bool> {
    let mut notes = preflight_blocks(&doc.blocks, assets)?;
    if let Some(abstract_) = &doc.meta.abstract_ {
        notes |= preflight_blocks(abstract_, assets)?;
    }
    if let Some(bibliography) = &doc.resolved.bibliography {
        for entry in &bibliography.entries {
            if let Some(prefix) = &entry.prefix {
                notes |= preflight_inlines(prefix, assets)?;
            }
            notes |= preflight_inlines(&entry.inlines, assets)?;
        }
    }
    Ok(notes)
}

fn preflight_blocks(
    blocks: &[crate::ir::Block], assets: &AssetRegistry,
) -> Result<bool> {
    use crate::ir::Block;
    let mut notes = false;
    for block in blocks {
        match block {
            Block::Heading { inlines, .. }
            | Block::Paragraph { inlines, .. } => {
                notes |= preflight_inlines(inlines, assets)?;
            }
            Block::BlockQuote { blocks, .. } | Block::Div { blocks, .. } => {
                notes |= preflight_blocks(blocks, assets)?;
            }
            Block::List { items, .. } => {
                for item in items {
                    notes |= preflight_blocks(&item.blocks, assets)?;
                }
            }
            Block::DefinitionList { items, .. } => {
                for item in items {
                    notes |= preflight_inlines(&item.term, assets)?;
                    for definition in &item.definitions {
                        notes |= preflight_blocks(definition, assets)?;
                    }
                }
            }
            Block::Figure { caption, body, .. } => {
                if let Some(caption) = caption {
                    notes |= preflight_inlines(caption, assets)?;
                }
                notes |= preflight_blocks(body, assets)?;
            }
            Block::Table { head, body, foot, .. } => {
                for row in head.iter().chain(body).chain(foot) {
                    for cell in &row.cells {
                        notes |= preflight_inlines(&cell.inlines, assets)?;
                    }
                }
            }
            Block::Callout { title, blocks, .. } => {
                if let Some(title) = title {
                    notes |= preflight_inlines(title, assets)?;
                }
                notes |= preflight_blocks(blocks, assets)?;
            }
            Block::Cell(cell) => {
                for output in &cell.outputs {
                    for candidate in &output.candidates {
                        notes |= preflight_blocks(&candidate.blocks, assets)?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(notes)
}

fn preflight_inlines(
    inlines: &[crate::ir::Inline], assets: &AssetRegistry,
) -> Result<bool> {
    use crate::ir::Inline;
    let mut notes = false;
    for inline in inlines {
        match inline {
            Inline::Image { url, alt, .. } => {
                if let Some(id) = url.strip_prefix("asset:")
                    && assets.get(id).is_none()
                {
                    assets.read(id)?;
                }
                notes |= preflight_inlines(alt, assets)?;
            }
            Inline::Footnote { blocks, .. } => {
                preflight_blocks(blocks, assets)?;
                notes = true;
            }
            Inline::Emph { inlines }
            | Inline::Strong { inlines }
            | Inline::Strikeout { inlines }
            | Inline::Subscript { inlines }
            | Inline::Superscript { inlines }
            | Inline::Underline { inlines }
            | Inline::SmallCaps { inlines }
            | Inline::Link { inlines, .. }
            | Inline::Span { inlines, .. } => {
                notes |= preflight_inlines(inlines, assets)?;
            }
            Inline::Ref { supplement, resolved, .. } => {
                if let Some(supplement) = supplement {
                    notes |= preflight_inlines(supplement, assets)?;
                }
                if let Some(resolved) = resolved {
                    notes |= preflight_inlines(&resolved.text, assets)?;
                }
            }
            Inline::Cite { items, resolved, .. } => {
                for item in items {
                    if let Some(prefix) = &item.prefix {
                        notes |= preflight_inlines(prefix, assets)?;
                    }
                    if let Some(suffix) = &item.suffix {
                        notes |= preflight_inlines(suffix, assets)?;
                    }
                }
                if let Some(resolved) = resolved {
                    notes |= preflight_inlines(&resolved.inlines, assets)?;
                }
            }
            _ => {}
        }
    }
    Ok(notes)
}

/// XML escaping, namespace constants, and ordered OOXML property builders.
pub mod xml {
    /// Schema child order for `PPR`.
    pub const PPR: &[&str] = &[
        "w:pStyle", "w:keepNext", "w:keepLines", "w:pageBreakBefore",
        "w:framePr", "w:widowControl", "w:numPr", "w:suppressLineNumbers",
        "w:pBdr", "w:shd", "w:tabs", "w:suppressAutoHyphens", "w:kinsoku",
        "w:wordWrap", "w:overflowPunct", "w:topLinePunct", "w:autoSpaceDE",
        "w:autoSpaceDN", "w:bidi", "w:adjustRightInd", "w:snapToGrid",
        "w:spacing", "w:ind", "w:contextualSpacing", "w:mirrorIndents",
        "w:suppressOverlap", "w:jc", "w:textDirection", "w:textAlignment",
        "w:textboxTightWrap", "w:outlineLvl", "w:divId", "w:cnfStyle", "w:rPr",
        "w:sectPr", "w:pPrChange",
    ];

    /// Schema child order for `RPR`.
    pub const RPR: &[&str] = &[
        "w:rStyle", "w:rFonts", "w:b", "w:bCs", "w:i", "w:iCs", "w:caps",
        "w:smallCaps", "w:strike", "w:dstrike", "w:outline", "w:shadow",
        "w:emboss", "w:imprint", "w:noProof", "w:snapToGrid", "w:vanish",
        "w:webHidden", "w:color", "w:spacing", "w:w", "w:kern", "w:position",
        "w:sz", "w:szCs", "w:highlight", "w:u", "w:effect", "w:bdr", "w:shd",
        "w:fitText", "w:vertAlign", "w:rtl", "w:cs", "w:em", "w:lang",
        "w:eastAsianLayout", "w:specVanish", "w:oMath",
    ];

    /// Schema child order for `TBLPR`.
    pub const TBLPR: &[&str] = &[
        "w:tblStyle", "w:tblpPr", "w:tblOverlap", "w:bidiVisual",
        "w:tblStyleRowBandSize", "w:tblStyleColBandSize", "w:tblW", "w:jc",
        "w:tblCellSpacing", "w:tblInd", "w:tblBorders", "w:shd", "w:tblLayout",
        "w:tblCellMar", "w:tblLook", "w:tblCaption", "w:tblDescription",
        "w:tblPrChange",
    ];

    /// Schema child order for `TRPR`.
    pub const TRPR: &[&str] = &[
        "w:cnfStyle", "w:divId", "w:gridBefore", "w:gridAfter", "w:wBefore",
        "w:wAfter", "w:cantSplit", "w:trHeight", "w:tblHeader",
        "w:tblCellSpacing", "w:jc", "w:hidden", "w:ins", "w:del",
        "w:trPrChange",
    ];

    /// Schema child order for `TCPR`.
    pub const TCPR: &[&str] = &[
        "w:cnfStyle", "w:tcW", "w:gridSpan", "w:hMerge", "w:vMerge",
        "w:tcBorders", "w:shd", "w:noWrap", "w:tcMar", "w:textDirection",
        "w:tcFitText", "w:vAlign", "w:hideMark", "w:headers", "w:cellIns",
        "w:cellDel", "w:cellMerge", "w:tcPrChange",
    ];

    /// Schema child order for `SECTPR`.
    pub const SECTPR: &[&str] = &[
        "w:headerReference", "w:footerReference", "w:footnotePr",
        "w:endnotePr", "w:type", "w:pgSz", "w:pgMar", "w:paperSrc",
        "w:pgBorders", "w:lnNumType", "w:pgNumType", "w:cols", "w:formProt",
        "w:vAlign", "w:noEndnote", "w:titlePg", "w:textDirection", "w:bidi",
        "w:rtlGutter", "w:docGrid", "w:printerSettings", "w:sectPrChange",
    ];

    /// Schema child order for `STYLE`.
    pub const STYLE: &[&str] = &[
        "w:name", "w:aliases", "w:basedOn", "w:next", "w:link",
        "w:autoRedefine", "w:hidden", "w:uiPriority", "w:semiHidden",
        "w:unhideWhenUsed", "w:qFormat", "w:locked", "w:personal",
        "w:personalCompose", "w:personalReply", "w:rsid", "w:pPr", "w:rPr",
        "w:tblPr", "w:trPr", "w:tcPr", "w:tblStylePr",
    ];

    /// Schema child order for `LVL`.
    pub const LVL: &[&str] = &[
        "w:start", "w:numFmt", "w:lvlRestart", "w:pStyle", "w:isLgl", "w:suff",
        "w:lvlText", "w:lvlPicBulletId", "w:legacy", "w:lvlJc", "w:pPr",
        "w:rPr",
    ];

    /// Schema child order for `SETTINGS`.
    pub const SETTINGS: &[&str] = &[
        "w:writeProtection",
        "w:view",
        "w:zoom",
        "w:removePersonalInformation",
        "w:removeDateAndTime",
        "w:doNotDisplayPageBoundaries",
        "w:displayBackgroundShape",
        "w:printPostScriptOverText",
        "w:printFractionalCharacterWidth",
        "w:printFormsData",
        "w:embedTrueTypeFonts",
        "w:embedSystemFonts",
        "w:saveSubsetFonts",
        "w:saveFormsData",
        "w:mirrorMargins",
        "w:alignBordersAndEdges",
        "w:bordersDoNotSurroundHeader",
        "w:bordersDoNotSurroundFooter",
        "w:gutterAtTop",
        "w:hideSpellingErrors",
        "w:hideGrammaticalErrors",
        "w:activeWritingStyle",
        "w:proofState",
        "w:formsDesign",
        "w:attachedTemplate",
        "w:linkStyles",
        "w:stylePaneFormatFilter",
        "w:stylePaneSortMethod",
        "w:documentType",
        "w:mailMerge",
        "w:revisionView",
        "w:trackRevisions",
        "w:doNotTrackMoves",
        "w:doNotTrackFormatting",
        "w:documentProtection",
        "w:autoFormatOverride",
        "w:styleLockTheme",
        "w:styleLockQFSet",
        "w:defaultTabStop",
        "w:autoHyphenation",
        "w:consecutiveHyphenLimit",
        "w:hyphenationZone",
        "w:doNotHyphenateCaps",
        "w:showEnvelope",
        "w:summaryLength",
        "w:clickAndTypeStyle",
        "w:defaultTableStyle",
        "w:evenAndOddHeaders",
        "w:bookFoldRevPrinting",
        "w:bookFoldPrinting",
        "w:bookFoldPrintingSheets",
        "w:drawingGridHorizontalSpacing",
        "w:drawingGridVerticalSpacing",
        "w:displayHorizontalDrawingGridEvery",
        "w:displayVerticalDrawingGridEvery",
        "w:doNotUseMarginsForDrawingGridOrigin",
        "w:drawingGridHorizontalOrigin",
        "w:drawingGridVerticalOrigin",
        "w:doNotShadeFormData",
        "w:noPunctuationKerning",
        "w:characterSpacingControl",
        "w:printTwoOnOne",
        "w:strictFirstAndLastChars",
        "w:noLineBreaksAfter",
        "w:noLineBreaksBefore",
        "w:savePreviewPicture",
        "w:doNotValidateAgainstSchema",
        "w:saveInvalidXml",
        "w:ignoreMixedContent",
        "w:alwaysShowPlaceholderText",
        "w:doNotDemarcateInvalidXml",
        "w:saveXmlDataOnly",
        "w:useXSLTWhenSaving",
        "w:saveThroughXslt",
        "w:showXMLTags",
        "w:alwaysMergeEmptyNamespace",
        "w:updateFields",
        "w:hdrShapeDefaults",
        "w:footnotePr",
        "w:endnotePr",
        "w:compat",
        "w:docVars",
        "w:rsids",
        "m:mathPr",
        "w:attachedSchema",
        "w:themeFontLang",
        "w:clrSchemeMapping",
        "w:doNotIncludeSubdocsInStats",
        "w:doNotAutoCompressPictures",
        "w:forceUpgrade",
        "w:captions",
        "w:readModeInkLockDown",
        "w:smartTagType",
        "sl:schemaLibrary",
        "w:shapeDefaults",
        "w:doNotEmbedSmartTags",
        "w:decimalSymbol",
        "w:listSeparator",
    ];

    /// Schema child order for `ORDERS`.
    pub const ORDERS: &[(&str, &[&str])] = &[
        ("w:styles", &["w:docDefaults", "w:latentStyles", "w:style"]),
        ("w:docDefaults", &["w:rPrDefault", "w:pPrDefault"]),
        (
            "w:numbering",
            &[
                "w:numPicBullet", "w:abstractNum", "w:num",
                "w:numIdMacAtCleanup",
            ],
        ),
        (
            "w:abstractNum",
            &[
                "w:nsid", "w:multiLevelType", "w:tmpl", "w:name",
                "w:styleLink", "w:numStyleLink", "w:lvl",
            ],
        ),
        ("w:num", &["w:abstractNumId", "w:lvlOverride"]),
        ("w:lvlOverride", &["w:startOverride", "w:lvl"]),
        ("w:numPr", &["w:ilvl", "w:numId"]),
        (
            "w:pBdr",
            &["w:top", "w:left", "w:bottom", "w:right", "w:between", "w:bar"],
        ),
        (
            "w:tblBorders",
            &[
                "w:top", "w:left", "w:bottom", "w:right", "w:insideH",
                "w:insideV",
            ],
        ),
        (
            "w:tcBorders",
            &[
                "w:top", "w:left", "w:bottom", "w:right", "w:insideH",
                "w:insideV", "w:tl2br", "w:tr2bl",
            ],
        ),
        ("w:tblCellMar", &["w:top", "w:left", "w:bottom", "w:right"]),
        ("w:footnotePr", &["w:pos", "w:numFmt", "w:numStart", "w:numRestart"]),
        ("w:sdt", &["w:sdtPr", "w:sdtEndPr", "w:sdtContent"]),
        (
            "wp:inline",
            &[
                "wp:extent", "wp:effectExtent", "wp:docPr",
                "wp:cNvGraphicFramePr", "a:graphic",
            ],
        ),
        ("m:r", &["m:rPr", "w:rPr", "m:t"]),
        ("m:rPr", &["m:lit", "m:nor", "m:scr", "m:sty", "m:brk", "m:aln"]),
        (
            "m:naryPr",
            &[
                "m:chr", "m:limLoc", "m:grow", "m:subHide", "m:supHide",
                "m:ctrlPr",
            ],
        ),
        (
            "m:dPr",
            &[
                "m:begChr", "m:sepChr", "m:endChr", "m:grow", "m:shp",
                "m:ctrlPr",
            ],
        ),
        (
            "m:mPr",
            &[
                "m:baseJc", "m:plcHide", "m:rSpRule", "m:cGpRule", "m:rSp",
                "m:cSp", "m:cGp", "m:mcs", "m:ctrlPr",
            ],
        ),
        ("m:mcPr", &["m:count", "m:mcJc"]),
        ("m:groupChrPr", &["m:chr", "m:pos", "m:vertJc", "m:ctrlPr"]),
        (
            "m:phantPr",
            &[
                "m:show", "m:zeroWid", "m:zeroAsc", "m:zeroDesc", "m:transp",
                "m:ctrlPr",
            ],
        ),
        (
            "m:borderBoxPr",
            &[
                "m:hideTop", "m:hideBot", "m:hideLeft", "m:hideRight",
                "m:strikeH", "m:strikeV", "m:strikeBLTR", "m:strikeTLBR",
                "m:ctrlPr",
            ],
        ),
        ("m:fPr", &["m:type", "m:ctrlPr"]),
        ("m:radPr", &["m:degHide", "m:ctrlPr"]),
        ("m:accPr", &["m:chr", "m:ctrlPr"]),
        ("m:barPr", &["m:pos", "m:ctrlPr"]),
        (
            "m:boxPr",
            &["m:opEmu", "m:noBreak", "m:diff", "m:brk", "m:aln", "m:ctrlPr"],
        ),
        (
            "m:eqArrPr",
            &[
                "m:baseJc", "m:maxDist", "m:objDist", "m:rSpRule", "m:rSp",
                "m:ctrlPr",
            ],
        ),
        ("m:f", &["m:fPr", "m:num", "m:den"]),
        ("m:rad", &["m:radPr", "m:deg", "m:e"]),
        ("m:nary", &["m:naryPr", "m:sub", "m:sup", "m:e"]),
        ("m:sSubSup", &["m:sSubSupPr", "m:e", "m:sub", "m:sup"]),
        ("m:sPre", &["m:sPrePr", "m:sub", "m:sup", "m:e"]),
        ("m:limLow", &["m:limLowPr", "m:e", "m:lim"]),
        ("m:limUpp", &["m:limUppPr", "m:e", "m:lim"]),
        (
            "m:mathPr",
            &[
                "m:mathFont", "m:brkBin", "m:brkBinSub", "m:smallFrac",
                "m:dispDef", "m:lMargin", "m:rMargin", "m:defJc", "m:preSp",
                "m:postSp", "m:interSp", "m:intraSp", "m:wrapIndent",
                "m:wrapRight", "m:intLim", "m:naryLim",
            ],
        ),
    ];

    /// XML declaration for every generated part.
    pub const XML_HEAD: &str =
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#;

    /// Namespace URI for `w`.
    pub const NS_W: &str =
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

    /// Namespace URI for `m`.
    pub const NS_M: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/math";

    /// Namespace URI for `r`.
    pub const NS_R: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

    /// Namespace URI for `rels`.
    pub const NS_RELS: &str =
        "http://schemas.openxmlformats.org/package/2006/relationships";

    /// Namespace URI for `o`.
    pub const NS_O: &str = "urn:schemas-microsoft-com:office:office";

    /// Namespace URI for `v`.
    pub const NS_V: &str = "urn:schemas-microsoft-com:vml";

    /// Namespace URI for `w10`.
    pub const NS_W10: &str = "urn:schemas-microsoft-com:office:word";

    /// Namespace URI for `a`.
    pub const NS_A: &str =
        "http://schemas.openxmlformats.org/drawingml/2006/main";

    /// Namespace URI for `pic`.
    pub const NS_PIC: &str =
        "http://schemas.openxmlformats.org/drawingml/2006/picture";

    /// Namespace URI for `wp`.
    pub const NS_WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";

    /// Namespace URI for `sl`.
    pub const NS_SL: &str =
        "http://schemas.openxmlformats.org/schemaLibrary/2006/main";

    /// Relationship type for officeDocument.
    pub const REL_OFFICE_DOCUMENT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";

    /// Relationship type for extended-properties.
    pub const REL_APP: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties";

    /// Relationship type for image.
    pub const REL_IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";

    /// Relationship type for hyperlink.
    pub const REL_HYPERLINK: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";

    /// Relationship type for styles.
    pub const REL_STYLES: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";

    /// Relationship type for numbering.
    pub const REL_NUMBERING: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering";

    /// Relationship type for settings.
    pub const REL_SETTINGS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings";

    /// Relationship type for footnotes.
    pub const REL_FOOTNOTES: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes";

    /// Relationship type for footer.
    pub const REL_FOOTER: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer";

    /// Relationship type for header.
    pub const REL_HEADER: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header";

    /// Relationship type for theme.
    pub const REL_THEME: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme";

    /// Relationship type for fontTable.
    pub const REL_FONT_TABLE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/fontTable";

    /// Relationship type for webSettings.
    pub const REL_WEB_SETTINGS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/webSettings";

    /// Core properties relationship type.
    pub const REL_CORE: &str = "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";

    /// Content type for document.
    pub const CT_DOCUMENT: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";

    /// Content type for core.
    pub const CT_CORE: &str =
        "application/vnd.openxmlformats-package.core-properties+xml";

    /// Content type for app.
    pub const CT_APP: &str =
        "application/vnd.openxmlformats-officedocument.extended-properties+xml";

    /// Content type for rels.
    pub const CT_RELS: &str =
        "application/vnd.openxmlformats-package.relationships+xml";

    /// Content type for styles.
    pub const CT_STYLES: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml";

    /// Content type for numbering.
    pub const CT_NUMBERING: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml";

    /// Content type for settings.
    pub const CT_SETTINGS: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml";

    /// Content type for footnotes.
    pub const CT_FOOTNOTES: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml";

    /// Content type for footer.
    pub const CT_FOOTER: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml";

    /// Content type for header.
    pub const CT_HEADER: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml";

    /// Content type for theme.
    pub const CT_THEME: &str =
        "application/vnd.openxmlformats-officedocument.theme+xml";

    /// Content type for font table.
    pub const CT_FONT_TABLE: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml";

    /// Content type for web settings.
    pub const CT_WEB_SETTINGS: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.webSettings+xml";

    /// English Metric Units per inch.
    pub const EMU_PER_INCH: u64 = 914_400;
    /// English Metric Units per typographic point.
    pub const EMU_PER_POINT: u64 = 12_700;

    /// Escapes XML text and attribute values after removing invalid characters.
    pub fn escape(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for c in clean(text).chars() {
            match c {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                c => out.push(c),
            }
        }
        out
    }

    /// Removes characters XML 1.0 cannot hold.
    pub fn clean(text: &str) -> String {
        text.chars().filter(|&c| is_xml_char(c)).collect()
    }

    /// Whether XML 1.0 can hold `c` (its `Char` production).
    pub fn is_xml_char(c: char) -> bool {
        matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..)
    }

    /// Collapses whitespace to single spaces and trims the result.
    pub fn single_line(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Builds a complex Word field, retaining the separator for empty results.
    pub fn field(instr: &str, result_runs: &str) -> String {
        let (start, end) = field_parts(instr);
        format!("{start}{result_runs}{end}")
    }

    /// Shared framing for cached fields spanning multiple paragraphs.
    pub(crate) fn field_parts(instr: &str) -> (String, &'static str) {
        let start = format!(
            "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> {} </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>",
            escape(instr),
        );
        (start, "<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>")
    }

    /// Ordered XML children, serialized by schema rank then insertion sequence.
    #[derive(Debug, Clone)]
    pub struct Props {
        tag: &'static str,
        attrs: String,
        order: &'static [&'static str],
        children: Vec<(usize, usize, String)>,
    }

    impl Props {
        /// Creates an empty property element using the supplied child order.
        pub fn new(tag: &'static str, order: &'static [&'static str]) -> Props {
            Props { tag, attrs: String::new(), order, children: Vec::new() }
        }

        /// Adds already escaped attributes to the outer property element.
        pub fn with_attrs(mut self, attrs: &str) -> Self {
            self.attrs = attrs.trim().to_owned();
            self
        }

        /// Replaces every child of this name with the supplied XML.
        pub fn set(
            &mut self, name: &'static str, xml: impl Into<String>,
        ) -> &mut Self {
            if let Some(rank) = self.rank(name) {
                self.children.retain(|(index, _, _)| *index != rank);
                self.append(rank, xml.into());
            }
            self
        }

        /// Appends a repeatable child without replacing earlier occurrences.
        pub fn push(
            &mut self, name: &'static str, xml: impl Into<String>,
        ) -> &mut Self {
            if let Some(rank) = self.rank(name) {
                self.append(rank, xml.into());
            }
            self
        }

        /// Whether no property children have been added.
        pub fn is_empty(&self) -> bool {
            self.children.is_empty()
        }

        /// Serializes ordered children, or returns an empty string when empty.
        pub fn to_xml(&self) -> String {
            if self.is_empty() {
                return String::new();
            }
            let mut children: Vec<_> = self.children.iter().collect();
            children.sort_by_key(|(rank, sequence, _)| (*rank, *sequence));
            let attrs = if self.attrs.is_empty() {
                String::new()
            } else {
                format!(" {}", self.attrs)
            };
            let mut xml = format!("<{}{attrs}>", self.tag);
            for (_, _, child) in children {
                xml.push_str(child);
            }
            xml.push_str(&format!("</{}>", self.tag));
            xml
        }

        fn rank(&self, name: &str) -> Option<usize> {
            let rank = self.order.iter().position(|child| *child == name);
            if rank.is_none() {
                tracing::error!(
                    parent = self.tag,
                    child = name,
                    "unknown OOXML property child"
                );
                debug_assert!(false, "unknown child {name} of {}", self.tag);
            }
            rank
        }

        fn append(&mut self, rank: usize, xml: String) {
            let sequence = self
                .children
                .iter()
                .map(|(_, sequence, _)| *sequence)
                .max()
                .map_or(0, |n| n + 1);
            self.children.push((rank, sequence, xml));
        }
    }

    /// Paragraph properties, serialized through the schema order.
    #[derive(Debug, Clone, Default)]
    pub struct ParaProps {
        /// Resolved paragraph style identifier.
        pub style: Option<String>,
        /// Keep this paragraph with the following paragraph.
        pub keep_next: bool,
        /// List level and numbering instance identifier.
        pub num: Option<(u8, u32)>,
        /// Complete `w:pBdr` element.
        pub border: Option<String>,
        /// Complete `w:shd` element.
        pub shading: Option<String>,
        /// `w:tab` elements wrapped in one `w:tabs` element.
        pub tabs: Vec<String>,
        /// Complete `w:spacing` element.
        pub spacing: Option<String>,
        /// Complete `w:ind` element.
        pub ind: Option<String>,
        /// Alignment value.
        pub jc: Option<&'static str>,
    }

    impl ParaProps {
        /// Serializes these properties, omitting an empty property element.
        pub fn to_xml(&self) -> String {
            let mut props = Props::new("w:pPr", PPR);
            val(&mut props, "w:pStyle", self.style.as_deref());
            flag(&mut props, "w:keepNext", self.keep_next);
            if let Some((level, id)) = self.num {
                props.set("w:numPr", format!(
                    "<w:numPr><w:ilvl w:val=\"{level}\"/><w:numId w:val=\"{id}\"/></w:numPr>"
                ));
            }
            raw(&mut props, "w:pBdr", &self.border);
            raw(&mut props, "w:shd", &self.shading);
            if !self.tabs.is_empty() {
                props.set(
                    "w:tabs",
                    format!("<w:tabs>{}</w:tabs>", self.tabs.join("")),
                );
            }
            raw(&mut props, "w:spacing", &self.spacing);
            raw(&mut props, "w:ind", &self.ind);
            val(&mut props, "w:jc", self.jc);
            props.to_xml()
        }
    }

    /// Run properties shared by ordinary text and generated fields.
    #[derive(Debug, Clone, Default)]
    pub struct RunProps {
        /// Resolved character style identifier.
        pub style: Option<String>,
        /// Complete `w:rFonts` element.
        pub fonts: Option<String>,
        /// Bold in both ordinary and complex scripts.
        pub bold: bool,
        /// Italic in both ordinary and complex scripts.
        pub italic: bool,
        /// Small capitals.
        pub small_caps: bool,
        /// Single strikethrough.
        pub strike: bool,
        /// Text colour value.
        pub color: Option<String>,
        /// Baseline offset in half-points.
        pub position: Option<i32>,
        /// Single underline.
        pub underline: bool,
        /// Complete `w:shd` element.
        pub shading: Option<String>,
        /// Vertical alignment value.
        pub vert_align: Option<&'static str>,
    }

    impl RunProps {
        /// Serializes these properties, omitting an empty property element.
        pub fn to_xml(&self) -> String {
            let mut props = Props::new("w:rPr", RPR);
            val(&mut props, "w:rStyle", self.style.as_deref());
            raw(&mut props, "w:rFonts", &self.fonts);
            flag(&mut props, "w:b", self.bold);
            flag(&mut props, "w:bCs", self.bold);
            flag(&mut props, "w:i", self.italic);
            flag(&mut props, "w:iCs", self.italic);
            flag(&mut props, "w:smallCaps", self.small_caps);
            flag(&mut props, "w:strike", self.strike);
            val(&mut props, "w:color", self.color.as_deref());
            if let Some(position) = self.position {
                val(&mut props, "w:position", Some(&position.to_string()));
            }
            if self.underline {
                val(&mut props, "w:u", Some("single"));
            }
            raw(&mut props, "w:shd", &self.shading);
            val(&mut props, "w:vertAlign", self.vert_align);
            props.to_xml()
        }
    }

    /// Table properties; XML-valued fields hold their complete elements.
    #[derive(Debug, Clone, Default)]
    pub struct TableProps {
        /// Resolved table style identifier.
        pub style: Option<String>,
        /// Complete `w:tblW` element.
        pub width: Option<String>,
        /// Complete `w:tblBorders` element.
        pub borders: Option<String>,
        /// Use fixed column widths.
        pub layout_fixed: bool,
        /// Complete `w:tblLook` element.
        pub look: Option<String>,
        /// Complete `w:tblCaption` element.
        pub caption: Option<String>,
    }

    impl TableProps {
        /// Serializes these properties, omitting an empty property element.
        pub fn to_xml(&self) -> String {
            let mut props = Props::new("w:tblPr", TBLPR);
            val(&mut props, "w:tblStyle", self.style.as_deref());
            raw(&mut props, "w:tblW", &self.width);
            raw(&mut props, "w:tblBorders", &self.borders);
            if self.layout_fixed {
                props.set("w:tblLayout", "<w:tblLayout w:type=\"fixed\"/>");
            }
            raw(&mut props, "w:tblLook", &self.look);
            raw(&mut props, "w:tblCaption", &self.caption);
            props.to_xml()
        }
    }

    /// Table row properties.
    #[derive(Debug, Clone, Default)]
    pub struct RowProps {
        /// Prevent splitting across pages.
        pub cant_split: bool,
        /// Repeat as a header row.
        pub header: bool,
    }

    impl RowProps {
        /// Serializes these properties, omitting an empty property element.
        pub fn to_xml(&self) -> String {
            let mut props = Props::new("w:trPr", TRPR);
            flag(&mut props, "w:cantSplit", self.cant_split);
            flag(&mut props, "w:tblHeader", self.header);
            props.to_xml()
        }
    }

    /// Table cell properties.
    #[derive(Debug, Clone, Default)]
    pub struct CellProps {
        /// Complete `w:tcW` element.
        pub width: Option<String>,
        /// Number of grid columns spanned by this cell.
        pub grid_span: Option<u32>,
        /// Vertical merge state: `restart` or `continue`.
        pub v_merge: Option<&'static str>,
        /// Complete `w:shd` element.
        pub shading: Option<String>,
        /// Vertical alignment value.
        pub v_align: Option<&'static str>,
    }

    impl CellProps {
        /// Serializes these properties, omitting an empty property element.
        pub fn to_xml(&self) -> String {
            let mut props = Props::new("w:tcPr", TCPR);
            raw(&mut props, "w:tcW", &self.width);
            if let Some(span) = self.grid_span {
                val(&mut props, "w:gridSpan", Some(&span.to_string()));
            }
            if self.v_merge == Some("continue") {
                flag(&mut props, "w:vMerge", true);
            } else {
                val(&mut props, "w:vMerge", self.v_merge);
            }
            raw(&mut props, "w:shd", &self.shading);
            val(&mut props, "w:vAlign", self.v_align);
            props.to_xml()
        }
    }

    fn raw(props: &mut Props, name: &'static str, xml: &Option<String>) {
        if let Some(xml) = xml {
            props.set(name, xml.clone());
        }
    }

    fn flag(props: &mut Props, name: &'static str, value: bool) {
        if value {
            props.set(name, format!("<{name}/>"));
        }
    }

    fn val(props: &mut Props, name: &'static str, value: Option<&str>) {
        if let Some(value) = value {
            props.set(name, format!("<{name} w:val=\"{}\"/>", escape(value)));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::validate::parse_xml;
    use super::xml::*;

    #[test]
    fn escape_and_clean_match_presenter() {
        assert_eq!(clean("a\tb\nc\rd"), "a\tb\nc\rd");
        assert_eq!(clean("a\u{0}b\u{8}c\u{B}d\u{1F}e\u{7F}f"), "abcde\u{7F}f",);
        assert_eq!(escape("a&b<c>d\"e'f"), "a&amp;b&lt;c&gt;d&quot;e'f");
        assert_eq!(clean("x\u{fffe}\u{ffff}y"), "xy");
        assert_eq!(single_line("  One\n\t two\r\n words "), "One two words");
    }

    #[test]
    fn field_has_the_complex_form() {
        let result = "<w:r><w:t xml:space=\"preserve\">1</w:t></w:r>";
        assert_eq!(
            field("PAGE", result),
            format!(
                concat!(
                    "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r>",
                    "<w:r><w:instrText xml:space=\"preserve\"> PAGE </w:instrText></w:r>",
                    "<w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>{}",
                    "<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>"
                ),
                result
            ),
        );
        let empty = field("A&B", "");
        assert!(empty.contains(" A&amp;B "));
        assert!(empty.contains(
            "<w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:fldChar w:fldCharType=\"end\"/>"
        ));
        assert!(!empty.contains("w:dirty"));
    }

    #[test]
    fn props_sort_by_order() {
        let mut props = Props::new("w:pPr", PPR);
        assert!(props.is_empty());
        assert_eq!(props.to_xml(), "");
        props
            .set("w:jc", "<w:jc w:val=\"right\"/>")
            .set("w:pStyle", "<w:pStyle w:val=\"old\"/>")
            .set("w:pStyle", "<w:pStyle w:val=\"new\"/>");
        assert_eq!(
            props.to_xml(),
            concat!(
                "<w:pPr><w:pStyle w:val=\"new\"/>",
                "<w:jc w:val=\"right\"/></w:pPr>"
            )
        );
        let mut props = Props::new("w:abstractNum", ORDERS[3].1)
            .with_attrs("w:abstractNumId=\"991\"");
        props
            .push("w:lvl", "<w:lvl w:ilvl=\"0\"/>")
            .set("w:multiLevelType", "<w:multiLevelType w:val=\"multilevel\"/>")
            .push("w:lvl", "<w:lvl w:ilvl=\"1\"/>");
        assert_eq!(
            props.to_xml(),
            concat!(
                "<w:abstractNum w:abstractNumId=\"991\">",
                "<w:multiLevelType w:val=\"multilevel\"/>",
                "<w:lvl w:ilvl=\"0\"/><w:lvl w:ilvl=\"1\"/></w:abstractNum>"
            )
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "unknown child")]
    fn unknown_child_panics_in_debug() {
        Props::new("w:pPr", PPR).set("w:bogus", "<w:bogus/>");
    }

    #[test]
    fn order_tables_have_no_duplicates() {
        let tables =
            [PPR, RPR, TBLPR, TRPR, TCPR, SECTPR, STYLE, LVL, SETTINGS];
        for order in tables.into_iter().chain(ORDERS.iter().map(|(_, o)| *o)) {
            let unique: BTreeSet<_> = order.iter().copied().collect();
            assert_eq!(unique.len(), order.len(), "{order:?}");
        }
        let parents: BTreeSet<_> =
            ORDERS.iter().map(|(name, _)| name).collect();
        assert_eq!(parents.len(), ORDERS.len());
    }

    fn children(xml: &str) -> Vec<String> {
        let wrapped = format!("<root xmlns:w=\"{NS_W}\">{xml}</root>");
        let root = parse_xml(&wrapped).unwrap();
        root.children[0].children.iter().map(|node| node.name.clone()).collect()
    }

    #[test]
    fn builders_follow_child_order() {
        let para = ParaProps {
            style: Some("BodyText".into()),
            keep_next: true,
            num: Some((2, 1001)),
            border: Some("<w:pBdr/>".into()),
            shading: Some("<w:shd/>".into()),
            tabs: vec!["<w:tab/>".into()],
            spacing: Some("<w:spacing/>".into()),
            ind: Some("<w:ind/>".into()),
            jc: Some("right"),
        };
        assert_eq!(
            children(&para.to_xml()),
            [
                "w:pStyle", "w:keepNext", "w:numPr", "w:pBdr", "w:shd",
                "w:tabs", "w:spacing", "w:ind", "w:jc",
            ]
        );
        let run = RunProps {
            style: Some("BodyTextChar".into()),
            fonts: Some("<w:rFonts/>".into()),
            bold: true,
            italic: true,
            small_caps: true,
            strike: true,
            color: Some("ABCDEF".into()),
            position: Some(-3),
            underline: true,
            shading: Some("<w:shd/>".into()),
            vert_align: Some("subscript"),
        };
        assert_eq!(
            children(&run.to_xml()),
            [
                "w:rStyle", "w:rFonts", "w:b", "w:bCs", "w:i", "w:iCs",
                "w:smallCaps", "w:strike", "w:color", "w:position", "w:u",
                "w:shd", "w:vertAlign",
            ]
        );
        let table = TableProps {
            style: Some("Table".into()),
            width: Some("<w:tblW/>".into()),
            borders: Some("<w:tblBorders/>".into()),
            layout_fixed: true,
            look: Some("<w:tblLook/>".into()),
            caption: Some("<w:tblCaption/>".into()),
        };
        assert_eq!(
            children(&table.to_xml()),
            [
                "w:tblStyle", "w:tblW", "w:tblBorders", "w:tblLayout",
                "w:tblLook", "w:tblCaption",
            ]
        );
        assert_eq!(
            children(&RowProps { cant_split: true, header: true }.to_xml()),
            ["w:cantSplit", "w:tblHeader"]
        );
        let cell = CellProps {
            width: Some("<w:tcW/>".into()),
            grid_span: Some(2),
            v_merge: Some("restart"),
            shading: Some("<w:shd/>".into()),
            v_align: Some("bottom"),
        };
        assert_eq!(
            children(&cell.to_xml()),
            ["w:tcW", "w:gridSpan", "w:vMerge", "w:shd", "w:vAlign",]
        );
        let continued =
            CellProps { v_merge: Some("continue"), ..CellProps::default() }
                .to_xml();
        assert_eq!(continued, "<w:tcPr><w:vMerge/></w:tcPr>");
        assert_eq!(RunProps::default().to_xml(), "");
        assert_eq!(ParaProps::default().to_xml(), "");
        assert_eq!(TableProps::default().to_xml(), "");
        assert_eq!(RowProps::default().to_xml(), "");
        assert_eq!(CellProps::default().to_xml(), "");
    }
}
