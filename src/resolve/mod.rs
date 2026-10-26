//! Document reference and numbering resolution (references.md §4; ir.md §4).
//! Owner: T01 → T17. Stub created by T00.

pub mod footnotes;
pub mod l10n;
pub mod labels;
pub mod numbering;
pub mod toc;

pub use l10n::{LangTag, Term, caption_separator, supplement, term};

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::cite;
use crate::config::{
    Config, EquationNumbering, FigureNumbering, HeadingNumbering, Toc,
};
use crate::error::{Diagnostic, Diagnostics, Level};
use crate::ir::{
    Attrs, Block, CiteItem, CiteMode, Document, FigureKind, Inline, Inlines,
    LabelInfo, LabelKind, RefForm, Resolved, ResolvedRef, SourceInfo,
    SourceKind, SourceSpan, TocEntry, detect_kind,
};
use footnotes::{Footnotes, NoteKey};
use labels::{auto_id, label_text, merge_text, reserved, text_inline};
use numbering::{Counters, NumberingPattern};

pub use labels::anchor;

/// The shared bibliography target in every output.
pub const BIBLIOGRAPHY_ANCHOR: &str = "refs";

/// Effective policy and optional citation library for one resolve pass.
pub struct ResolveOptions<'a> {
    /// Configuration already merged by the frontend/pipeline.
    pub config: &'a Config,
    /// Loaded bibliography, absent until citations are configured.
    pub library: Option<&'a cite::Library>,
    /// Unresolved references become warnings rather than errors.
    pub lenient: bool,
}

/// Resolves labels, numbering, references, notes and the shared TOC model.
/// Problems are recorded as diagnostics; invalid references become placeholders.
pub fn resolve(
    document: &mut Document, opts: &ResolveOptions<'_>,
) -> Diagnostics {
    let mut resolver = Resolver::new(document, opts);
    document.resolved = Resolved::default();
    for phase in [Phase::Setup, Phase::Labels, Phase::AutoIds, Phase::Number] {
        resolver.document(document, phase);
    }
    resolver.render_citations(document);
    resolver.document(document, Phase::Finish);
    resolver.has_bibliography = document.resolved.bibliography.is_some();
    if document.resolved.bibliography.is_some() && !resolver.bibliography_seen {
        document.blocks.push(Block::Bibliography { span: None });
    }
    resolver.document(document, Phase::Assemble);
    document.resolved.labels = resolver
        .labels
        .into_iter()
        .map(|(label, record)| (label, record.info))
        .collect();
    document.resolved.toc = resolver.toc;
    document.resolved.footnotes = resolver.notes.count;
    document.features.citations = false;
    document.features.footnotes = false;
    crate::ir::walk_inlines(document, &mut |inline| {
        document_features(inline, &mut resolver.features);
    });
    document.features.citations = resolver.features.0;
    document.features.footnotes = resolver.features.1;
    resolver.diagnostics
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Setup,
    Labels,
    AutoIds,
    Number,
    Finish,
    Assemble,
}

#[derive(Clone)]
struct Context {
    span: Option<SourceSpan>,
    scope: PathBuf,
    typst: bool,
    position: String,
    identity: usize,
    abstract_: bool,
    primary: bool,
}

impl Context {
    fn child(&self, suffix: impl std::fmt::Display) -> Context {
        Context {
            position: format!("{}{suffix}", self.position),
            ..self.clone()
        }
    }

    fn at(&self, span: Option<&SourceSpan>, source: &SourceInfo) -> Context {
        let Some(span) = span else {
            return self.clone();
        };
        let path = span.file.as_ref().unwrap_or(&source.path);
        let kind = if path == &source.path {
            Some(source.kind)
        } else {
            detect_kind(path).ok()
        };
        Context {
            span: Some(span.clone()),
            scope: path.clone(),
            typst: kind == Some(SourceKind::Typst),
            ..self.clone()
        }
    }

    fn note(&self, label: &str) -> NoteKey {
        NoteKey::new(&self.scope, self.typst, label)
    }
}

struct LabelRecord {
    info: LabelInfo,
    path: PathBuf,
    identity: usize,
}

struct OwnedRequest {
    items: Vec<CiteItem>,
    mode: CiteMode,
    note_number: Option<u32>,
}

struct Resolver<'a> {
    opts: &'a ResolveOptions<'a>,
    source: SourceInfo,
    lang: LangTag,
    pattern: Option<NumberingPattern>,
    section: bool,
    keys: BTreeSet<String>,
    note_style: bool,
    labels: BTreeMap<String, LabelRecord>,
    anchors: BTreeMap<String, String>,
    taken: BTreeSet<String>,
    counters: Counters,
    notes: Footnotes,
    candidate_notes: BTreeMap<String, Footnotes>,
    requests: Vec<OwnedRequest>,
    toc: Vec<TocEntry>,
    bibliography_seen: bool,
    has_bibliography: bool,
    diagnostics: Diagnostics,
    features: (bool, bool),
}

