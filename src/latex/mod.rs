//! LaTeX rendering, resource plans and materialization.

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use crate::assets::registry::AssetRegistry;
use crate::config::{Config, LatexCite, LatexEngine, LatexUnicode};
use crate::error::{
    Diagnostic, Diagnostics, Error, Level, Result, template_error,
};
use crate::ir::{DocMeta, Document};
use crate::math::latex_source::macros_from_config;

pub mod assets;
pub mod emit;
pub mod escape;
pub mod preamble;
pub mod template;

pub use assets::{materialise, needs_world};
pub use preamble::preamble;

/// The original bytes or image rendition behind a planned sidecar file.
#[derive(Debug, Clone)]
pub enum FileSource {
    /// A registered image; render requests its compiler PDF rendition.
    Asset {
        /// Registry identity.
        id: String,
        /// Whether the source needs a PDF rendition.
        render: bool,
    },
    /// A bibliography copied byte for byte.
    Bib(PathBuf),
}

/// One portable sidecar basename and its source.
#[derive(Debug, Clone)]
pub struct PlannedFile {
    /// File basename within the export's sidecar directory.
    pub name: String,
    /// Original bytes or a requested image rendition.
    pub source: FileSource,
}

/// Rendered document text and its complete resource plan.
pub struct LatexOutput {
    /// UTF-8 TeX, empty only when required template slots are absent.
    pub tex: String,
    /// Resources in emitted image then bibliography order.
    pub files: Vec<PlannedFile>,
    /// Located emission and template findings.
    pub diagnostics: Diagnostics,
}

/// Renders a dedicated LaTeX template and plans its resources without copying.
///
/// Only the selected template is read. Missing required slots are diagnostics;
/// unreadable, invalid or failing templates return their located template error.
pub fn render(
    document: &Document, registry: &AssetRegistry, config: &Config, stem: &str,
) -> Result<LatexOutput> {
    render_detailed(document, registry, config, stem)
        .map_err(|failure| *failure.error)
}

/// Internal render errors preserve findings collected before template evaluation.
#[derive(Debug)]
pub(crate) struct RenderFailure {
    pub(crate) error: Box<Error>,
    pub(crate) diagnostics: Diagnostics,
}

impl From<Error> for RenderFailure {
    fn from(error: Error) -> Self {
        Self { error: Box::new(error), diagnostics: Diagnostics::new() }
    }
}

