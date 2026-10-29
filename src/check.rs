//! Document lints and presenter compatibility (markdown-dialect.md §9–10).
//! Frontend and resolver findings are reused; target packages are discarded.
//! Error conversion and position ordering are adapted from presenter_rs/check.rs.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use comrak::nodes::{AstNode, NodeValue};
use comrak::{Arena, parse_document};
use serde_json::Value;

use crate::config::{Config, HtmlMath, Target};
use crate::error::{Diagnostic, Diagnostics, Error, Level, Result};
use crate::ir::{
    Block, Cell, Document, Inline, LabelInfo, MathDialect, RefForm, SourceKind,
    SourceSpan, detect_kind,
};
use crate::markdown::directives::{
    DIRECTIVE_COLON_REWRITTEN, colon_prepass, parse_directive_info,
};
use crate::markdown::frontmatter::split_front_matter;
use crate::math::latex_mathml::{self, MathError};
use crate::math::latex_source;
use crate::notebook::nbformat::{self, CellType};
use crate::pipeline::{self, BuildFailure, BuildOptions, Mode};

mod targets;
mod visible;

/// Optional checks beyond the frontend and reference resolver.
#[derive(Debug, Clone, Copy, Default)]
pub struct CheckOptions {
    /// Warn about source syntax that presenter reads differently.
    pub compat_presenter: bool,
}

/// Checks a document and every included file without producing output.
///
/// Resolution is always lenient, regardless of the supplied build mode. A
/// stopped preparation retains every finding collected before its failure.
pub fn run(
    opts: &BuildOptions, check: &CheckOptions,
) -> std::result::Result<Diagnostics, BuildFailure> {
    let mut opts = opts.clone();
    opts.mode = Mode::Check;
    let prepared = pipeline::prepare(&opts)?;
    let mut diagnostics = prepared.diagnostics.clone().into_vec();
    let math = math_diagnostics(&prepared.document, &prepared.config);
    diagnostics.extend(pipeline::escalate_strict(
        math.into(),
        opts.cli.strict,
        &prepared.config,
    ));
    diagnostics.extend(pipeline::escalate_strict(
        targets::run(&prepared, &opts).into(),
        opts.cli.strict,
        &prepared.config,
    ));
    if check.compat_presenter {
        for diagnostic in &mut diagnostics {
            if diagnostic.code == DIRECTIVE_COLON_REWRITTEN {
                diagnostic.level = Level::Warning;
            }
        }
        let root = &prepared.document.source;
        let mut seen = BTreeSet::new();
        for path in
            std::iter::once(&root.path).chain(&prepared.document.includes)
        {
            let identity =
                std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            if !seen.insert(identity) {
                continue;
            }
            let result = source_compatibility(
                path,
                if path == &root.path {
                    root.kind
                } else {
                    detect_kind(path).unwrap_or(SourceKind::Markdown)
                },
                &prepared.config,
                &mut diagnostics,
            );
            if let Err(error) = result {
                return Err(BuildFailure {
                    error: Box::new(error),
                    diagnostics: diagnostics.into(),
                });
            }
        }
    }
    sort_by_position(
        &mut diagnostics, &prepared.document.source.path,
        &prepared.document.includes,
    );
    Ok(diagnostics.into())
}

/// A source equation, with the union of outputs that selected this IR node.
struct CheckedMath<'a> {
    source: &'a str,
    display: bool,
    baseline: bool,
    location: MathLocation,
    targets: Vec<Target>,
}

/// Borrowed node addresses distinguish equal or empty sources at different
/// positions. These keys are never dereferenced and stay local to one check.
#[derive(PartialEq, Eq, Hash)]
enum MathIdentity {
    Block(*const Block),
    Inline(*const Inline),
}

#[derive(Clone)]
struct MathLocation {
    path: PathBuf,
    span: Option<SourceSpan>,
}

impl MathLocation {
    fn nested(&self, span: Option<&SourceSpan>) -> Self {
        let Some(span) = span else {
            return self.clone();
        };
        let mut span = span.clone();
        if let Some(parent) = &self.span {
            let same_file = span
                .file
                .as_deref()
                .is_none_or(|file| file == self.path.as_path());
            if same_file {
                span.cell = span.cell.or(parent.cell);
            }
            span.file = span.file.or_else(|| parent.file.clone());
        }
        Self {
            path: span.file.clone().unwrap_or_else(|| self.path.clone()),
            span: Some(span),
        }
    }

    fn attach(&self, mut diagnostic: Diagnostic) -> Diagnostic {
        diagnostic.path = Some(self.path.clone());
        diagnostic.span = self.span.clone();
        diagnostic
    }
}

#[derive(Clone, Copy)]
struct MathVisibility<'a> {
    targets: &'a [Target],
    baseline: bool,
}

#[derive(Default)]
struct MathCollector<'a> {
    labels: Option<&'a BTreeMap<String, LabelInfo>>,
    equations: Vec<CheckedMath<'a>>,
    seen: HashMap<MathIdentity, usize>,
}

impl<'a> MathCollector<'a> {
    fn equation(
        &mut self, identity: MathIdentity, source: &'a str,
        dialect: MathDialect, display: bool, location: &MathLocation,
        view: MathVisibility<'_>,
    ) {
        if dialect != MathDialect::Latex
            || !view.baseline
                && !view
                    .targets
                    .iter()
                    .any(|target| matches!(target, Target::Pdf | Target::Typst))
        {
            return;
        }
        if let Some(&index) = self.seen.get(&identity) {
            let previous = &mut self.equations[index];
            previous.baseline |= view.baseline;
            for target in view.targets {
                if !previous.targets.contains(target) {
                    previous.targets.push(*target);
                }
            }
            return;
        }
        self.seen.insert(identity, self.equations.len());
        self.equations.push(CheckedMath {
            source,
            display,
            baseline: view.baseline,
            location: location.clone(),
            targets: view.targets.to_vec(),
        });
    }