impl<'a> Resolver<'a> {
    fn new(document: &mut Document, opts: &'a ResolveOptions<'a>) -> Self {
        let lang = LangTag::effective(&document.meta, opts.config);
        let mut resolver = Resolver {
            opts,
            source: document.source.clone(),
            lang: lang
                .clone()
                .unwrap_or(LangTag { lang: "en".into(), region: None }),
            pattern: None,
            section: opts.config.numbering.figures == FigureNumbering::Section,
            keys: opts.library.map(cite::Library::keys).unwrap_or_default(),
            note_style: opts.library.is_some_and(cite::Library::is_note_style),
            labels: BTreeMap::new(),
            anchors: BTreeMap::new(),
            taken: BTreeSet::new(),
            counters: Counters::default(),
            notes: Footnotes::default(),
            candidate_notes: BTreeMap::new(),
            requests: Vec::new(),
            toc: Vec::new(),
            bibliography_seen: false,
            has_bibliography: false,
            diagnostics: Diagnostics::new(),
            features: (false, false),
        };
        let context = resolver.root(false);
        if lang.is_none() {
            let tag = document
                .meta
                .lang
                .as_deref()
                .unwrap_or(&opts.config.document.lang);
            resolver.warn(
                "lang-invalid",
                format!("unknown language tag `{tag}`; using English"),
                &context,
            );
        }
        resolver.pattern = match &opts.config.numbering.headings {
            HeadingNumbering::Off => None,
            HeadingNumbering::Pattern(pattern) => NumberingPattern::parse(pattern).or_else(|| {
                resolver.error("numbering-invalid", format!("invalid numbering pattern `{pattern}` (falls back to `1.1`)"), &context);
                NumberingPattern::parse("1.1")
            }),
        };
        if resolver.section && resolver.pattern.is_none() {
            resolver.warn("numbering-section-without-headings", "figures = \"section\" needs numbered headings; numbering per document", &context);
            resolver.section = false;
        }
        if let Toc::Depth(depth) = opts.config.document.toc
            && !has_toc(document)
        {
            document.blocks.insert(0, Block::Toc { depth, span: None });
        }
        resolver.taken.extend(resolver.keys.iter().cloned());
        resolver
    }

    fn root(&self, abstract_: bool) -> Context {
        Context {
            span: None,
            scope: self.source.path.clone(),
            typst: self.source.kind == SourceKind::Typst,
            position: if abstract_ {
                "abstract".into()
            } else {
                "body".into()
            },
            identity: 0,
            abstract_,
            primary: true,
        }
    }

    fn report(
        &mut self, level: Level, code: &'static str,
        message: impl Into<String>, context: &Context,
    ) {
        let mut diagnostic = Diagnostic::new(level, code, message);
        diagnostic.path = Some(
            context
                .span
                .as_ref()
                .and_then(|span| span.file.clone())
                .unwrap_or_else(|| context.scope.clone()),
        );
        diagnostic.span = context.span.clone();
        self.diagnostics.push(diagnostic);
    }

    fn warn(
        &mut self, code: &'static str, message: impl Into<String>,
        context: &Context,
    ) {
        self.report(Level::Warning, code, message, context);
    }

    fn error(
        &mut self, code: &'static str, message: impl Into<String>,
        context: &Context,
    ) {
        self.report(Level::Error, code, message, context);
    }

    fn document(&mut self, document: &mut Document, phase: Phase) {
        if let Some(abstract_) = &mut document.meta.abstract_ {
            self.blocks(abstract_, &self.root(true), phase);
        }
        self.blocks(&mut document.blocks, &self.root(false), phase);
    }

    fn blocks(
        &mut self, blocks: &mut Vec<Block>, parent: &Context, phase: Phase,
    ) {
        let mut index = 0;
        while index < blocks.len() {
            let mut context = parent
                .child(format_args!("[{index}]"))
                .at(blocks[index].span(), &self.source);
            // Block allocations survive inline classification even when its
            // expansions change the enclosing inline's sequence index.
            context.identity = &blocks[index] as *const Block as usize;
            if phase == Phase::Setup
                && matches!(blocks[index], Block::Bibliography { .. })
            {
                if self.bibliography_seen {
                    self.warn(
                        "bibliography-duplicate",
                        "a document has one bibliography; this one is ignored",
                        &context,
                    );
                    blocks.remove(index);
                    continue;
                }
                self.bibliography_seen = true;
            }
            self.block(&mut blocks[index], &context, phase);
            index += 1;
        }
    }

    fn block(&mut self, block: &mut Block, context: &Context, phase: Phase) {
        if phase == Phase::Labels {
            self.block_label(block, context);
        }
        if phase == Phase::AutoIds
            && let Block::Heading { attrs, inlines, .. } = block
            && attrs.id.is_none()
        {
            let id = auto_id(inlines, &self.taken);
            self.register(
                &id,
                LabelKind::Heading,
                Some(label_text(inlines)),
                context,
                false,
            );
            attrs.id = Some(id);
        }
        if phase == Phase::Number {
            self.number_block(block, context);
        }
        if phase == Phase::Assemble {
            self.toc_block(block, context);
        }
        self.block_children(block, context, phase);
    }