pub(crate) fn render_detailed(
    document: &Document, registry: &AssetRegistry, config: &Config, stem: &str,
) -> std::result::Result<LatexOutput, RenderFailure> {
    let (path, source) = if config.latex.template.as_os_str().is_empty() {
        (PathBuf::from("writer-template.tex"), template::SAMPLE.to_owned())
    } else {
        let path = config.latex.template.clone();
        let source =
            fs::read_to_string(&path).map_err(|error| Error::Resource {
                what: "LaTeX template",
                path: path.clone(),
                message: error.to_string(),
            })?;
        (path, source)
    };
    let map = config.latex.unicode == LatexUnicode::Map
        || config.latex.unicode == LatexUnicode::Auto
            && config.latex.engine == LatexEngine::Pdflatex;
    let (environment, observed) = observed_environment(map);
    let name = path.to_string_lossy();
    let template = environment
        .template_from_named_str(&name, &source)
        .map_err(|error| template_error(error, false))?;
    let variables = template.undeclared_variables(false);
    let mut diagnostics = Diagnostics::new();
    for slot in ["preamble", "body"] {
        if !variables.contains(slot) {
            diagnostics.push(
                Diagnostic::error(
                    "latex-template-slot-missing",
                    format!(
                        "LaTeX template {} has no \\VAR{{{slot}}}",
                        path.display()
                    ),
                )
                .with_path(&path),
            );
        }
    }
    if diagnostics.has_errors() {
        return Ok(LatexOutput {
            tex: String::new(),
            files: Vec::new(),
            diagnostics,
        });
    }

    let (mut emitted, mut unicode) =
        emit::emit_with_unicode(document, registry, config, stem);
    if map && !emitted.usage.math && metadata_has_mapping(&document.meta) {
        emitted.usage.math = true;
        // Body emission did not use math, so its finish pass did not validate
        // these macros. No source line is invented for metadata or vars.
        for (name, _) in macros_from_config(&config.math.katex_options) {
            if !name.strip_prefix('\\').is_some_and(|name| {
                !name.is_empty()
                    && name.bytes().all(|byte| byte.is_ascii_alphabetic())
            }) {
                diagnostics.push(Diagnostic::warning(
                    "math-macro-invalid",
                    format!("[math] katex_options.macros[\"{name}\"]: expected a backslash followed by ASCII letters"),
                ).with_path(&document.source.path));
            }
        }
    }
    // Retain structured code points rather than interpreting the emitter's
    // already truncated display message. The replacement summary comes after
    // actual template filter evaluation and still occurs once per document.
    diagnostics.extend(
        std::mem::take(&mut emitted.diagnostics)
            .into_iter()
            .filter(|finding| finding.code != "latex-unicode"),
    );
    for path in &config.references.bibliography {
        if path.extension().is_some_and(|extension| {
            extension.eq_ignore_ascii_case("yaml")
                || extension.eq_ignore_ascii_case("yml")
        }) {
            diagnostics.push(Diagnostic::error(
                "latex-bib-yaml",
                "hayagriva YAML cannot feed BibTeX or biber; convert it to .bib",
            ).with_path(path));
        }
    }
    if emitted.toc > 0 && !variables.contains("toc") {
        emitted.body.insert_str(0, "\\tableofcontents\n\n");
    }
    let context = render_context(document, config, &emitted, stem, map);
    let rendered = template.render(&context);
    for character in
        observed.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).iter()
    {
        if !unicode.characters.contains(character) {
            unicode.characters.push(*character);
        }
    }
    if let Some(finding) = unicode_summary(&unicode, document, config) {
        diagnostics.push(finding);
    }
    let tex = rendered.map_err(|error| RenderFailure {
        error: Box::new(template_error(error, false)),
        diagnostics: diagnostics.clone(),
    })?;
    let mut files = Vec::new();
    for image in emitted.images {
        files.push(PlannedFile {
            name: image.name,
            source: FileSource::Asset { id: image.id, render: image.render },
        });
    }
    for (path, name) in emitted.bibliography {
        files.push(PlannedFile { name, source: FileSource::Bib(path) });
    }
    Ok(LatexOutput { tex, files, diagnostics })
}

// Evaluation is observable here; eventual printing is not. Captured/discarded
// expressions that execute tex count, while preescaped variables, raw vars and
// template/raw-TeX literals remain outside this observer's scope.
fn observed_environment(
    map: bool,
) -> (minijinja::Environment<'static>, Arc<Mutex<Vec<char>>>) {
    let mut environment = template::environment(map);
    let observed = Arc::new(Mutex::new(Vec::new()));
    let collector = observed.clone();
    environment.add_filter("tex", move |source: String| {
        let text = escape::text(&source, map).into_owned();
        let mut characters =
            collector.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        for character in text.chars().filter(|ch| !escape::pdflatex_safe(*ch)) {
            if !characters.contains(&character) {
                characters.push(character);
            }
        }
        text
    });
    (environment, observed)
}

fn unicode_summary(
    inventory: &emit::UnicodeInventory, document: &Document, config: &Config,
) -> Option<Diagnostic> {
    if inventory.characters.is_empty() {
        return None;
    }
    let mut message = inventory
        .characters
        .iter()
        .take(10)
        .map(|ch| format!("U+{:04X} ({ch})", *ch as u32))
        .collect::<Vec<_>>()
        .join(", ");
    if inventory.characters.len() > 10 {
        message.push_str(&format!(
            " and {} more",
            inventory.characters.len() - 10
        ));
    }
    let path = inventory
        .span
        .as_ref()
        .and_then(|span| span.file.as_ref())
        .unwrap_or(&document.source.path);
    let mut finding = Diagnostic::new(
        if config.latex.engine == LatexEngine::Pdflatex {
            Level::Warning
        } else {
            Level::Info
        },
        "latex-unicode",
        format!("characters not supported by pdfLaTeX: {message}"),
    )
    .with_path(path);
    if let Some(span) = &inventory.span {
        finding = finding.with_span(span.clone());
    }
    Some(finding)
}