    fn blocks(
        &mut self, blocks: &'a [Block], inherited: &MathLocation,
        view: MathVisibility<'_>,
    ) {
        for block in blocks {
            let location = inherited.nested(block.span());
            match block {
                Block::Heading { inlines, .. }
                | Block::Paragraph { inlines, .. } => {
                    self.inlines(inlines, &location, view);
                }
                Block::Math { source, dialect, .. } => self.equation(
                    MathIdentity::Block(std::ptr::from_ref(block)),
                    source,
                    *dialect,
                    true,
                    &location,
                    view,
                ),
                Block::BlockQuote { blocks, .. }
                | Block::Div { blocks, .. } => {
                    self.blocks(blocks, &location, view);
                }
                Block::List { items, .. } => {
                    for item in items {
                        self.blocks(&item.blocks, &location, view);
                    }
                }
                Block::DefinitionList { items, .. } => {
                    for item in items {
                        self.inlines(&item.term, &location, view);
                        for definition in &item.definitions {
                            self.blocks(definition, &location, view);
                        }
                    }
                }
                Block::Figure { caption, body, .. } => {
                    if let Some(caption) = caption {
                        self.inlines(caption, &location, view);
                    }
                    self.blocks(body, &location, view);
                }
                Block::Table { head, body, foot, .. } => {
                    for row in head.iter().chain(body).chain(foot) {
                        for cell in &row.cells {
                            self.inlines(&cell.inlines, &location, view);
                        }
                    }
                }
                Block::Callout { title, blocks, .. } => {
                    if let Some(title) = title {
                        self.inlines(title, &location, view);
                    }
                    self.blocks(blocks, &location, view);
                }
                Block::Cell(cell) => self.cell(cell, &location, view),
                _ => {}
            }
        }
    }