    fn block_children(
        &mut self, block: &mut Block, context: &Context, phase: Phase,
    ) {
        match block {
            Block::Heading { inlines, .. }
            | Block::Paragraph { inlines, .. } => {
                self.inlines(inlines, &context.child(".inlines"), phase)
            }
            Block::BlockQuote { blocks, .. } | Block::Div { blocks, .. } => {
                self.blocks(blocks, &context.child(".blocks"), phase)
            }
            Block::Callout { title, blocks, .. } => {
                self.optional_inlines(title, &context.child(".title"), phase);
                self.blocks(blocks, &context.child(".blocks"), phase);
            }
            Block::List { items, .. } => {
                for (i, item) in items.iter_mut().enumerate() {
                    self.blocks(
                        &mut item.blocks,
                        &context.child(format_args!(".items[{i}]")),
                        phase,
                    );
                }
            }
            Block::DefinitionList { items, .. } => {
                for (i, item) in items.iter_mut().enumerate() {
                    let context = context.child(format_args!(".items[{i}]"));
                    self.inlines(
                        &mut item.term,
                        &context.child(".term"),
                        phase,
                    );
                    for (j, definition) in
                        item.definitions.iter_mut().enumerate()
                    {
                        self.blocks(
                            definition,
                            &context.child(format_args!(".definitions[{j}]")),
                            phase,
                        );
                    }
                }
            }
            Block::Figure { kind, caption, body, .. } => {
                if matches!(kind, FigureKind::Table | FigureKind::Listing) {
                    self.optional_inlines(
                        caption,
                        &context.child(".caption"),
                        phase,
                    );
                    self.blocks(body, &context.child(".body"), phase);
                } else {
                    self.blocks(body, &context.child(".body"), phase);
                    self.optional_inlines(
                        caption,
                        &context.child(".caption"),
                        phase,
                    );
                }
            }
            Block::Table { head, body, foot, .. } => {
                for (section, rows) in
                    [("head", head), ("body", body), ("foot", foot)]
                {
                    for (r, row) in rows.iter_mut().enumerate() {
                        for (c, cell) in row.cells.iter_mut().enumerate() {
                            self.inlines(
                                &mut cell.inlines,
                                &context.child(format_args!(
                                    ".{section}[{r}].cells[{c}]"
                                )),
                                phase,
                            );
                        }
                    }
                }
            }
            Block::Cell(cell) => self.cell(cell, context, phase),
            Block::CodeBlock { .. }
            | Block::Math { .. }
            | Block::Raw { .. }
            | Block::ThematicBreak { .. }
            | Block::PageBreak { .. }
            | Block::Toc { .. }
            | Block::Bibliography { .. } => {}
        }
    }

    fn cell(
        &mut self, cell: &mut crate::ir::Cell, context: &Context, phase: Phase,
    ) {
        for (output_index, output) in cell.outputs.iter_mut().enumerate() {
            let context =
                context.child(format_args!(".outputs[{output_index}]"));
            let snapshot = (self.counters.clone(), self.notes.clone());
            let mut primary_state = snapshot.clone();
            for (index, candidate) in output.candidates.iter_mut().enumerate() {
                if index > 0
                    && matches!(
                        phase,
                        Phase::Setup
                            | Phase::Labels
                            | Phase::AutoIds
                            | Phase::Assemble
                    )
                {
                    continue;
                }
                if phase == Phase::Number {
                    (self.counters, self.notes) = snapshot.clone();
                }
                let mut context =
                    context.child(format_args!(".candidates[{index}]"));
                context.primary &= index == 0;
                if phase == Phase::Finish {
                    self.notes = snapshot.1.clone();
                    if index > 0
                        && let Some(alternative) =
                            self.candidate_notes.get(&context.position)
                    {
                        self.notes = self.notes.with_alternative(alternative);
                    }
                }
                self.blocks(&mut candidate.blocks, &context, phase);
                if phase == Phase::Number && index > 0 {
                    self.candidate_notes
                        .insert(context.position.clone(), self.notes.clone());
                }
                if index == 0 {
                    primary_state = (self.counters.clone(), self.notes.clone());
                }
            }
            if phase == Phase::Number {
                (self.counters, self.notes) = primary_state;
            } else if phase == Phase::Finish {
                self.notes = snapshot.1;
            }
        }
    }

    fn optional_inlines(
        &mut self, inlines: &mut Option<Inlines>, context: &Context,
        phase: Phase,
    ) {
        if let Some(inlines) = inlines {
            self.inlines(inlines, context, phase);
        }
    }

    fn block_label(&mut self, block: &mut Block, context: &Context) {
        let (attrs, kind, text) = match block {
            Block::Heading { attrs, inlines, .. } => {
                (attrs, LabelKind::Heading, Some(label_text(inlines)))
            }
            Block::Figure { attrs, kind, caption, .. } => {
                let kind = match kind {
                    FigureKind::Image | FigureKind::Other => LabelKind::Figure,
                    FigureKind::Table => LabelKind::Table,
                    FigureKind::Listing => LabelKind::Listing,
                };
                (
                    attrs,
                    kind,
                    caption.as_ref().map(|inlines| label_text(inlines)),
                )
            }
            Block::Math { attrs, .. } => (attrs, LabelKind::Equation, None),
            Block::Callout { attrs, title, .. } => (
                attrs,
                LabelKind::Other,
                title.as_ref().map(|inlines| label_text(inlines)),
            ),
            Block::Div { attrs, .. }
            | Block::CodeBlock { attrs, .. }
            | Block::Table { attrs, .. } => (attrs, LabelKind::Other, None),
            Block::Cell(cell) => (&mut cell.attrs, LabelKind::Other, None),
            _ => return,
        };
        self.register_id(&mut attrs.id, kind, text, context);
    }

    fn register_id(
        &mut self, id: &mut Option<String>, kind: LabelKind,
        text: Option<String>, context: &Context,
    ) {
        if let Some(label) = id
            && !self.register(label, kind, text, context, true)
        {
            *id = None;
        }
    }

    fn register(
        &mut self, label: &str, kind: LabelKind, text: Option<String>,
        context: &Context, explicit: bool,
    ) -> bool {
        if !typst_syntax::is_valid_label_literal_id(label) {
            self.error("label-invalid", format!("label `{label}` is not valid (letters, digits, `_ - : .` only)"), context);
            return false;
        }
        self.taken.insert(label.into());
        if let Some(first) = self.labels.get(label) {
            let line =
                first.info.span.as_ref().map_or(1, |span| span.start_line);
            self.error("duplicate-label", format!("label `<{label}>` occurs multiple times in the document (first defined at {}:{line})", first.path.display()), context);
            return true;
        }
        if explicit && reserved(label) {
            self.warn("label-reserved", format!("label `<{label}>` is reserved for generated anchors (rename it)"), context);
        }
        let anchor = anchor(label);
        if let Some(first) = self.anchors.get(&anchor) {
            self.error("anchor-collision", format!("labels `<{first}>` and `<{label}>` map to the same anchor `{anchor}` (rename one)"), context);
        } else {
            self.anchors.insert(anchor, label.into());
        }
        self.labels.insert(
            label.into(),
            LabelRecord {
                info: LabelInfo {
                    kind,
                    number: None,
                    supplement: supplement(kind, &self.lang),
                    text,
                    span: context.span.clone(),
                },
                path: context.scope.clone(),
                identity: context.identity,
            },
        );
        true
    }