fn render_context(
    document: &Document, config: &Config, emitted: &emit::Emitted, stem: &str,
    map: bool,
) -> Value {
    let meta = &document.meta;
    let text = |value: Option<&str>| {
        escape::text(value.unwrap_or_default(), map).into_owned()
    };
    let authors = meta
        .authors
        .iter()
        .map(|author| {
            json!({
                "name": text(Some(&author.name)),
                "affiliation": text(author.affiliation.as_deref()),
                "email": text(author.email.as_deref()),
                "id": text(author.id.as_deref()),
            })
        })
        .collect::<Vec<_>>();
    let lang = meta
        .lang
        .as_deref()
        .filter(|lang| !lang.is_empty())
        .or_else(|| {
            (!config.document.lang.is_empty())
                .then_some(config.document.lang.as_str())
        })
        .unwrap_or("en")
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
        .collect::<String>();
    let bibliography = emitted
        .bibliography
        .iter()
        .map(|(_, name)| format!("{stem}_assets/{name}"))
        .collect::<Vec<_>>();
    json!({
        "preamble": preamble(&emitted.usage, config), "body": emitted.body,
        "title": text(meta.title.as_deref()),
        "subtitle": text(meta.subtitle.as_deref()), "date": text(meta.date.as_deref()),
        "authors": authors, "abstract": emitted.abstract_,
        "keywords": meta.keywords.iter().map(|keyword| text(Some(keyword))).collect::<Vec<_>>(),
        "lang": lang, "toc": emitted.toc, "bibliography": bibliography,
        "has_citations": document.features.citations,
        "cite": match config.latex.cite {
            LatexCite::Biblatex => "biblatex", LatexCite::Natbib => "natbib",
            LatexCite::Plain => "plain",
        },
        "emit_bibliography": config.latex.emit_bibliography,
        "has_math": emitted.usage.math, "has_code": emitted.usage.code,
        "has_highlighted": emitted.usage.highlighted, "has_cells": emitted.usage.cells,
        "vars": meta.vars,
    })
}

fn metadata_has_mapping(meta: &DocMeta) -> bool {
    meta.title
        .iter()
        .chain(&meta.subtitle)
        .chain(&meta.date)
        .any(|text| text_has_mapping(text))
        || meta.authors.iter().any(|author| {
            text_has_mapping(&author.name)
                || author
                    .affiliation
                    .iter()
                    .chain(&author.email)
                    .chain(&author.id)
                    .any(|text| text_has_mapping(text))
        })
        || meta.keywords.iter().any(|text| text_has_mapping(text))
        || meta.vars.iter().any(|(key, value)| {
            text_has_mapping(key) || value_has_mapping(value)
        })
}

fn text_has_mapping(text: &str) -> bool {
    text.chars().any(|ch| escape::map_char(ch).is_some())
}