    fn inlines(
        &mut self, inlines: &'a [Inline], location: &MathLocation,
        view: MathVisibility<'_>,
    ) {
        for inline in inlines {
            match inline {
                Inline::Math { source, dialect } => self.equation(
                    MathIdentity::Inline(std::ptr::from_ref(inline)),
                    source,
                    *dialect,
                    false,
                    location,
                    view,
                ),
                Inline::Emph { inlines }
                | Inline::Strong { inlines }
                | Inline::Strikeout { inlines }
                | Inline::Subscript { inlines }
                | Inline::Superscript { inlines }
                | Inline::Underline { inlines }
                | Inline::SmallCaps { inlines }
                | Inline::Link { inlines, .. }
                | Inline::Span { inlines, .. } => {
                    self.inlines(inlines, location, view);
                }
                Inline::Image { alt, .. } => {
                    let targets = without_pdf(view.targets);
                    self.inlines(
                        alt,
                        location,
                        MathVisibility { targets: &targets, ..view },
                    );
                }
                Inline::Footnote { blocks, .. } => {
                    self.blocks(blocks, location, view);
                }
                Inline::Ref { target, supplement, form, resolved, .. } => {
                    self.reference(
                        target,
                        supplement.as_deref(),
                        *form,
                        resolved.as_ref(),
                        location,
                        view,
                    );
                }
                Inline::Cite { items, resolved, .. } => {
                    let targets = if resolved.is_some() {
                        view.targets.to_vec()
                    } else {
                        without_pdf(view.targets)
                    };
                    let view = MathVisibility { targets: &targets, ..view };
                    for item in items {
                        if let Some(prefix) = &item.prefix {
                            self.inlines(prefix, location, view);
                        }
                        if let Some(suffix) = &item.suffix {
                            self.inlines(suffix, location, view);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn reference(
        &mut self, target: &str, supplement: Option<&'a [Inline]>,
        form: RefForm, resolved: Option<&'a crate::ir::ResolvedRef>,
        location: &MathLocation, view: MathVisibility<'_>,
    ) {
        let info = self.labels.and_then(|labels| labels.get(target));
        let unnumbered = form != RefForm::Page
            && info.is_some_and(|info| info.number.is_none());
        let targets = if info.is_some() && !unnumbered {
            view.targets.to_vec()
        } else {
            without_pdf(view.targets)
        };
        if let Some(supplement) = supplement {
            self.inlines(
                supplement,
                location,
                MathVisibility { targets: &targets, ..view },
            );
        }
        if unnumbered
            && view.targets.contains(&Target::Pdf)
            && let Some(resolved) = resolved
        {
            // Full PDF emission uses this generated payload only for an
            // unnumbered link. Its source nodes are checked separately.
            self.inlines(
                &resolved.text,
                location,
                MathVisibility { targets: &[Target::Pdf], baseline: false },
            );
        }
    }

    fn cell(
        &mut self, cell: &'a Cell, location: &MathLocation,
        view: MathVisibility<'_>,
    ) {
        if !cell.show_output {
            return;
        }
        for output in &cell.outputs {
            let picked: Vec<_> = view
                .targets
                .iter()
                .map(|&target| (target, output.pick(math_target_name(target))))
                .collect();
            // Walk the union in candidate priority order. The target's chosen
            // borrowed slice identifies the candidate, never its source text.
            for candidate in &output.candidates {
                let selected: Vec<_> = picked
                    .iter()
                    .filter(|(_, blocks)| {
                        std::ptr::eq(*blocks, candidate.blocks.as_slice())
                    })
                    .map(|(target, _)| *target)
                    .collect();
                if !selected.is_empty() {
                    self.blocks(
                        &candidate.blocks,
                        location,
                        MathVisibility { targets: &selected, ..view },
                    );
                }
            }
        }
    }
}

fn without_pdf(targets: &[Target]) -> Vec<Target> {
    targets.iter().copied().filter(|target| *target != Target::Pdf).collect()
}

fn math_target_name(target: Target) -> &'static str {
    match target {
        Target::Html => "html",
        Target::Pdf | Target::Typst => "typst",
        Target::Docx => "docx",
        Target::Latex => "latex",
    }
}

fn math_diagnostics(document: &Document, config: &Config) -> Vec<Diagnostic> {
    let targets = config.targets_for(document.source.kind);
    let location =
        MathLocation { path: document.source.path.clone(), span: None };
    let mut collector = MathCollector {
        labels: Some(&document.resolved.labels),
        ..MathCollector::default()
    };
    let view = MathVisibility { targets: &targets, baseline: true };
    if let Some(abstract_) = &document.meta.abstract_ {
        collector.blocks(abstract_, &location, view);
    }
    collector.blocks(&document.blocks, &location, view);
    // Source traversal owns baseline checks. Backend visibility uses the actual
    // emitted branch, including table clipping and resolved inline payloads.
    for equation in &mut collector.equations {
        equation
            .targets
            .retain(|target| !matches!(target, Target::Pdf | Target::Typst));
    }
    for target in targets
        .iter()
        .copied()
        .filter(|target| matches!(target, Target::Pdf | Target::Typst))
    {
        visible::walk(document, target, config, |node, location| {
            let (identity, source, dialect, display) = match node {
                visible::Node::Block(
                    block @ Block::Math { source, dialect, .. },
                ) => (
                    MathIdentity::Block(std::ptr::from_ref(block)),
                    source.as_str(),
                    *dialect,
                    true,
                ),
                visible::Node::Inline(
                    inline @ Inline::Math { source, dialect },
                ) => (
                    MathIdentity::Inline(std::ptr::from_ref(inline)),
                    source.as_str(),
                    *dialect,
                    false,
                ),
                _ => return,
            };
            collector.equation(
                identity,
                source,
                dialect,
                display,
                location,
                MathVisibility { targets: &[target], baseline: false },
            );
        });
    }
    let macros = latex_source::macros_from_config(&config.math.katex_options);
    let mut diagnostics = Vec::new();
    let mut macros_reported = false;
    for equation in &collector.equations {
        if equation.baseline && !macros_reported {
            macros_reported = true;
            diagnostics.extend(
                latex_mathml::macro_diagnostics(&macros)
                    .into_iter()
                    .map(|finding| equation.location.attach(finding)),
            );
        }
        validate_math(equation, &targets, config, &macros, &mut diagnostics);
    }
    diagnostics
}

fn validate_math(
    equation: &CheckedMath<'_>, selected: &[Target], config: &Config,
    macros: &BTreeMap<String, String>, diagnostics: &mut Vec<Diagnostic>,
) {
    let (prepared, removed) = latex_source::prepare(equation.source);
    let backend = selected.iter().find_map(|target| {
        if !equation.targets.contains(target) {
            return None;
        }
        match target {
            Target::Pdf => Some("pdf"),
            Target::Typst => Some("typst"),
            _ => None,
        }
    });
    let tag_target = if backend.is_some() {
        backend
    } else {
        selected.iter().find_map(|target| {
            if !equation.targets.contains(target) {
                return None;
            }
            match target {
                Target::Html => Some("html"),
                Target::Docx => Some("docx"),
                _ => None,
            }
        })
    };
    if removed && let Some(target) = tag_target {
        diagnostics.push(equation.location.attach(Diagnostic::warning(
            "math-tag-ignored",
            format!("\\tag/\\label inside math are ignored for {target}; label the equation with {{#eq-x}}"),
        )));
    }
    if equation.baseline
        && let Err(error) =
            latex_mathml::to_mathml(&prepared, equation.display, macros)
    {
        let mut message = match error {
            MathError::Syntax { message, range, .. } => {
                let position = range.map_or_else(String::new, |range| {
                    format!(
                        " at bytes {}..{} of the equation",
                        range.start, range.end
                    )
                });
                format!("invalid LaTeX math: {message}{position}")
            }
            error => format!("invalid LaTeX math: {error}"),
        };
        if config.math.html == HtmlMath::Katex
            && equation.targets.contains(&Target::Html)
        {
            message.push_str("; KaTeX may still render it");
        }
        diagnostics.push(
            equation
                .location
                .attach(Diagnostic::warning("math-syntax", message)),
        );
    }
    if let Some(target) = backend {
        let (prepared, _) = latex_source::for_typst(equation.source, macros);
        if let Err(reason) = latex_source::preflight_typst(&prepared) {
            diagnostics.push(equation.location.attach(Diagnostic::warning(
                "math-conversion-failed",
                format!("{target}: cannot convert equation ({reason}); used the source text"),
            )));
        }
    }
}

/// Converts a stopped check to a located error finding for text or JSON reports.
pub fn error_diagnostic(error: &Error, input: &Path) -> Diagnostic {
    let code = match error {
        Error::Parse { .. } | Error::FrontMatter(_) | Error::Json(_) => {
            "parse-error"
        }
        Error::Config(_) => "config-error",
        Error::Io(_) => "io-error",
        Error::MissingAsset { .. } => "missing-asset",
        Error::Theme { .. } => "theme-error",
        Error::Template { .. } => "template-error",
        Error::Style { .. } => "style-error",
        Error::Compile { .. } => "compile-error",
        Error::Resource { .. } => "resource-error",
        Error::Other(_) => "error",
    };
    let span = error.span();
    let path = error
        .path()
        .or_else(|| span.as_ref().and_then(|span| span.file.as_deref()))
        .unwrap_or(input);
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut diagnostic =
        Diagnostic::error(code, error.message().into_owned()).with_path(path);
    diagnostic.span = span;
    diagnostic
}

/// Stable source order, adapted from presenter's check implementation.
fn sort_by_position(
    diagnostics: &mut [Diagnostic], root: &Path, includes: &[PathBuf],
) {
    diagnostics.sort_by_key(|diagnostic| {
        let position =
            diagnostic.span.as_ref().map(|span| (span.cell, span.start_line));
        let path = diagnostic.path.as_deref()?;
        if path == root {
            return Some((0, position));
        }
        let index = includes.iter().position(|included| included == path)?;
        Some((index + 1, position))
    });
}

fn source_compatibility(
    path: &Path, kind: SourceKind, config: &Config,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    let text = std::fs::read_to_string(path).map_err(|error| {
        Error::Io(std::io::Error::new(
            error.kind(),
            format!(
                "cannot read {} for compatibility checks: {error}",
                path.display()
            ),
        ))
    })?;
    if kind != SourceKind::Notebook {
        author_list_compatibility(path, &text, None, diagnostics);
        markdown_compatibility(path, &text, config, None, true, diagnostics);
        return Ok(());
    }
    let (notebook, _) = nbformat::parse_notebook(path, &text)?;
    let first = notebook
        .cells
        .iter()
        .position(|cell| cell.cell_type == CellType::Markdown);
    for (index, cell) in notebook.cells.iter().enumerate() {
        let raw_markdown = cell.cell_type == CellType::Raw
            && matches!(
                cell.raw_mimetype()
                    .unwrap_or("")
                    .split(';')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase()
                    .as_str(),
                "markdown" | "text/markdown" | "text/x-markdown"
            );
        if cell.cell_type == CellType::Markdown || raw_markdown {
            let first_header = first == Some(index);
            let header = cell.cell_type == CellType::Markdown
                && (first_header
                    || split_front_matter(cell.source.as_str()).is_some_and(
                        |(yaml, _, _)| {
                            crate::notebook::convert::later_metadata_header(
                                yaml,
                            )
                        },
                    ));
            if first_header {
                author_list_compatibility(
                    path,
                    cell.source.as_str(),
                    Some(index),
                    diagnostics,
                );
            }
            markdown_compatibility(
                path,
                cell.source.as_str(),
                config,
                Some(index),
                header,
                diagnostics,
            );
        }
    }
    Ok(())
}

fn markdown_compatibility(
    path: &Path, text: &str, config: &Config, cell: Option<usize>,
    front_matter: bool, diagnostics: &mut Vec<Diagnostic>,
) {
    let mut options = crate::markdown::comrak_options(&config.markdown);
    options.extension.front_matter_delimiter =
        front_matter.then(|| "---".into());
    let arena = Arena::new();
    let (rewritten, _) = colon_prepass(text, &options);
    let root = parse_document(&arena, &rewritten, &options);
    let lines: Vec<_> = text.lines().collect();
    let nodes: Vec<_> =
        root.children().filter(|node| significant(node)).collect();
    let boundary = |node: &AstNode<'_>| {
        let data = node.data();
        matches!(data.value, NodeValue::ThematicBreak)
            && lines
                .get(data.sourcepos.start.line - 1)
                .is_some_and(|line| line.trim() == "---")
    };
    for (index, node) in nodes.iter().enumerate() {
        let data = node.data();
        if !matches!(&data.value, NodeValue::BlockDirective(directive)
            if parse_directive_info(&directive.info).name == "include")
        {
            continue;
        }
        let before = index == 0 || boundary(nodes[index - 1]);
        let after = index + 1 == nodes.len() || boundary(nodes[index + 1]);
        if before && after && !node.children().any(|node| significant(node)) {
            continue;
        }
        diagnostics.push(
            Diagnostic::warning(
                "presenter-include-position",
                "presenter requires an include to be the only content between `---` rules; put a rule before and after it",
            )
            .with_path(path)
            .with_span(SourceSpan {
                start_line: data.sourcepos.start.line,
                end_line: data.sourcepos.end.line,
                cell,
                file: None,
            }),
        );
    }
}

fn significant(node: &AstNode<'_>) -> bool {
    match &node.data().value {
        NodeValue::FrontMatter(_) | NodeValue::FootnoteDefinition(_) => false,
        NodeValue::HtmlBlock(html) => !only_comments(&html.literal),
        NodeValue::BlockDirective(directive) => !matches!(
            parse_directive_info(&directive.info).name.as_str(),
            "notes" | "comment"
        ),
        _ => true,
    }
}

fn only_comments(mut text: &str) -> bool {
    while let Some(rest) = text.trim_start().strip_prefix("<!--") {
        let Some(end) = rest.find("-->") else {
            return true;
        };
        text = &rest[end + 3..];
    }
    text.trim().is_empty()
}

fn author_list_compatibility(
    path: &Path, text: &str, cell: Option<usize>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some((yaml, _, closing)) = split_front_matter(text) else {
        return;
    };
    let Ok(Value::Object(meta)) = serde_norway::from_str(yaml) else {
        return;
    };
    let Some(key) = ["author", "authors"]
        .into_iter()
        .find(|key| meta.get(*key).is_some_and(Value::is_array))
    else {
        return;
    };
    let mut offset = 0;
    let line = yaml.split_inclusive('\n').position(|line| {
        let prefix = &yaml[..offset];
        offset += line.len();
        !line.starts_with([' ', '\t'])
            && line.split_once(':').is_some_and(|(name, _)| {
                name.trim_end().trim_matches(['\'', '"']) == key
            })
            && matches!(
                serde_norway::from_str::<Value>(prefix),
                Ok(Value::Null | Value::Object(_))
            )
    });
    diagnostics.push(
        Diagnostic::warning(
            "presenter-front-matter",
            "presenter accepts a scalar `author`, not an author list",
        )
        .with_path(path)
        .with_span(SourceSpan {
            start_line: line.map_or(1, |line| line + 2),
            end_line: line.map_or(closing, |line| line + 2),
            cell,
            file: None,
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn compatibility(text: &str) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        author_list_compatibility(
            Path::new("source.md"),
            text,
            None,
            &mut diagnostics,
        );
        markdown_compatibility(
            Path::new("source.md"),
            text,
            &Config::default(),
            None,
            true,
            &mut diagnostics,
        );
        diagnostics
    }

    fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        for (name, text) in files {
            let path = directory.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        directory
    }

    #[test]
    fn include_positions_use_real_rules_and_file_edges() {
        for text in [
            ":::include part.md\n:::\n",
            "---\n:::include part.md\n:::\n",
            ":::include part.md\n:::\n\n---\n",
            "Before.\n\n---\n\n:::include part.md\n:::\n\n---\n\nAfter.",
            "<!-- comment -->\n\n:::notes\nIgnored.\n:::\n\n:::include part.md\n:::\n\n:::comment\nIgnored.\n:::\n",
            "---\ntitle: Header\n---\n\n:::include part.md\n:::\n",
        ] {
            assert!(compatibility(text).is_empty(), "{text}");
        }
        for text in [
            "Before.\n\n:::include part.md\n:::\n",
            ":::include part.md\n:::\n\nAfter.",
            ":::include part.md\nBody.\n:::\n",
            "Title\n---\n\n:::include part.md\n:::\n",
            "```md\n---\n```\n\n:::include part.md\n:::\n",
            "* * *\n\n:::include part.md\n:::\n",
            "- ---\n\n:::include part.md\n:::\n",
            "<div>\nVisible HTML.\n</div>\n\n:::include part.md\n:::\n",
        ] {
            let diagnostics = compatibility(text);
            assert_eq!(diagnostics.len(), 1, "{text}: {diagnostics:?}");
            assert_eq!(diagnostics[0].code, "presenter-include-position");
        }
    }

    #[test]
    fn compatibility_ignores_nested_includes_and_code_or_notes() {
        let text = "> :::include part.md\n> :::\n\n- :::include part.md\n  :::\n\n:::note\n:::include part.md\n:::\n\n```md\n:::include part.md\n:::\n```\n\n<!-- :::include part.md -->\n\n:::notes\n:::include part.md\n:::\n";
        assert!(compatibility(text).is_empty());
        for text in [
            "![A figure](image.png)\n\nTable: Caption\n\n@fig-x\n",
            ":::toc\n:::\n\n```{=latex}\n\\clearpage\n```\n\nInline ^[note].",
        ] {
            assert!(compatibility(text).is_empty());
        }
    }

    #[test]
    fn author_compatibility_distinguishes_scalars_and_both_list_keys() {
        assert!(compatibility("---\nauthor: Ada\n---\n\nText.").is_empty());
        for field in [
            "author: [Ada, Grace]", "authors: [Ada]", "authors: []",
            "'authors':\n  - name: Ada",
        ] {
            let diagnostics = compatibility(&format!("---\n{field}\n---\n"));
            assert_eq!(diagnostics.len(), 1, "{field}");
            assert_eq!(diagnostics[0].code, "presenter-front-matter");
            assert_eq!(diagnostics[0].span.as_ref().unwrap().start_line, 2);
        }
    }

    #[test]
    fn author_locations_skip_nested_keys_and_fall_back_for_complex_headers() {
        let diagnostics = compatibility(
            "---\ndescription: |\n  authors: literal text\ncustom:\n  authors: [Nested]\nauthors: [Ada]\n---\n",
        );
        assert_eq!(diagnostics.len(), 1);
        let span = diagnostics[0].span.as_ref().unwrap();
        assert_eq!((span.start_line, span.end_line), (6, 6));
        for yaml in ["  authors: [Ada]", "{authors: [Ada], title: Flow}"] {
            let diagnostics = compatibility(&format!("---\n{yaml}\n---\n"));
            assert_eq!(diagnostics.len(), 1);
            let span = diagnostics[0].span.as_ref().unwrap();
            assert_eq!((span.start_line, span.end_line), (1, 3));
        }
    }

    #[test]
    fn checks_force_lenient_resolution_and_leave_outputs_unwritten() {
        let directory = project(&[(
            "main.md",
            "See @missing.\n\n![Missing image](missing.png)\n",
        )]);
        let opts = BuildOptions::new(directory.path().join("main.md"));
        let diagnostics = run(&opts, &CheckOptions::default()).unwrap();
        assert_eq!(opts.mode, Mode::Build);
        assert!(diagnostics.iter().any(|d| d.code == "unresolved-reference"));
        assert!(diagnostics.iter().any(|d| d.code == "missing-asset"));
        assert!(diagnostics.iter().all(|d| d.level == Level::Warning));
        assert!(!directory.path().join("dist").exists());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn compat_promotes_frontend_colon_findings_once_and_checks_included_files()
    {
        let directory = project(&[
            (
                "main.md",
                ":::include part.md\n:::\n\n---\n\n:::include part.md\n:::\n",
            ),
            (
                "part.md",
                "---\nauthors: [Ada]\n---\n\n:::callout title=\"Figure: one\"\nBody.\n:::\n",
            ),
        ]);
        let opts = BuildOptions::new(directory.path().join("main.md"));
        let ordinary = run(&opts, &CheckOptions::default()).unwrap();
        let diagnostics =
            run(&opts, &CheckOptions { compat_presenter: true }).unwrap();
        let original: Vec<_> = ordinary
            .iter()
            .filter(|d| d.code == DIRECTIVE_COLON_REWRITTEN)
            .collect();
        assert_eq!(original.len(), 1);
        assert_eq!(original[0].level, Level::Info);
        let promoted: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == DIRECTIVE_COLON_REWRITTEN)
            .collect();
        assert_eq!(promoted.len(), 1);
        assert_eq!(promoted[0].level, Level::Warning);
        let authors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "presenter-front-matter")
            .collect();
        assert_eq!(authors.len(), 1);
        assert_eq!(
            authors[0].path,
            Some(
                std::fs::canonicalize(directory.path().join("part.md"))
                    .unwrap()
            )
        );
    }

    #[test]
    fn notebook_compatibility_scans_source_cells_and_only_the_first_header() {
        let source = json!({"nbformat": 4, "metadata": {"authors": ["Metadata"]}, "cells": [
            {"cell_type": "code", "source": ":::include ignored.md\n:::\n", "execution_count": 1},
            {"cell_type": "markdown", "source": "---\nauthors: [Ada]\n---\n"},
            {"cell_type": "markdown", "source": "---\nauthors: [Ignored]\n---\n"},
            {"cell_type": "markdown", "metadata": {"tags": ["remove-cell"]}, "source": "Before.\n\n:::include removed.md\n:::\n"},
            {"cell_type": "raw", "metadata": {"raw_mimetype": "text/markdown; charset=utf-8"}, "source": "Before.\n\n:::include raw.md\n:::\n"},
            {"cell_type": "raw", "metadata": {"raw_mimetype": "text/html"}, "source": "Before.\n\n:::include ignored.md\n:::\n"},
        ]}).to_string();
        let directory = project(&[("cells.ipynb", &source)]);
        let path = directory.path().join("cells.ipynb");
        let mut diagnostics = Vec::new();
        source_compatibility(
            &path,
            SourceKind::Notebook,
            &Config::default(),
            &mut diagnostics,
        )
        .unwrap();
        let locations: Vec<_> = diagnostics
            .iter()
            .map(|d| {
                let span = d.span.as_ref().unwrap();
                (d.code, span.cell, span.start_line)
            })
            .collect();
        assert_eq!(
            locations,
            [
                ("presenter-front-matter", Some(1), 2),
                ("presenter-include-position", Some(3), 3),
                ("presenter-include-position", Some(4), 3),
            ]
        );
    }

    #[test]
    fn later_known_headers_hide_scalar_contents_but_unknown_and_raw_stay_literal()
     {
        let header = "---\nabstract: |\n  Context.\n\n  :::include hidden.md\n  :::\n---\n\nAfter.";
        let source = json!({"nbformat": 4, "cells": [
            {"cell_type": "markdown", "source": "First."},
            {"cell_type": "markdown", "source": header},
        ]})
        .to_string();
        let directory = project(&[("cells.ipynb", &source)]);
        let path = directory.path().join("cells.ipynb");
        let diagnostics = run(
            &BuildOptions::new(&path),
            &CheckOptions { compat_presenter: true },
        )
        .unwrap();
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "notebook-front-matter-ignored")
        );
        assert!(!diagnostics.iter().any(|d| d.code.starts_with("presenter-")));

        let unknown = header.replace("abstract:", "custom:");
        let source = json!({"nbformat": 4, "cells": [
            {"cell_type": "markdown", "source": "First."},
            {"cell_type": "markdown", "source": header},
            {"cell_type": "markdown", "source": unknown},
            {"cell_type": "raw", "metadata": {"raw_mimetype": "text/markdown"}, "source": header},
        ]}).to_string();
        std::fs::write(&path, source).unwrap();
        let mut diagnostics = Vec::new();
        source_compatibility(
            &path,
            SourceKind::Notebook,
            &Config::default(),
            &mut diagnostics,
        )
        .unwrap();
        let locations: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| {
                let span = diagnostic.span.as_ref().unwrap();
                (diagnostic.code, span.cell, span.start_line)
            })
            .collect();
        assert_eq!(
            locations,
            [
                ("presenter-include-position", Some(2), 5),
                ("presenter-include-position", Some(3), 5),
            ]
        );
        assert!(crate::notebook::convert::later_metadata_header(
            "title: Later"
        ));
        assert!(crate::notebook::convert::later_metadata_header(
            "markdown: {references: false}"
        ));
        assert!(!crate::notebook::convert::later_metadata_header(
            "custom: Unknown"
        ));
        assert!(!crate::notebook::convert::later_metadata_header(
            "authors: invalid-shape"
        ));
    }

    #[test]
    fn fatal_error_conversion_keeps_notebook_and_included_locations() {
        let input = Path::new("root.ipynb");
        let error = Error::Parse {
            path: PathBuf::from("included.ipynb"),
            line: None,
            message: "cell 4, line 3: malformed include".into(),
        };
        let diagnostic = error_diagnostic(&error, input);
        assert_eq!(diagnostic.code, "parse-error");
        assert_eq!(diagnostic.message, "malformed include");
        assert_eq!(diagnostic.span.as_ref().unwrap().cell, Some(4));
        assert_eq!(diagnostic.span.as_ref().unwrap().start_line, 3);
        assert!(diagnostic.path.unwrap().ends_with("included.ipynb"));

        let error = Error::FrontMatter(
            "cell 2: invalid YAML at line 5 column 7".into(),
        );
        let diagnostic = error_diagnostic(&error, input);
        assert_eq!(diagnostic.code, "parse-error");
        assert_eq!(diagnostic.span.as_ref().unwrap().cell, Some(2));
        assert_eq!(diagnostic.span.as_ref().unwrap().start_line, 5);

        let error = Error::MissingAsset {
            path: PathBuf::from("missing.png"),
            span: Some(SourceSpan {
                start_line: 8,
                end_line: 8,
                cell: None,
                file: Some(PathBuf::from("included.md")),
            }),
        };
        let diagnostic = error_diagnostic(&error, input);
        assert_eq!(diagnostic.code, "missing-asset");
        assert!(diagnostic.path.unwrap().ends_with("included.md"));
    }

    #[test]
    fn failures_keep_library_errors_available_to_the_command() {
        let directory = project(&[("main.md", ":::include missing.md\n:::\n")]);
        let opts = BuildOptions::new(directory.path().join("main.md"));
        let failure = run(&opts, &CheckOptions::default()).unwrap_err();
        assert!(matches!(*failure.error, Error::Parse { line: Some(1), .. }));
        let diagnostic = error_diagnostic(&failure.error, &opts.input);
        assert_eq!(diagnostic.code, "parse-error");
        assert!(diagnostic.message.contains("file not found"));
    }

    #[test]
    fn source_order_is_stable_for_unlocated_root_and_included_findings() {
        let located = |code, path: &str, cell, line| {
            Diagnostic::warning(code, code).with_path(path).with_span(
                SourceSpan {
                    start_line: line,
                    end_line: line,
                    cell,
                    file: None,
                },
            )
        };
        let mut diagnostics = vec![
            located("child-later", "child.md", None, 8),
            located("root-cell", "main.ipynb", Some(1), 1),
            located("tie-first", "main.ipynb", Some(0), 3),
            located("tie-second", "main.ipynb", Some(0), 3),
            Diagnostic::info("unlocated", "unlocated").with_path("main.ipynb"),
            located("child-earlier", "child.md", None, 2),
        ];
        sort_by_position(
            &mut diagnostics,
            Path::new("main.ipynb"),
            &[PathBuf::from("child.md")],
        );
        assert_eq!(
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            [
                "unlocated", "tie-first", "tie-second", "root-cell",
                "child-earlier", "child-later"
            ]
        );
    }

    #[test]
    fn empty_math_nodes_keep_distinct_identities_across_target_union_visits() {
        let inlines = vec![
            Inline::Math { source: String::new(), dialect: MathDialect::Latex },
            Inline::Math { source: String::new(), dialect: MathDialect::Latex },
            Inline::Math {
                source: "#native".into(),
                dialect: MathDialect::Typst,
            },
        ];
        let location = MathLocation { path: "source.md".into(), span: None };
        let mut collector = MathCollector::default();
        collector.inlines(
            &inlines,
            &location,
            MathVisibility { targets: &[Target::Pdf], baseline: true },
        );
        collector.inlines(
            &inlines,
            &location,
            MathVisibility {
                targets: &[Target::Html, Target::Pdf],
                baseline: true,
            },
        );
        assert_eq!(collector.equations.len(), 2);
        for equation in collector.equations {
            assert!(equation.source.is_empty());
            assert_eq!(equation.targets, [Target::Pdf, Target::Html]);
        }
    }

    #[test]
    fn saved_output_union_tracks_borrowed_candidates_and_hidden_cells() {
        use crate::ir::{Attrs, CellOutput, OutputCandidate};
        let equation = || Block::Math {
            source: String::new(),
            dialect: MathDialect::Latex,
            attrs: Attrs::default(),
            number: None,
            span: None,
        };
        let mut cell = Cell {
            show_output: true,
            outputs: vec![CellOutput {
                candidates: vec![
                    OutputCandidate {
                        mime: "text/html".into(),
                        blocks: vec![
                            Block::Raw {
                                format: "html".into(),
                                text: "<p>html</p>".into(),
                                span: None,
                            },
                            equation(),
                        ],
                    },
                    OutputCandidate {
                        mime: "text/latex".into(),
                        blocks: vec![equation()],
                    },
                ],
                ..CellOutput::default()
            }],
            ..Cell::default()
        };
        let location = MathLocation { path: "cells.ipynb".into(), span: None };
        let targets = [Target::Pdf, Target::Html, Target::Docx, Target::Pdf];
        let mut collector = MathCollector::default();
        collector.cell(
            &cell,
            &location,
            MathVisibility { targets: &targets, baseline: true },
        );
        assert_eq!(collector.equations.len(), 2);
        assert_eq!(collector.equations[0].targets, [Target::Html]);
        assert!(collector.equations[1].targets.contains(&Target::Pdf));
        assert!(collector.equations[1].targets.contains(&Target::Docx));
        drop(collector);
        cell.show_output = false;
        let mut hidden = MathCollector::default();
        hidden.cell(
            &cell,
            &location,
            MathVisibility { targets: &targets, baseline: true },
        );
        assert!(hidden.equations.is_empty());
        let mut unselected = MathCollector::default();
        cell.show_output = true;
        unselected.cell(
            &cell,
            &location,
            MathVisibility { targets: &[], baseline: true },
        );
        assert!(unselected.equations.is_empty());
    }

    #[test]
    fn nested_math_locations_preserve_include_and_cell_when_child_has_lines() {
        let parent = MathLocation {
            path: "included.ipynb".into(),
            span: Some(SourceSpan {
                start_line: 1,
                end_line: 12,
                cell: Some(4),
                file: Some("included.ipynb".into()),
            }),
        };
        let child = parent.nested(Some(&SourceSpan {
            start_line: 7,
            end_line: 8,
            cell: None,
            file: None,
        }));
        let finding = child.attach(Diagnostic::warning("math-syntax", "bad"));
        assert_eq!(finding.path.as_deref(), Some(Path::new("included.ipynb")));
        let span = finding.span.unwrap();
        assert_eq!(
            (span.start_line, span.end_line, span.cell),
            (7, 8, Some(4))
        );
        assert_eq!(span.file.as_deref(), Some(Path::new("included.ipynb")));
        let included_markdown = parent.nested(Some(&SourceSpan {
            start_line: 3,
            end_line: 3,
            cell: None,
            file: Some("other.md".into()),
        }));
        assert_eq!(
            included_markdown.span.unwrap().cell,
            None,
            "an included Markdown file must not inherit a notebook cell"
        );
    }

    #[test]
    fn pdf_inline_visibility_matches_nonfragment_reference_and_citation_emission()
     {
        use crate::ir::{
            Attrs, CiteItem, CiteMode, LabelKind, ResolvedCite, ResolvedRef,
        };
        let math = |name: &str| Inline::Math {
            source: format!(r"\foo{{{name}}}"),
            dialect: MathDialect::Latex,
        };
        let label = |number: Option<&str>| LabelInfo {
            kind: LabelKind::Heading,
            number: number.map(str::to_owned),
            supplement: "Section".into(),
            text: None,
            span: None,
        };
        let labels = BTreeMap::from([
            ("plain".into(), label(None)),
            ("page".into(), label(None)),
            ("numbered".into(), label(Some("1"))),
        ]);
        let reference = |target: &str, form| Inline::Ref {
            target: target.into(),
            supplement: Some(vec![math(target)]),
            form,
            resolved: Some(ResolvedRef {
                kind: LabelKind::Heading,
                number: String::new(),
                text: vec![math(if target == "plain" {
                    "generated"
                } else {
                    "unused-generated"
                })],
                anchor: target.into(),
            }),
            span: None,
        };
        let citation = |resolved: bool| Inline::Cite {
            items: vec![CiteItem {
                key: "key".into(),
                prefix: Some(vec![math(if resolved {
                    "cite-prefix"
                } else {
                    "missing-prefix"
                })]),
                suffix: Some(vec![math(if resolved {
                    "cite-suffix"
                } else {
                    "missing-suffix"
                })]),
                ..CiteItem::default()
            }],
            mode: CiteMode::Normal,
            resolved: resolved.then(|| ResolvedCite {
                inlines: vec![math("unused-generated")],
                note: false,
            }),
            span: None,
        };
        let inlines = vec![
            Inline::Image {
                alt: vec![math("alt")],
                url: "asset:image".into(),
                title: None,
                attrs: Attrs::default(),
            },
            reference("missing", RefForm::Normal),
            reference("plain", RefForm::Normal),
            reference("page", RefForm::Page),
            reference("numbered", RefForm::Normal),
            citation(false),
            citation(true),
        ];
        let location = MathLocation { path: "source.md".into(), span: None };
        let mut collector =
            MathCollector { labels: Some(&labels), ..MathCollector::default() };
        collector.inlines(
            &inlines,
            &location,
            MathVisibility {
                targets: &[Target::Html, Target::Pdf],
                baseline: true,
            },
        );
        assert_eq!(collector.equations.len(), 10);
        for name in
            ["alt", "missing", "plain", "missing-prefix", "missing-suffix"]
        {
            let source = format!(r"\foo{{{name}}}");
            let equation = collector
                .equations
                .iter()
                .find(|e| e.source == source)
                .unwrap();
            assert!(equation.baseline, "{name}");
            assert!(!equation.targets.contains(&Target::Pdf), "{name}");
        }
        for name in ["page", "numbered", "cite-prefix", "cite-suffix"] {
            let source = format!(r"\foo{{{name}}}");
            let equation = collector
                .equations
                .iter()
                .find(|e| e.source == source)
                .unwrap();
            assert!(equation.baseline, "{name}");
            assert!(equation.targets.contains(&Target::Pdf), "{name}");
        }
        let generated = collector
            .equations
            .iter()
            .find(|e| e.source == r"\foo{generated}")
            .unwrap();
        assert!(!generated.baseline);
        assert_eq!(generated.targets, [Target::Pdf]);
        let mut diagnostics = Vec::new();
        validate_math(
            generated,
            &[Target::Pdf],
            &Config::default(),
            &BTreeMap::new(),
            &mut diagnostics,
        );
        assert_eq!(
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            ["math-conversion-failed"]
        );
    }
}