    fn set_number(
        &mut self, id: Option<&str>, number: Option<String>, context: &Context,
    ) {
        if context.primary
            && let Some(record) = id.and_then(|id| self.labels.get_mut(id))
            && record.identity == context.identity
        {
            record.info.number = number;
        }
    }

    fn number_block(&mut self, block: &mut Block, context: &Context) {
        match block {
            Block::Heading { level, attrs, number, .. } => {
                *number = None;
                let mut reference = None;
                if self.pattern.is_some()
                    && !context.abstract_
                    && !attrs.classes.iter().any(|c| c == "unnumbered")
                {
                    self.counters.heading(*level, self.section);
                    if let Some(pattern) = &self.pattern {
                        let (display, failures) = pattern
                            .apply_with_systems(&self.counters.headings, false);
                        reference = Some(
                            pattern.apply(&self.counters.headings, true).0,
                        );
                        *number = Some(display);
                        if let Some(&(n, system)) = failures.first() {
                            let message = if n == 0 {
                                format!(
                                    "the numeral system `{system}` cannot represent zero"
                                )
                            } else {
                                format!(
                                    "the number {n} is too large to be represented with the `{system}` numeral system"
                                )
                            };
                            self.warn(
                                "numbering-unrepresentable", message, context,
                            );
                        }
                    }
                }
                self.set_number(attrs.id.as_deref(), reference, context);
            }
            Block::Figure { kind, attrs, number, .. } => {
                *number = (!attrs.classes.iter().any(|c| c == "unnumbered"))
                    .then(|| self.counters.figure(*kind, self.section));
                self.set_number(attrs.id.as_deref(), number.clone(), context);
            }
            Block::Math { attrs, number, .. } => {
                let enabled = match self.opts.config.numbering.equations {
                    EquationNumbering::All => true,
                    EquationNumbering::Labelled => attrs.id.is_some(),
                    EquationNumbering::None => false,
                };
                *number = (enabled
                    && !attrs.classes.iter().any(|c| c == "unnumbered"))
                .then(|| self.counters.equation(self.section));
                self.set_number(attrs.id.as_deref(), number.clone(), context);
            }
            _ => {}
        }
    }

    fn toc_block(&mut self, block: &Block, context: &Context) {
        if context.abstract_ {
            return;
        }
        match block {
            Block::Heading { level, number, inlines, attrs, .. }
                if !attrs
                    .classes
                    .iter()
                    .any(|c| matches!(c.as_str(), "notoc" | "unlisted")) =>
            {
                if let Some(id) = &attrs.id {
                    self.toc.push(TocEntry {
                        level: *level,
                        number: number.clone(),
                        inlines: toc::without_footnotes(inlines),
                        anchor: id.clone(),
                    });
                }
            }
            // Bibliography entries are appended by T70's phase 4 before assembly.
            Block::Bibliography { .. } if self.has_bibliography => {
                self.toc.push(TocEntry {
                    level: 1,
                    number: None,
                    inlines: vec![text_inline(term(
                        &self.lang,
                        Term::Bibliography,
                    ))],
                    anchor: BIBLIOGRAPHY_ANCHOR.into(),
                })
            }
            _ => {}
        }
    }

    fn inlines(
        &mut self, inlines: &mut Inlines, context: &Context, phase: Phase,
    ) {
        if phase == Phase::Number {
            let mut classified = Vec::new();
            for (index, inline) in
                std::mem::take(inlines).into_iter().enumerate()
            {
                let context = inline_context(
                    &inline,
                    &context.child(format_args!("[{index}]")),
                    &self.source,
                );
                classified.extend(self.classify(inline, &context));
            }
            *inlines = merge_adjacent_citations(classified);
        }
        for (index, inline) in inlines.iter_mut().enumerate() {
            let context = inline_context(
                inline,
                &context.child(format_args!("[{index}]")),
                &self.source,
            );
            if phase == Phase::Labels {
                self.inline_label(inline, &context);
            }
            if phase == Phase::Number {
                self.number_inline(inline, &context);
            }
            self.inline_children(inline, &context, phase);
            if phase == Phase::Finish {
                self.finish_inline(inline, &context);
            }
        }
    }

    fn inline_label(&mut self, inline: &mut Inline, context: &Context) {
        match inline {
            Inline::Code { attrs, .. }
            | Inline::Link { attrs, .. }
            | Inline::Image { attrs, .. }
            | Inline::Span { attrs, .. } => {
                self.register_id(
                    &mut attrs.id,
                    LabelKind::Other,
                    None,
                    context,
                );
            }
            Inline::Footnote { label, .. } if context.typst => {
                self.register_id(label, LabelKind::Footnote, None, context)
            }
            _ => {}
        }
    }