fn value_has_mapping(value: &Value) -> bool {
    match value {
        Value::String(text) => text_has_mapping(text),
        Value::Array(values) => values.iter().any(value_has_mapping),
        Value::Object(values) => values.iter().any(|(key, value)| {
            text_has_mapping(key) || value_has_mapping(value)
        }),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LatexUnicode;
    use crate::ir::{
        Attrs, Author, Block, Features, Inline, MathDialect, Resolved,
        SourceInfo, SourceKind, SourceSpan,
    };

    fn document() -> Document {
        Document {
            meta: DocMeta::default(),
            blocks: vec![Block::Paragraph {
                inlines: vec![Inline::text("Body")],
                span: None,
            }],
            features: Features::default(),
            source: SourceInfo {
                path: "/book/main.md".into(),
                base_dir: "/book".into(),
                kind: SourceKind::Markdown,
            },
            assets: Vec::new(),
            includes: Vec::new(),
            resolved: Resolved::default(),
        }
    }

    fn custom(source: &str) -> (tempfile::TempDir, Config) {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.latex.template = directory.path().join("template.tex");
        fs::write(&config.latex.template, source).unwrap();
        (directory, config)
    }

    #[test]
    fn slots_are_checked_before_body_findings_or_file_planning() {
        for (source, slots) in [
            ("no slots", vec!["preamble", "body"]),
            (r"\VAR{body}", vec!["preamble"]),
            (r"\VAR{preamble}", vec!["body"]),
        ] {
            let (_directory, mut config) = custom(source);
            config.references.bibliography.push("/not/read/refs.yaml".into());
            let mut document = document();
            document.blocks = vec![Block::CodeBlock {
                lang: None,
                text: r"\end{Verbatim}".into(),
                attrs: Attrs::default(),
                span: None,
            }];
            let output =
                render(&document, &AssetRegistry::new(), &config, "paper")
                    .unwrap();
            assert!(output.tex.is_empty());
            assert!(output.files.is_empty());
            assert!(output.diagnostics.has_errors());
            let diagnostics = output.diagnostics.into_vec();
            assert_eq!(diagnostics.len(), slots.len());
            for (finding, slot) in diagnostics.iter().zip(slots) {
                assert_eq!(finding.code, "latex-template-slot-missing");
                assert_eq!(finding.path.as_ref(), Some(&config.latex.template));
                assert!(finding.span.is_none());
                assert_eq!(
                    finding.message,
                    format!(
                        "LaTeX template {} has no \\VAR{{{slot}}}",
                        config.latex.template.display(),
                    )
                );
            }
        }
    }

    #[test]
    fn named_template_parse_and_undefined_variable_errors_keep_actual_locations()
     {
        for source in [
            "\\VAR{preamble}\n\\VAR{body}\n\\VAR{",
            "\\VAR{preamble}\n\\VAR{body}\n\\VAR{vars.absent}",
            "\\VAR{preamble}\n\\VAR{body}\n\\VAR{vars.absent | tex}",
        ] {
            let (_directory, config) = custom(source);
            let error =
                render(&document(), &AssetRegistry::new(), &config, "paper")
                    .err()
                    .unwrap();
            let Error::Template { template, line, .. } = error else {
                panic!("{error}")
            };
            assert_eq!(template, config.latex.template.to_string_lossy());
            assert_eq!(line, Some(3));
        }
        let (_directory, config) = custom(concat!(
            "\\VAR{preamble}\n\\VAR{body}\n",
            "\\BLOCK{if vars.absent}hidden\\BLOCK{endif}guarded\n",
        ));
        let output =
            render(&document(), &AssetRegistry::new(), &config, "paper")
                .unwrap();
        assert!(output.tex.ends_with("guarded\n"));
    }

    #[test]
    fn unreadable_custom_templates_are_located_resources_without_sample_fallback()
     {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.latex.template = directory.path().join("missing.tex");
        let error =
            render(&document(), &AssetRegistry::new(), &config, "paper")
                .err()
                .unwrap();
        let Error::Resource { what, path, message } = error else {
            panic!("{error}")
        };
        assert_eq!(what, "LaTeX template");
        assert_eq!(path, config.latex.template);
        assert!(!message.is_empty());
    }

    #[test]
    fn all_template_variables_keep_their_escaped_or_raw_contract() {
        let (_directory, mut config) = custom(concat!(
            "\\VAR{preamble}\\VAR{body}\n",
            "T=\\VAR{title}|S=\\VAR{subtitle}|D=\\VAR{date}\n",
            "A=\\VAR{authors[0].name}|\\VAR{authors[0].affiliation}|",
            "\\VAR{authors[0].email}|\\VAR{authors[0].id}\n",
            "K=\\VAR{keywords | join(', ')}|L=\\VAR{lang}\n",
            "ABS=\\VAR{abstract}\nRAW=\\VAR{vars.raw}|FILTER=\\VAR{vars.filtered | tex}\n",
            "FLAGS=\\VAR{toc},\\VAR{has_citations},\\VAR{cite},",
            "\\VAR{emit_bibliography},\\VAR{has_math},\\VAR{has_code},",
            "\\VAR{has_highlighted},\\VAR{has_cells}\n",
            "TYPES=\\VAR{toc is integer},\\VAR{has_citations is boolean},",
            "\\VAR{emit_bibliography is boolean},\\VAR{has_math is boolean},",
            "\\VAR{has_code is boolean},\\VAR{has_highlighted is boolean},",
            "\\VAR{has_cells is boolean}\n",
            "BIB=\\VAR{bibliography | join(',')}\n",
        ));
        config.latex.cite = LatexCite::Plain;
        config.references.bibliography.push("/not/read/refs.bib".into());
        let mut document = document();
        document.blocks = vec![Block::Raw {
            format: "latex".into(),
            text: r"\VAR{unparsed} & raw".into(),
            span: None,
        }];
        document.meta = DocMeta {
            title: Some("A & α".into()),
            subtitle: Some("# S".into()),
            date: Some("2026_10".into()),
            authors: vec![Author {
                name: "A & B".into(),
                affiliation: None,
                email: Some("a_b@c".into()),
                id: Some("10%".into()),
            }],
            keywords: vec!["tag&".into(), "α".into()],
            lang: Some("es_MX!".into()),
            abstract_: Some(vec![Block::Paragraph {
                inlines: vec![Inline::text("Abstract &")],
                span: None,
            }]),
            vars: serde_json::from_value(json!({
                "raw": r"\alpha & raw", "filtered": "β &",
            }))
            .unwrap(),
            ..DocMeta::default()
        };
        let output =
            render(&document, &AssetRegistry::new(), &config, "paper").unwrap();
        assert!(output.tex.contains(r"\usepackage{amsmath,amssymb}"));
        assert!(output.tex.contains(r"\VAR{unparsed} & raw"));
        assert!(
            output.tex.contains(
                "T=A \\& \\ensuremath{\\alpha}|S=\\# S|D=2026\\_10\n"
            )
        );
        assert!(output.tex.contains("A=A \\& B||a\\_b@c|10\\%\n"));
        assert!(
            output.tex.contains("K=tag\\&, \\ensuremath{\\alpha}|L=esMX\n")
        );
        assert!(output.tex.contains("ABS=Abstract \\&\n"));
        assert!(
            output.tex.contains(
                "RAW=\\alpha & raw|FILTER=\\ensuremath{\\beta} \\&\n"
            )
        );
        assert!(
            output
                .tex
                .contains("FLAGS=0,False,plain,False,True,False,False,False\n")
        );
        assert!(
            output.tex.contains("TYPES=True,True,True,True,True,True,True\n"),
            "{}",
            output.tex
        );
        assert!(output.tex.contains("BIB=paper_assets/refs.bib\n"));
        assert!(output.diagnostics.is_empty());
    }

    #[test]
    fn toc_insertion_uses_syntactic_template_variables_and_keeps_literal_body_bytes()
     {
        for (extra, inserted) in [
            ("", true),
            (r"\BLOCK{if false}\VAR{toc}\BLOCK{endif}", false),
            (r"DEPTH=\VAR{toc}", false),
        ] {
            let source =
                format!("\\VAR{{preamble}}\n{extra}\nBODY:\\VAR{{body}}");
            let (_directory, config) = custom(&source);
            let mut document = document();
            document.blocks.insert(0, Block::Toc { depth: 3, span: None });
            let output =
                render(&document, &AssetRegistry::new(), &config, "paper")
                    .unwrap();
            assert_eq!(output.tex.contains(r"\tableofcontents"), inserted);
            assert!(output.tex.contains(r"\setcounter{tocdepth}{3}"));
            if inserted {
                assert!(
                    output.tex.ends_with("BODY:\\tableofcontents\n\nBody\n")
                );
            } else {
                assert!(output.tex.ends_with("BODY:Body\n"));
            }
            if extra.starts_with("DEPTH") {
                assert!(output.tex.contains("DEPTH=3"));
            }
        }
    }

    #[test]
    fn metadata_and_nested_var_keys_conservatively_enable_math_only_when_mapping_is_effective()
     {
        for (engine, unicode, mapped) in [
            (LatexEngine::Pdflatex, LatexUnicode::Auto, true),
            (LatexEngine::Pdflatex, LatexUnicode::Keep, false),
            (LatexEngine::Xelatex, LatexUnicode::Auto, false),
            (LatexEngine::Xelatex, LatexUnicode::Map, true),
            (LatexEngine::Lualatex, LatexUnicode::Map, true),
        ] {
            let (_directory, mut config) = custom(
                "\\VAR{preamble}\nMATH=\\VAR{has_math}\nTYPE=\\VAR{has_math is boolean}\n\\VAR{body}",
            );
            config.latex.engine = engine;
            config.latex.unicode = unicode;
            config.math.katex_options.insert(
                "macros".into(),
                json!({
                    "bad": "x", "\\+": "y", "\\Good": "#1", "\\Ignored": 1,
                }),
            );
            let mut document = document();
            // The mapped character appears only in a nested map key. A
            // template could iterate/filter it, so it is a string anywhere.
            document.meta.vars.insert("nested".into(), json!([{"α": false}]));
            let output =
                render(&document, &AssetRegistry::new(), &config, "paper")
                    .unwrap();
            assert_eq!(
                output.tex.contains(r"\usepackage{amsmath,amssymb}"),
                mapped
            );
            assert!(output.tex.contains(if mapped {
                "MATH=True"
            } else {
                "MATH=False"
            }));
            assert_eq!(
                output.tex.contains(r"\providecommand{\Good}[1]{#1}"),
                mapped
            );
            assert!(output.tex.contains("TYPE=True"), "{}", output.tex);
            let findings = output.diagnostics.iter().collect::<Vec<_>>();
            assert_eq!(findings.len(), if mapped { 2 } else { 0 });
            for finding in findings {
                assert_eq!(finding.code, "math-macro-invalid");
                assert_eq!(finding.path.as_ref(), Some(&document.source.path));
                assert!(finding.span.is_none());
            }
        }
    }

    #[test]
    fn existing_body_macro_findings_are_not_duplicated_by_mapped_metadata() {
        let mut config = Config::default();
        config.math.katex_options.insert("macros".into(), json!({"bad": "x"}));
        let mut document = document();
        document.meta.title = Some("α".into());
        let span = SourceSpan {
            start_line: 2,
            end_line: 2,
            cell: Some(4),
            file: Some("/book/child.ipynb".into()),
        };
        document.blocks = vec![Block::Math {
            source: "x".into(),
            dialect: MathDialect::Latex,
            attrs: Attrs::default(),
            number: None,
            span: Some(span.clone()),
        }];
        let output =
            render(&document, &AssetRegistry::new(), &config, "paper").unwrap();
        let findings = output
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "math-macro-invalid")
            .collect::<Vec<_>>();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].span, Some(span));
        assert_eq!(
            findings[0].path.as_deref(),
            Some(std::path::Path::new("/book/child.ipynb"))
        );
    }

    #[test]
    fn plans_are_deterministic_immutable_and_never_read_or_render_asset_bytes()
    {
        let (_directory, mut config) = custom(r"\VAR{preamble}\VAR{body}");
        let root = config.latex.template.parent().unwrap().to_owned();
        let mut registry = AssetRegistry::new();
        let png = registry.register_file(&root.join("plot.png"));
        let svg = registry.register_bytes(
            b"not an SVG".to_vec(),
            "image/svg+xml",
            "diagram.svg",
        );
        config.references.bibliography = vec![
            root.join("a/refs.bib"),
            root.join("b/refs.bib"),
            root.join("refs.YaMl"),
            root.join("other.YML"),
        ];
        let mut document = document();
        document.blocks = vec![Block::Paragraph {
            inlines: [&png, &svg, &png]
                .into_iter()
                .map(|id| Inline::Image {
                    alt: Vec::new(),
                    url: AssetRegistry::placeholder(id),
                    title: None,
                    attrs: Attrs::default(),
                })
                .collect(),
            span: None,
        }];
        let before_document = document.clone();
        let before_registry = registry.clone();
        let before_config = serde_json::to_value(&config).unwrap();
        let first = render(&document, &registry, &config, "paper").unwrap();
        let second = render(&document, &registry, &config, "paper").unwrap();
        assert_eq!(first.tex, second.tex);
        assert_eq!(
            first
                .files
                .iter()
                .map(|file| file.name.as_str())
                .collect::<Vec<_>>(),
            ["plot.png", "diagram.svg.pdf", "refs.bib", "refs-2.bib",]
        );
        assert_eq!(format!("{:?}", first.files), format!("{:?}", second.files));
        assert!(
            matches!(&first.files[0].source, FileSource::Asset { id, render: false } if id == &png)
        );
        assert!(
            matches!(&first.files[1].source, FileSource::Asset { id, render: true } if id == &svg)
        );
        assert!(
            matches!(&first.files[2].source, FileSource::Bib(path) if path == &root.join("a/refs.bib"))
        );
        assert!(needs_world(&first.files));
        let findings = first.diagnostics.into_vec();
        assert_eq!(findings.len(), 2);
        assert!(
            findings.iter().all(|finding| finding.code == "latex-bib-yaml")
        );
        assert_eq!(findings[0].path.as_ref(), Some(&root.join("refs.YaMl")));
        assert_eq!(findings[1].path.as_ref(), Some(&root.join("other.YML")));
        assert_eq!(document, before_document);
        assert_eq!(registry, before_registry);
        assert_eq!(serde_json::to_value(&config).unwrap(), before_config);
        let entries = fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(entries, [std::ffi::OsString::from("template.tex")]);
    }

    #[test]
    fn built_in_sample_defines_absent_metadata_and_suppresses_a_clock_based_date()
     {
        let output = render(
            &document(),
            &AssetRegistry::new(),
            &Config::default(),
            "paper",
        )
        .unwrap();
        assert!(output.tex.contains(r"\date{}"));
        assert!(output.tex.contains("Body\n"));
        assert!(!output.tex.contains(r"\maketitle"));
        assert!(output.tex.ends_with("\\end{document}\n"));
        assert!(output.files.is_empty());
        assert!(output.diagnostics.is_empty());
    }

    #[test]
    fn detailed_template_failures_retain_findings_from_completed_emission() {
        let (_directory, mut config) =
            custom("\\VAR{preamble}\n\\VAR{body}\n\\VAR{vars.missing}\n");
        config.latex.code = crate::config::LatexCode::Listings;
        let mut document = document();
        document.blocks = vec![Block::CodeBlock {
            lang: Some("rust".into()),
            text: "fn main() {}".into(),
            attrs: Attrs::default(),
            span: None,
        }];
        let failure =
            render_detailed(&document, &AssetRegistry::new(), &config, "paper")
                .err()
                .unwrap();
        let Error::Template { template, line, .. } = *failure.error else {
            panic!()
        };
        assert_eq!(template, config.latex.template.to_string_lossy());
        assert_eq!(line, Some(3));
        let findings = failure.diagnostics.into_vec();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "latex-listings-language");
    }

    #[test]
    fn unicode_scope_excludes_unused_vars_raw_vars_metadata_and_template_literals()
     {
        let (_directory, config) = custom(concat!(
            "literal 😀\\VAR{preamble}\\VAR{body}",
            "\\VAR{title}\\VAR{vars.raw}",
            "\\BLOCK{if false}\\VAR{vars.unused | tex}\\BLOCK{endif}",
        ));
        let mut document = document();
        document.meta.title = Some("一".into());
        document.meta.vars.insert("raw".into(), "二".into());
        document.meta.vars.insert("unused".into(), "三".into());
        let output =
            render(&document, &AssetRegistry::new(), &config, "paper").unwrap();
        assert!(output.tex.contains("literal 😀"));
        assert!(output.tex.ends_with("一二"));
        assert!(!output.tex.contains('三'));
        assert!(
            !output
                .diagnostics
                .iter()
                .any(|finding| finding.code == "latex-unicode")
        );
    }

    #[test]
    fn tex_filter_scans_escaped_survivors_at_evaluation_time_even_when_discarded()
     {
        for (engine, unicode, alpha, level) in [
            (LatexEngine::Pdflatex, LatexUnicode::Auto, false, Level::Warning),
            (LatexEngine::Xelatex, LatexUnicode::Auto, true, Level::Info),
            (LatexEngine::Xelatex, LatexUnicode::Map, false, Level::Info),
        ] {
            let (_directory, mut config) = custom(concat!(
                "\\VAR{preamble}\\VAR{body}",
                "\\BLOCK{set unused = vars.value | tex}",
            ));
            config.latex.engine = engine;
            config.latex.unicode = unicode;
            let mut document = document();
            document
                .meta
                .vars
                .insert("value".into(), "\u{200b}\u{202f}α😀".into());
            let output =
                render(&document, &AssetRegistry::new(), &config, "paper")
                    .unwrap();
            assert!(!output.tex.contains('😀'));
            let findings = output
                .diagnostics
                .iter()
                .filter(|finding| finding.code == "latex-unicode")
                .collect::<Vec<_>>();
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].level, level);
            assert_eq!(findings[0].path.as_ref(), Some(&document.source.path));
            assert!(findings[0].span.is_none());
            assert!(findings[0].message.contains("U+1F600 (😀)"));
            assert_eq!(findings[0].message.contains("U+03B1 (α)"), alpha);
            assert!(!findings[0].message.contains("U+200B"));
            assert!(!findings[0].message.contains("U+202F"));
        }
    }

    #[test]
    fn unicode_inventory_merges_untruncated_ir_then_filter_order_once() {
        let (_directory, config) = custom(concat!(
            "\\VAR{vars.first | tex}\\VAR{preamble}\\VAR{body}",
            "\\VAR{vars.second | tex}\\VAR{vars.first | tex}",
        ));
        let first: String = (0x4e00..=0x4e0d)
            .map(|code| char::from_u32(code).unwrap())
            .collect();
        let span = SourceSpan {
            start_line: 7,
            end_line: 7,
            cell: Some(3),
            file: Some("/book/child.ipynb".into()),
        };
        let mut document = document();
        document.blocks = vec![Block::Paragraph {
            inlines: vec![Inline::text(&first)],
            span: Some(span.clone()),
        }];
        document.meta.vars.insert("first".into(), "一😀".into());
        document.meta.vars.insert("second".into(), "😀一".into());
        let output =
            render(&document, &AssetRegistry::new(), &config, "paper").unwrap();
        let findings = output
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "latex-unicode")
            .collect::<Vec<_>>();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].span, Some(span));
        assert_eq!(
            findings[0].path.as_deref(),
            Some(std::path::Path::new("/book/child.ipynb"))
        );
        assert!(findings[0].message.contains("U+4E00 (一), U+4E01 (丁)"));
        assert!(findings[0].message.contains("U+4E09 (三) and 5 more"));
        assert!(!findings[0].message.contains("U+4E0A"));
        assert!(!findings[0].message.contains("U+1F600"));
    }

    #[test]
    fn ir_only_unicode_summary_matches_the_unchanged_public_emitter() {
        let (_directory, config) = custom(r"\VAR{preamble}\VAR{body}");
        let mut document = document();
        document.blocks = vec![Block::Paragraph {
            inlines: vec![Inline::text("一😀一")],
            span: None,
        }];
        let body =
            emit::emit(&document, &AssetRegistry::new(), &config, "paper");
        let output =
            render(&document, &AssetRegistry::new(), &config, "paper").unwrap();
        let select = |diagnostics: Diagnostics| {
            diagnostics
                .into_iter()
                .filter(|finding| finding.code == "latex-unicode")
                .collect::<Vec<_>>()
        };
        assert_eq!(select(body.diagnostics), select(output.diagnostics));
    }

    #[test]
    fn failed_template_render_keeps_a_single_merged_unicode_summary() {
        let (_directory, config) = custom(concat!(
            "\\VAR{preamble}\\VAR{body}\\VAR{vars.extra | tex}",
            "\\VAR{vars.missing}",
        ));
        let mut document = document();
        document.blocks = vec![Block::Paragraph {
            inlines: vec![Inline::text("一")],
            span: None,
        }];
        document.meta.vars.insert("extra".into(), "😀".into());
        let failure =
            render_detailed(&document, &AssetRegistry::new(), &config, "paper")
                .err()
                .unwrap();
        assert!(matches!(*failure.error, Error::Template { .. }));
        let findings = failure.diagnostics.into_vec();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "latex-unicode");
        assert!(findings[0].message.contains("U+4E00 (一), U+1F600 (😀)"));
    }

    #[test]
    fn sample_loads_bibliography_packages_only_for_configured_bibliographies() {
        for family in [LatexCite::Biblatex, LatexCite::Natbib, LatexCite::Plain]
        {
            for configured in [false, true] {
                let mut config = Config::default();
                config.latex.cite = family;
                if configured {
                    // Pure rendering must plan the file without reading it.
                    config
                        .references
                        .bibliography
                        .push("/not/read/refs.bib".into());
                }
                let document = document();
                let output =
                    render(&document, &AssetRegistry::new(), &config, "paper")
                        .unwrap();
                assert!(output.diagnostics.is_empty());
                assert_eq!(output.files.len(), usize::from(configured));
                assert_eq!(
                    output.tex.contains("{biblatex}"),
                    configured && family == LatexCite::Biblatex,
                    "{}",
                    output.tex
                );
                assert_eq!(
                    output.tex.contains(r"\usepackage[round]{natbib}"),
                    configured && family == LatexCite::Natbib,
                    "{}",
                    output.tex
                );
                assert_eq!(
                    output
                        .tex
                        .contains(r"\addbibresource{paper_assets/refs.bib}"),
                    configured && family == LatexCite::Biblatex,
                    "{}",
                    output.tex
                );
                // Packages follow the resource list; bibliography printing
                // still follows the unchanged document citations flag.
                assert!(!output.tex.contains(r"\printbibliography"));
                assert!(!output.tex.contains(r"\bibliographystyle"));
                assert!(!output.tex.contains(r"\bibliography{"));
                assert_eq!(config.latex.cite, family);
            }
        }
    }
}