    fn inline_children(
        &mut self, inline: &mut Inline, context: &Context, phase: Phase,
    ) {
        match inline {
            Inline::Emph { inlines }
            | Inline::Strong { inlines }
            | Inline::Strikeout { inlines }
            | Inline::Subscript { inlines }
            | Inline::Superscript { inlines }
            | Inline::Underline { inlines }
            | Inline::SmallCaps { inlines }
            | Inline::Link { inlines, .. }
            | Inline::Span { inlines, .. } => {
                self.inlines(inlines, &context.child(".inlines"), phase)
            }
            Inline::Footnote { blocks, .. } => {
                self.blocks(blocks, &context.child(".blocks"), phase)
            }
            Inline::Ref { supplement, .. } => self.optional_inlines(
                supplement,
                &context.child(".supplement"),
                phase,
            ),
            Inline::Cite { items, .. } => {
                for (index, item) in items.iter_mut().enumerate() {
                    self.optional_inlines(
                        &mut item.prefix,
                        &context.child(format_args!(".items[{index}].prefix")),
                        phase,
                    );
                    self.optional_inlines(
                        &mut item.suffix,
                        &context.child(format_args!(".items[{index}].suffix")),
                        phase,
                    );
                }
            }
            // Alt text is flattened at Finish without diagnostics or counter changes.
            Inline::Image { .. }
            | Inline::Text { .. }
            | Inline::Code { .. }
            | Inline::Math { .. }
            | Inline::FootnoteRef { .. }
            | Inline::LineBreak
            | Inline::SoftBreak
            | Inline::Raw { .. } => {}
        }
    }

    fn classify(&mut self, inline: Inline, context: &Context) -> Inlines {
        match inline {
            Inline::Ref { target, supplement, form, span, .. } => {
                self.classify_ref(target, supplement, form, span, context)
            }
            Inline::Cite { items, mode, span, .. } => {
                self.classify_cite(items, mode, span, context)
            }
            other => vec![other],
        }
    }

    fn classify_ref(
        &mut self, target: String, supplement: Option<Inlines>, form: RefForm,
        span: Option<SourceSpan>, context: &Context,
    ) -> Inlines {
        match (
            self.labels.get(&target).map(|record| record.info.kind),
            self.keys.contains(&target),
        ) {
            (Some(_), true) => {
                self.error("ambiguous-label", format!("label `<{target}>` occurs both in the document and a bibliography (rename the label or bibliography key)"), context);
                vec![placeholder(&[target])]
            }
            (Some(LabelKind::Footnote), false) => {
                if supplement.is_some() || form != RefForm::Normal {
                    self.invalid_mode(
                        "supplement or page", "a footnote reference", context,
                    );
                }
                // Keep its global-label identity until Finish. An ordinary
                // Markdown FootnoteRef with this spelling remains file-scoped.
                vec![Inline::Ref {
                    target,
                    supplement: None,
                    form: RefForm::Normal,
                    resolved: None,
                    span,
                }]
            }
            (Some(_), false) => vec![Inline::Ref {
                target,
                supplement,
                form,
                resolved: None,
                span,
            }],
            (None, true) => {
                if form == RefForm::Page {
                    self.invalid_mode("page", "a citation", context);
                }
                vec![Inline::Cite {
                    items: vec![CiteItem {
                        key: target,
                        locator: supplement
                            .filter(|s| !s.is_empty())
                            .map(|s| crate::ir::plain_text(&s)),
                        ..CiteItem::default()
                    }],
                    mode: CiteMode::Normal,
                    resolved: None,
                    span,
                }]
            }
            (None, false) => {
                self.unresolved(&target, context);
                vec![placeholder(&[target])]
            }
        }
    }

    fn classify_cite(
        &mut self, items: Vec<CiteItem>, mode: CiteMode,
        span: Option<SourceSpan>, context: &Context,
    ) -> Inlines {
        if items.is_empty() {
            return Vec::new();
        }
        let targets: Vec<_> =
            items.iter().map(|item| item.key.clone()).collect();
        let mut labels = false;
        let mut keys = false;
        let mut failed = false;
        for item in &items {
            match (
                self.labels.contains_key(&item.key),
                self.keys.contains(&item.key),
            ) {
                (true, true) => {
                    self.error("ambiguous-label", format!("label `<{}>` occurs both in the document and a bibliography", item.key), context);
                    failed = true;
                }
                (true, false) => labels = true,
                (false, true) => keys = true,
                (false, false) => {
                    self.unresolved(&item.key, context);
                    failed = true;
                }
            }
        }
        if labels && keys {
            self.error(
                "ambiguous-label",
                "citation group mixes labels and bibliography keys",
                context,
            );
            failed = true;
        }
        if failed {
            return vec![placeholder(&targets)];
        }
        if keys {
            return vec![Inline::Cite { items, mode, span, resolved: None }];
        }
        if mode != CiteMode::Normal {
            self.invalid_mode(
                &format!("{mode:?}").to_ascii_lowercase(),
                "a cross-reference",
                context,
            );
        }
        let mut out = Vec::new();
        for (index, item) in items.into_iter().enumerate() {
            if index > 0 {
                out.push(text_inline(", "));
            }
            if let Some(prefix) = item.prefix {
                out.extend(prefix);
                out.push(text_inline(" "));
            }
            out.extend(self.classify_ref(
                item.key,
                None,
                RefForm::Normal,
                span.clone(),
                context,
            ));
            if let Some(locator) = item.locator {
                out.push(text_inline(format!(", {locator}")));
            }
            if let Some(suffix) = item.suffix {
                out.push(text_inline(" "));
                out.extend(suffix);
            }
        }
        out
    }

    fn invalid_mode(&mut self, mode: &str, target: &str, context: &Context) {
        self.warn(
            "invalid-reference-mode",
            format!("`.{mode}` does not apply to {target}; ignored"),
            context,
        );
    }

    fn unresolved(&mut self, target: &str, context: &Context) {
        let hint = if self.opts.library.is_none() {
            format!("configure a bibliography to cite `{target}`")
        } else {
            format!("`{target}` is no bibliography key either")
        };
        let level = if self.opts.lenient {
            Level::Warning
        } else {
            Level::Error
        };
        self.report(
            level,
            "unresolved-reference",
            format!(
                "label `<{target}>` does not exist in the document ({hint})"
            ),
            context,
        );
    }

    fn number_inline(&mut self, inline: &mut Inline, context: &Context) {
        match inline {
            Inline::Footnote { blocks: _, label, number } => {
                let key = label.as_deref().map(|label| context.note(label));
                if let Some(existing) =
                    key.as_ref().and_then(|key| self.notes.lookup(key))
                {
                    *inline = Inline::FootnoteRef {
                        label: label.clone().unwrap_or_default(),
                        number: Some(existing),
                    };
                    return;
                }
                let next = self.notes.next(key);
                *number = Some(next);
                if context.typst
                    && context.primary
                    && let Some(record) = label
                        .as_deref()
                        .and_then(|label| self.labels.get_mut(label))
                {
                    record.info.number = Some(next.to_string());
                }
            }
            Inline::Cite { items, mode, .. } => {
                for subgroup in cite_sub_groups(items) {
                    let note_number = (self.note_style
                        && *mode == CiteMode::Normal)
                        .then(|| self.notes.next(None));
                    self.requests.push(OwnedRequest {
                        items: subgroup.to_vec(),
                        mode: *mode,
                        note_number,
                    });
                }
            }
            _ => {}
        }
    }

    fn render_citations(&mut self, _document: &mut Document) {
        if self.opts.library.is_none() || self.requests.is_empty() {
            return;
        }
        // T70 implements references.md §5 here: render these owned subgroups,
        // build note-style Footnotes using note_number, and set Cite.resolved.
        for request in &self.requests {
            let _ = (&request.items, request.mode, request.note_number);
        }
    }

    fn finish_inline(&mut self, inline: &mut Inline, context: &Context) {
        match inline {
            Inline::Ref { target, supplement, form, resolved, .. } => {
                if let Some(record) = self.labels.get(target) {
                    let info = &record.info;
                    if info.kind == LabelKind::Footnote {
                        let key = NoteKey::new(&context.scope, true, target);
                        if let Some(number) = self.notes.lookup(&key) {
                            *inline = Inline::FootnoteRef {
                                label: target.clone(),
                                number: Some(number),
                            };
                        } else {
                            self.unresolved(target, context);
                            *inline = placeholder(std::slice::from_ref(target));
                        }
                        return;
                    }
                    let warning = info.number.is_none()
                        && info.kind != LabelKind::Heading
                        && *form != RefForm::Page;
                    *resolved = Some(reference_value(
                        target,
                        info,
                        supplement.as_deref(),
                    ));
                    if warning {
                        self.warn("unnumbered-target", format!("cannot reference {:?} without numbering; it renders as its text (enable `[numbering]`)", info.kind).to_ascii_lowercase(), context);
                    }
                } else {
                    self.unresolved(target, context);
                    *inline = placeholder(std::slice::from_ref(target));
                }
            }
            Inline::FootnoteRef { label, number } => {
                if let Some(found) = self.notes.lookup(&context.note(label)) {
                    *number = Some(found);
                } else {
                    self.unresolved(label, context);
                    *inline = placeholder(std::slice::from_ref(label));
                }
            }
            Inline::Link { url, .. } => {
                if let Some(target) = url.strip_prefix('#')
                    && target != BIBLIOGRAPHY_ANCHOR
                    && !self.labels.contains_key(target)
                {
                    self.warn(
                        "link-target-missing",
                        format!(
                            "link target `{url}` does not exist in the document"
                        ),
                        context,
                    );
                }
            }
            Inline::Image { alt, .. } if special_alt(alt) => {
                *alt = vec![text_inline(self.alt_text(alt))]
            }
            _ => {}
        }
    }

    fn alt_text(&self, inlines: &[Inline]) -> String {
        let mut out = String::new();
        for inline in inlines {
            match inline {
                Inline::Ref { target, supplement, .. } => {
                    let text = self
                        .labels
                        .get(target)
                        .map(|record| {
                            let resolved = reference_value(
                                target,
                                &record.info,
                                supplement.as_deref(),
                            );
                            label_text(&resolved.text)
                        })
                        .unwrap_or_else(|| format!("[?{target}]"));
                    out.push_str(&text);
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
                    out.push_str(&self.alt_text(inlines))
                }
                Inline::Image { alt, .. } => out.push_str(&self.alt_text(alt)),
                other => out.push_str(&label_text(std::slice::from_ref(other))),
            }
        }
        out
    }
}

fn inline_context(
    inline: &Inline, context: &Context, source: &SourceInfo,
) -> Context {
    let span = match inline {
        Inline::Ref { span, .. } | Inline::Cite { span, .. } => span.as_ref(),
        _ => None,
    };
    // Inline spans locate diagnostics; lexical footnote scope remains the
    // nearest block's file, even when a hand-built inline carries another span.
    let mut located = context.at(span, source);
    located.scope = context.scope.clone();
    located.typst = context.typst;
    located
}

fn reference_value(
    target: &str, info: &LabelInfo, explicit: Option<&[Inline]>,
) -> ResolvedRef {
    let text = if let Some(number) = &info.number {
        match explicit {
            Some([]) => vec![text_inline(number)],
            Some(inlines) => {
                let mut inlines = inlines.to_vec();
                inlines.push(text_inline(format!("\u{a0}{number}")));
                merge_text(inlines)
            }
            None => vec![text_inline(if info.supplement.is_empty() {
                number.clone()
            } else {
                format!("{}\u{a0}{number}", info.supplement)
            })],
        }
    } else if let Some(inlines) = explicit.filter(|inlines| !inlines.is_empty())
    {
        inlines.to_vec()
    } else {
        let text = info
            .text
            .as_deref()
            .filter(|text| !text.is_empty())
            .or_else(|| {
                (!info.supplement.is_empty())
                    .then_some(info.supplement.as_str())
            })
            .unwrap_or(target);
        vec![text_inline(text)]
    };
    ResolvedRef {
        kind: info.kind,
        number: info.number.clone().unwrap_or_default(),
        text,
        anchor: target.into(),
    }
}

fn placeholder(targets: &[String]) -> Inline {
    Inline::Span {
        attrs: Attrs {
            classes: vec!["unresolved-reference".into()],
            ..Attrs::default()
        },
        inlines: vec![text_inline(format!("[?{}]", targets.join("; ")))],
    }
}

fn has_toc(document: &Document) -> bool {
    let mut found = false;
    crate::ir::walk_blocks(document, &mut |block| {
        found |= matches!(block, Block::Toc { .. })
    });
    found
}

fn document_features(inline: &Inline, features: &mut (bool, bool)) {
    features.0 |= matches!(inline, Inline::Cite { .. });
    features.1 |= matches!(inline, Inline::Footnote { .. });
}

fn special_alt(inlines: &[Inline]) -> bool {
    inlines.iter().any(|inline| match inline {
        Inline::Ref { .. }
        | Inline::Cite { .. }
        | Inline::Footnote { .. }
        | Inline::FootnoteRef { .. } => true,
        Inline::Emph { inlines }
        | Inline::Strong { inlines }
        | Inline::Strikeout { inlines }
        | Inline::Subscript { inlines }
        | Inline::Superscript { inlines }
        | Inline::Underline { inlines }
        | Inline::SmallCaps { inlines }
        | Inline::Link { inlines, .. }
        | Inline::Span { inlines, .. } => special_alt(inlines),
        Inline::Image { alt, .. } => special_alt(alt),
        _ => false,
    })
}

fn citation_space(inline: &Inline) -> bool {
    matches!(inline, Inline::SoftBreak)
        || matches!(inline, Inline::Text { text } if text.chars().all(|c| matches!(c, ' ' | '\t')))
}

fn merge_adjacent_citations(inlines: Inlines) -> Inlines {
    let mut out = Vec::new();
    let mut iter = inlines.into_iter().peekable();
    while let Some(mut inline) = iter.next() {
        if let Inline::Cite { items, mode, .. } = &mut inline {
            loop {
                let mut spaces = Vec::new();
                while iter.peek().is_some_and(citation_space) {
                    if let Some(space) = iter.next() {
                        spaces.push(space);
                    }
                }
                if matches!(iter.peek(), Some(Inline::Cite { mode: next_mode, .. }) if next_mode == mode)
                {
                    if let Some(Inline::Cite { items: next, .. }) = iter.next()
                    {
                        items.extend(next);
                    }
                } else {
                    out.push(inline);
                    out.extend(spaces);
                    break;
                }
            }
        } else {
            out.push(inline);
        }
    }
    out
}

fn cite_sub_groups(items: &[CiteItem]) -> Vec<&[CiteItem]> {
    let mut groups = Vec::new();
    let mut start = 0;
    for (index, item) in items.iter().enumerate() {
        if index > start && item.prefix.is_some() {
            groups.push(&items[start..index]);
            start = index;
        }
        if item.suffix.is_some() {
            groups.push(&items[start..=index]);
            start = index + 1;
        }
    }
    if start < items.len() {
        groups.push(&items[start..]);
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cite(keys: &[&str], mode: CiteMode) -> Inline {
        Inline::Cite {
            items: keys
                .iter()
                .map(|key| CiteItem {
                    key: (*key).into(),
                    ..CiteItem::default()
                })
                .collect(),
            mode,
            resolved: None,
            span: None,
        }
    }

    #[test]
    fn merge_adjacent_citations() {
        let first_span =
            SourceSpan { start_line: 4, end_line: 4, file: None, cell: None };
        let mut a = cite(&["a"], CiteMode::Normal);
        if let Inline::Cite { span, .. } = &mut a {
            *span = Some(first_span.clone());
        }
        let merged = super::merge_adjacent_citations(vec![
            a,
            text_inline(" \t"),
            Inline::SoftBreak,
            cite(&["b"], CiteMode::Normal),
            cite(&["c"], CiteMode::Normal),
        ]);
        assert!(matches!(merged.as_slice(), [Inline::Cite { items, span, .. }]
            if items.iter().map(|i| i.key.as_str()).collect::<Vec<_>>() == ["a", "b", "c"] && span.as_ref() == Some(&first_span)));
        for separator in
            [text_inline(", "), text_inline("\n"), Inline::LineBreak]
        {
            assert_eq!(
                super::merge_adjacent_citations(vec![
                    cite(&["a"], CiteMode::Normal),
                    separator,
                    cite(&["b"], CiteMode::Normal)
                ])
                .len(),
                3
            );
        }
        assert_eq!(
            super::merge_adjacent_citations(vec![
                cite(&["a"], CiteMode::Normal),
                text_inline(" "),
                cite(&["b"], CiteMode::Prose)
            ])
            .len(),
            3
        );
    }

    #[test]
    fn cite_sub_groups() {
        let a = CiteItem { key: "doe2020".into(), ..CiteItem::default() };
        let b = CiteItem { key: "lee2019".into(), ..CiteItem::default() };
        let see = Some(vec![text_inline("see")]);
        let mut single = a.clone();
        single.key = "knuth1984".into();
        single.prefix = see.clone();
        single.locator = Some("p. 12".into());
        assert_eq!(super::cite_sub_groups(&[single]).len(), 1);
        let mut first = a.clone();
        first.prefix = see.clone();
        assert_eq!(super::cite_sub_groups(&[first, b.clone()]).len(), 1);
        let mut second = b.clone();
        second.prefix = see;
        assert_eq!(super::cite_sub_groups(&[a.clone(), second]).len(), 2);
        let mut first = a;
        first.suffix = Some(vec![text_inline("after")]);
        assert_eq!(super::cite_sub_groups(&[first, b]).len(), 2);
    }

    fn document(blocks: Vec<Block>) -> Document {
        Document {
            meta: crate::ir::DocMeta::default(),
            blocks,
            source: SourceInfo {
                path: "/docs/main.md".into(),
                kind: SourceKind::Markdown,
                base_dir: "/docs".into(),
            },
            assets: Vec::new(),
            includes: Vec::new(),
            features: crate::ir::Features::default(),
            resolved: Resolved::default(),
        }
    }

    fn heading(id: &str) -> Block {
        Block::Heading {
            level: 1,
            inlines: vec![text_inline(id)],
            attrs: Attrs { id: Some(id.into()), ..Attrs::default() },
            number: None,
            span: None,
        }
    }

    fn note(label: &str, blocks: Vec<Block>) -> Inline {
        Inline::Footnote { blocks, label: Some(label.into()), number: None }
    }

    #[test]
    fn classification_expansion_keeps_nested_label_numbers() {
        let mut item = CiteItem { key: "known".into(), ..CiteItem::default() };
        item.prefix = Some(vec![text_inline("see")]);
        let mut document = document(vec![
            heading("known"),
            Block::Paragraph {
                inlines: vec![
                    Inline::Cite {
                        items: vec![item],
                        mode: CiteMode::Normal,
                        resolved: None,
                        span: None,
                    },
                    note("n", vec![heading("nested")]),
                ],
                span: None,
            },
        ]);
        let mut config = Config::default();
        config.numbering.headings = HeadingNumbering::Pattern("1".into());
        let diagnostics = resolve(
            &mut document,
            &ResolveOptions { config: &config, library: None, lenient: false },
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(
            document.resolved.labels["nested"].number.as_deref(),
            Some("2")
        );
    }

    #[test]
    fn global_typst_note_refs_preserve_markdown_name_scopes() {
        let mut document = document(vec![
            Block::Paragraph {
                inlines: vec![note("n", Vec::new())],
                span: Some(SourceSpan {
                    start_line: 1,
                    end_line: 1,
                    cell: None,
                    file: Some("/docs/included.typ".into()),
                }),
            },
            Block::Paragraph {
                inlines: vec![
                    note("n", Vec::new()),
                    Inline::FootnoteRef { label: "n".into(), number: None },
                    Inline::Ref {
                        target: "n".into(),
                        supplement: None,
                        form: RefForm::Normal,
                        resolved: None,
                        span: None,
                    },
                    cite(&["n"], CiteMode::Normal),
                ],
                span: None,
            },
        ]);
        let diagnostics = resolve(
            &mut document,
            &ResolveOptions {
                config: &Config::default(),
                library: None,
                lenient: false,
            },
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(document.resolved.labels["n"].number.as_deref(), Some("1"));
        let Block::Paragraph { inlines, .. } = &document.blocks[1] else {
            panic!("paragraph expected")
        };
        assert!(matches!(
            inlines[1],
            Inline::FootnoteRef { number: Some(2), .. }
        ));
        assert!(matches!(
            inlines[2],
            Inline::FootnoteRef { number: Some(1), .. }
        ));
        assert!(matches!(
            inlines[3],
            Inline::FootnoteRef { number: Some(1), .. }
        ));
    }

    #[test]
    fn empty_citation_does_not_leave_unresolved_ir() {
        let mut document = document(vec![Block::Paragraph {
            inlines: vec![cite(&[], CiteMode::Normal)],
            span: None,
        }]);
        let diagnostics = resolve(
            &mut document,
            &ResolveOptions {
                config: &Config::default(),
                library: None,
                lenient: false,
            },
        );
        assert!(diagnostics.is_empty());
        assert!(!document.features.citations);
        crate::ir::walk_inlines(&document, &mut |inline| {
            assert!(!matches!(inline, Inline::Cite { resolved: None, .. }));
        });
    }

    #[test]
    fn reference_payloads_clone_finished_supplements() {
        let reference = |supplement| Inline::Ref {
            target: "target".into(),
            supplement,
            form: RefForm::Normal,
            resolved: None,
            span: None,
        };
        let mut document = document(vec![
            heading("target"),
            Block::Paragraph {
                inlines: vec![
                    note("n", Vec::new()),
                    reference(Some(vec![
                        reference(None),
                        text_inline(" "),
                        Inline::FootnoteRef { label: "n".into(), number: None },
                    ])),
                ],
                span: None,
            },
        ]);
        let mut config = Config::default();
        config.numbering.headings = HeadingNumbering::Pattern("1".into());
        let diagnostics = resolve(
            &mut document,
            &ResolveOptions { config: &config, library: None, lenient: false },
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let Block::Paragraph { inlines, .. } = &document.blocks[1] else {
            panic!("paragraph expected")
        };
        let Inline::Ref { resolved: Some(resolved), .. } = &inlines[1] else {
            panic!("resolved ref expected")
        };
        assert!(
            matches!(&resolved.text[0], Inline::Ref { resolved: Some(nested), .. }
            if crate::ir::plain_text(&nested.text) == "Section\u{a0}1")
        );
        assert!(matches!(
            &resolved.text[2],
            Inline::FootnoteRef { number: Some(1), .. }
        ));
    }
}
