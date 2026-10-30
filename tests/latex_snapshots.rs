//! Pure LaTeX body emission, corpus goldens and public semantic contracts.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use writer::assets::registry::AssetRegistry;
use writer::config::{
    Config, LatexCite, LatexCode, LatexEngine, LatexTables, LatexTopLevel,
    LatexUnicode,
};
use writer::error::{Diagnostics, Error, Level};
use writer::ir::*;
use writer::latex::emit::{Emitted, Usage, emit};
use writer::math::typst_to_latex;
use writer::resolve::{ResolveOptions, anchor, resolve};

#[path = "common/citations.rs"]
mod citation_library;

#[path = "common/latex_citations.rs"]
mod latex_citations;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn source(path: &Path) -> SourceInfo {
    SourceInfo {
        path: path.to_owned(),
        base_dir: path.parent().unwrap().into(),
        kind: detect_kind(path).unwrap(),
    }
}

fn document(blocks: Vec<Block>) -> Document {
    Document {
        meta: DocMeta::default(),
        blocks,
        features: Features::default(),
        source: SourceInfo {
            path: "/source/document.md".into(),
            base_dir: "/source".into(),
            kind: SourceKind::Markdown,
        },
        assets: Vec::new(),
        includes: Vec::new(),
        resolved: Resolved::default(),
    }
}

fn paragraph(inlines: Inlines) -> Block {
    Block::paragraph(inlines)
}

fn text(value: &str) -> Inline {
    Inline::text(value)
}

fn block(value: Value) -> Block {
    serde_json::from_value(value).unwrap()
}

fn inline(value: Value) -> Inline {
    serde_json::from_value(value).unwrap()
}

fn plan(output: &Emitted) -> Vec<(String, String, bool)> {
    output
        .images
        .iter()
        .map(|image| (image.id.clone(), image.name.clone(), image.render))
        .collect()
}

fn assert_layout(value: &str) {
    if !value.is_empty() {
        assert!(!value.starts_with('\n'), "{value:?}");
        assert!(value.ends_with('\n'), "{value:?}");
        assert!(!value.ends_with("\n\n"), "{value:?}");
    }
}

fn emit_checked(
    document: &Document, registry: &AssetRegistry, config: &Config,
) -> Emitted {
    let original = (document.clone(), registry.clone(), config.clone());
    let output = emit(document, registry, config, "paper.final");
    let again = emit(document, registry, config, "paper.final");
    assert_eq!(output.body, again.body);
    assert_eq!(output.abstract_, again.abstract_);
    assert_eq!(output.toc, again.toc);
    assert_eq!(output.usage, again.usage);
    assert_eq!(plan(&output), plan(&again));
    assert_eq!(output.bibliography, again.bibliography);
    assert_eq!(output.diagnostics, again.diagnostics);
    assert_eq!(*document, original.0);
    assert_eq!(*registry, original.1);
    assert_eq!(*config, original.2);
    assert_layout(&output.body);
    assert_layout(&output.abstract_);
    let unique =
        output.images.iter().map(|image| &image.id).collect::<BTreeSet<_>>();
    assert_eq!(unique.len(), output.images.len());
    output
}

fn redaction_roots(paths: &[PathBuf]) -> Vec<String> {
    let mut roots = paths
        .iter()
        .map(|path| {
            path.to_string_lossy()
                .replace('\\', "/")
                .trim_end_matches('/')
                .to_owned()
        })
        .collect::<Vec<_>>();
    roots.sort_by(|left, right| {
        right.len().cmp(&left.len()).then(left.cmp(right))
    });
    roots.dedup();
    roots
}

fn fixture_roots() -> Vec<String> {
    let written = fixtures();
    let mut roots = vec![written.clone()];
    if let Ok(canonical) = fs::canonicalize(written) {
        roots.push(canonical);
    }
    redaction_roots(&roots)
}

fn fixture_path(path: &str, roots: &[String]) -> Option<String> {
    let normalized = path.replace('\\', "/");
    roots.iter().find_map(|root| {
        if normalized == root.as_str() {
            Some("[fixtures]".into())
        } else {
            normalized
                .strip_prefix(root.as_str())
                .and_then(|tail| tail.strip_prefix('/'))
                .map(|relative| format!("[fixtures]/{relative}"))
        }
    })
}

fn stable_path_with_roots(path: &Path, roots: &[String]) -> String {
    let text = path.to_string_lossy();
    fixture_path(&text, roots).unwrap_or_else(|| text.replace('\\', "/"))
}

fn stable_path(path: &Path) -> String {
    stable_path_with_roots(path, &fixture_roots())
}

fn diagnostic_message(code: &str, message: &str, roots: &[String]) -> String {
    let marker = match code {
        "missing-asset" => " (looked for ",
        "duplicate-label" => " (first defined at ",
        _ => return message.to_owned(),
    };
    let Some((before, location)) =
        message.strip_suffix(')').and_then(|text| text.rsplit_once(marker))
    else {
        return message.to_owned();
    };
    let (path, line) = if code == "duplicate-label" {
        let Some((path, line)) = location.rsplit_once(':') else {
            return message.to_owned();
        };
        if line.is_empty() || !line.bytes().all(|byte| byte.is_ascii_digit()) {
            return message.to_owned();
        }
        (path, format!(":{line}"))
    } else {
        (location, String::new())
    };
    let Some(path) = fixture_path(path, roots) else {
        return message.to_owned();
    };
    format!("{before}{marker}{path}{line})")
}

fn findings(diagnostics: &Diagnostics) -> Vec<Value> {
    let roots = fixture_roots();
    diagnostics.iter().map(|finding| json!({
        "level":finding.level,"code":finding.code,
        "message":diagnostic_message(finding.code,&finding.message,&roots),
        "path":finding.path.as_deref().map(|path|stable_path_with_roots(path,&roots)),
        "span":finding.span.as_ref().map(|span| json!({
            "start_line":span.start_line,"end_line":span.end_line,"cell":span.cell,
            "file":span.file.as_deref().map(|path|stable_path_with_roots(path,&roots)),
        })),
    })).collect()
}

#[test]
fn only_known_diagnostic_path_fields_are_stabilized_across_platform_separators()
{
    for (root, separator) in
        [("/work/tests/fixtures", "/"), (r"C:\work\tests\fixtures", "\\")]
    {
        let roots = redaction_roots(&[root.into()]);
        let path = format!("{root}{separator}md{separator}references.md");
        let missing = format!("image `{path}` not found (looked for {path})");
        assert_eq!(
            diagnostic_message("missing-asset", &missing, &roots),
            format!(
                "image `{path}` not found (looked for [fixtures]/md/references.md)"
            )
        );
        let duplicate =
            format!("label `<x>` occurs twice (first defined at {path}:36)");
        assert_eq!(
            diagnostic_message("duplicate-label", &duplicate, &roots),
            "label `<x>` occurs twice (first defined at [fixtures]/md/references.md:36)"
        );
        // A path in unrelated prose, an unrelated code, or a malformed known
        // field is source text and must survive byte for byte.
        assert_eq!(
            diagnostic_message("unsupported-construct", &missing, &roots),
            missing
        );
        let literal = format!("literal path {path}");
        assert_eq!(
            diagnostic_message("missing-asset", &literal, &roots),
            literal
        );
        let malformed = duplicate.replace(":36)", ":line)");
        assert_eq!(
            diagnostic_message("duplicate-label", &malformed, &roots),
            malformed
        );
        let outside = format!(
            "image not found (looked for {root}-other{separator}file.png)"
        );
        assert_eq!(
            diagnostic_message("missing-asset", &outside, &roots),
            outside
        );
    }
}

#[test]
fn typed_and_message_paths_accept_written_canonical_and_windows_extended_roots()
{
    for (root_paths, paths) in [
        (
            vec!["/linked/tests/fixtures", "/actual/tests/fixtures"],
            vec![
                "/linked/tests/fixtures/md/references.md",
                "/actual/tests/fixtures/md/references.md",
            ],
        ),
        (
            vec![r"C:\work\tests\fixtures", r"\\?\C:\work\tests\fixtures"],
            vec![
                r"C:\work\tests\fixtures\md\references.md",
                r"\\?\C:\work\tests\fixtures\md\references.md",
                "C:/work/tests/fixtures/md/references.md",
                "//?/C:/work/tests/fixtures/md/references.md",
            ],
        ),
    ] {
        let roots = redaction_roots(
            &root_paths
                .iter()
                .map(|path| PathBuf::from(*path))
                .collect::<Vec<_>>(),
        );
        for path in paths {
            assert_eq!(
                stable_path_with_roots(Path::new(path), &roots),
                "[fixtures]/md/references.md"
            );
            let missing = format!("image missing (looked for {path})");
            assert_eq!(
                diagnostic_message("missing-asset", &missing, &roots),
                "image missing (looked for [fixtures]/md/references.md)"
            );
            let duplicate = format!("duplicate (first defined at {path}:36)");
            assert_eq!(
                diagnostic_message("duplicate-label", &duplicate, &roots),
                "duplicate (first defined at [fixtures]/md/references.md:36)"
            );
        }
    }
}

fn snapshot(output: &Emitted, diagnostics: &Diagnostics) -> Value {
    json!({
        "body":output.body,"abstract":output.abstract_,"toc":output.toc,
        "usage":format!("{:#?}",output.usage),
        "images":output.images.iter().enumerate().map(|(index,image)| json!({
            "identity":format!("asset-{}",index+1),"name":image.name,"render":image.render,
        })).collect::<Vec<_>>(),
        "bibliography":output.bibliography.iter().map(|(path,name)| json!({
            "source":stable_path(path),"name":name,
        })).collect::<Vec<_>>(),
        "diagnostics":findings(diagnostics),
    })
}

fn parsed(path: &Path, config: &Config) -> Result<Parsed, Error> {
    let source = source(path);
    let text = fs::read_to_string(path).unwrap();
    match source.kind {
        SourceKind::Markdown => writer::markdown::parse(&source, &text, config),
        SourceKind::Notebook => writer::notebook::parse(&source, &text, config),
        SourceKind::Typst => {
            writer::typst::frontend::parse(&source, &text, config)
        }
    }
}

fn case(
    path: &Path, mut config: Config,
) -> Result<(Emitted, Diagnostics), Error> {
    let mut parsed = parsed(path, &config)?;
    config.merge_front_matter(&parsed.document.meta).unwrap();
    let library = citation_library::load(&parsed.document, &config);
    parsed.diagnostics.extend(resolve(
        &mut parsed.document,
        &ResolveOptions {
            config: &config,
            library: library.as_ref(),
            lenient: true,
        },
    ));
    assert!(
        parsed.document.invariant_violations().is_empty(),
        "{}",
        path.display()
    );
    let output = emit_checked(&parsed.document, &parsed.assets, &config);
    parsed.diagnostics.extend(output.diagnostics.clone());
    Ok((output, parsed.diagnostics))
}

fn parse_error(error: &Error) -> String {
    let text = error.to_string();
    // Only replace the Display prefix known to be Error::Parse's path. Error
    // messages and every emitted source byte remain untouched.
    if let Error::Parse { path, .. } = error
        && let Some(rest) = text.strip_prefix(&path.display().to_string())
    {
        return format!("{}{rest}", stable_path(path));
    }
    text
}

#[test]
fn every_top_level_markdown_fixture_emits_a_body_and_honest_diagnostics() {
    let mut paths = fs::read_dir(fixtures().join("md"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension().is_some_and(|extension| extension == "md")
        })
        .collect::<Vec<_>>();
    paths.sort();
    assert_eq!(paths.len(), 20);
    for path in paths {
        let name = path.file_stem().unwrap().to_str().unwrap();
        match case(&path, Config::default()) {
            Ok((output, diagnostics)) => insta::assert_json_snapshot!(
                name,
                snapshot(&output, &diagnostics)
            ),
            Err(error) => insta::assert_snapshot!(name, parse_error(&error)),
        }
    }
}

#[test]
fn tour_covers_every_code_and_citation_mode_with_the_configured_library() {
    for (code_name, code) in [
        ("verbatim", LatexCode::Verbatim),
        ("highlighted", LatexCode::Highlighted),
        ("listings", LatexCode::Listings),
    ] {
        for (cite_name, cite) in [
            ("biblatex", LatexCite::Biblatex),
            ("natbib", LatexCite::Natbib),
            ("plain", LatexCite::Plain),
        ] {
            let mut config = Config::default();
            config.latex.code = code;
            config.latex.cite = cite;
            let (output, diagnostics) =
                case(&fixtures().join("latex/tour.md"), config).unwrap();
            assert!(!diagnostics.has_errors(), "{diagnostics:?}");
            latex_citations::assert_tour_commands(&output.body, cite);
            let unsupported = diagnostics
                .iter()
                .filter(|finding| {
                    finding.code == "unsupported-construct"
                        && finding.message.starts_with("latex: citation mode ")
                })
                .count();
            assert_eq!(
                unsupported,
                match cite {
                    LatexCite::Biblatex => 0,
                    LatexCite::Natbib => 1,
                    LatexCite::Plain => 4,
                }
            );
            assert!(!output.abstract_.is_empty());
            insta::assert_json_snapshot!(
                format!("tour_{code_name}_{cite_name}"),
                snapshot(&output, &diagnostics)
            );
        }
    }
}

#[test]
fn notebook_output_snapshots_respect_prompt_configuration() {
    for prompts in [false, true] {
        let mut config = Config::default();
        config.latex.cell_prompts = prompts;
        let (output, diagnostics) =
            case(&fixtures().join("ipynb/outputs.ipynb"), config).unwrap();
        assert_eq!(output.usage.cells, prompts);
        insta::assert_json_snapshot!(
            format!("notebook_prompts_{prompts}"),
            snapshot(&output, &diagnostics)
        );
    }
}

#[test]
fn native_typst_math_fixture_uses_the_public_converter_and_its_findings() {
    let (output, diagnostics) =
        case(&fixtures().join("typ/math.typ"), Config::default()).unwrap();
    assert!(output.usage.math);
    assert!(
        diagnostics.iter().any(|finding| finding.code == "E-TYP-MATH-CODE")
    );
    insta::assert_json_snapshot!(
        "native_typst_math",
        snapshot(&output, &diagnostics)
    );
}

#[test]
fn actual_verbatim_terminators_are_errors_with_safe_dump_output() {
    let path = fixtures().join("latex/verbatim-end.md");
    let (output, diagnostics) = case(&path, Config::default()).unwrap();
    let finding = diagnostics
        .iter()
        .find(|finding| finding.code == "latex-verbatim-end")
        .unwrap();
    assert_eq!(finding.level, Level::Error);
    assert_eq!(finding.path.as_ref(), Some(&path));
    assert!(output.body.contains("\\begin{writercode}"));
    assert!(!output.body.contains("\\begin{Verbatim}"));
    insta::assert_json_snapshot!(
        "verbatim_end",
        snapshot(&output, &diagnostics)
    );
    for (mode, end) in [
        (LatexCode::Highlighted, "\\end{Verbatim}"),
        (LatexCode::Listings, "\\end{lstlisting}"),
    ] {
        let mut config = Config::default();
        config.latex.code = mode;
        let doc = document(vec![Block::CodeBlock {
            lang: None,
            text: format!("prefix {end} suffix"),
            attrs: Attrs::default(),
            span: None,
        }]);
        let output = emit_checked(&doc, &AssetRegistry::new(), &config);
        let errors = output
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "latex-verbatim-end")
            .collect::<Vec<_>>();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].level, Level::Error);
        assert!(output.body.contains("\\begin{writercode}"));
        assert!(!output.body.contains("\\begin{Verbatim}"));
        assert!(!output.body.contains("\\begin{lstlisting}"));
    }
}

fn cite_item(key: &str, notes: bool) -> CiteItem {
    CiteItem {
        key: key.into(),
        locator: notes.then(|| "p. 12".into()),
        prefix: notes.then(|| vec![text("see")]),
        suffix: notes
            .then(|| vec![Inline::Emph { inlines: vec![text("more")] }]),
    }
}

fn cite_document(mode: CiteMode, group: bool, notes: bool) -> Document {
    let mut items = vec![cite_item("a", notes)];
    if group {
        items.push(cite_item("b", false));
    }
    let mut doc = document(vec![paragraph(vec![Inline::Cite {
        items,
        mode,
        span: None,
        resolved: Some(ResolvedCite {
            inlines: vec![text("DO-NOT-USE-CITE-CACHE")],
            note: true,
        }),
    }])]);
    doc.features.citations = true;
    doc
}

#[test]
fn handbuilt_citation_matrix_covers_all_modes_notes_groups_and_cached_note_ignorance()
 {
    let rows = [
        (CiteMode::Normal, "\\autocite", "\\citep", "\\cite"),
        (CiteMode::Prose, "\\textcite", "\\citet", "\\cite"),
        (CiteMode::Year, "\\autocite*", "\\citeyearpar", "\\cite"),
        (CiteMode::Author, "\\citeauthor", "\\citeauthor", "\\cite"),
        (CiteMode::Full, "\\fullcite", "\\citep", "\\cite"),
        (CiteMode::None, "\\nocite", "\\nocite", "\\nocite"),
    ];
    for (mode, biblatex, natbib, plain) in rows {
        for (style, command) in [
            (LatexCite::Biblatex, biblatex),
            (LatexCite::Natbib, natbib),
            (LatexCite::Plain, plain),
        ] {
            for group in [false, true] {
                let mut config = Config::default();
                config.latex.cite = style;
                let output = emit_checked(
                    &cite_document(mode, group, false),
                    &AssetRegistry::new(),
                    &config,
                );
                let keys = if group { "a,b" } else { "a" };
                assert_eq!(
                    output.body,
                    format!("{command}{{{keys}}}\n"),
                    "{style:?}/{mode:?}"
                );
                assert!(!output.body.contains("DO-NOT-USE-CITE-CACHE"));
                let unsupported = output
                    .diagnostics
                    .iter()
                    .filter(|finding| finding.code == "unsupported-construct")
                    .count();
                let expected = match style {
                    LatexCite::Biblatex => false,
                    LatexCite::Natbib => mode == CiteMode::Full,
                    LatexCite::Plain => {
                        !matches!(mode, CiteMode::Normal | CiteMode::None)
                    }
                };
                assert_eq!(
                    unsupported,
                    usize::from(expected),
                    "{style:?}/{mode:?}"
                );
            }
        }
    }
    let cases = [
        (
            LatexCite::Biblatex,
            CiteMode::Normal,
            "\\autocites[see][p.~12, \\emph{more}]{a}{b}",
        ),
        (
            LatexCite::Biblatex,
            CiteMode::Prose,
            "\\textcites[see][p.~12, \\emph{more}]{a}{b}",
        ),
        (
            LatexCite::Biblatex,
            CiteMode::Year,
            "\\autocite*[see][p.~12, \\emph{more}]{a}; \\autocite*{b}",
        ),
        (
            LatexCite::Biblatex,
            CiteMode::Author,
            "\\citeauthor[see][p.~12, \\emph{more}]{a}; \\citeauthor{b}",
        ),
        (
            LatexCite::Biblatex,
            CiteMode::Full,
            "\\fullcite[see][p.~12, \\emph{more}]{a}; \\fullcite{b}",
        ),
        (
            LatexCite::Natbib,
            CiteMode::Normal,
            "\\citetext{\\citealp[see][p.~12, \\emph{more}]{a}; \\citealp{b}}",
        ),
        (
            LatexCite::Natbib,
            CiteMode::Prose,
            "\\citet[see][p.~12, \\emph{more}]{a}; \\citet{b}",
        ),
        (
            LatexCite::Natbib,
            CiteMode::Year,
            "\\citeyearpar[see][p.~12, \\emph{more}]{a}; \\citeyearpar{b}",
        ),
        (
            LatexCite::Natbib,
            CiteMode::Author,
            "\\citeauthor{a}; \\citeauthor{b}",
        ),
        (
            LatexCite::Natbib,
            CiteMode::Full,
            "\\citetext{\\citealp[see][p.~12, \\emph{more}]{a}; \\citealp{b}}",
        ),
        (
            LatexCite::Plain,
            CiteMode::Normal,
            "see \\cite[p.~12, \\emph{more}]{a}; \\cite{b}",
        ),
        (
            LatexCite::Plain,
            CiteMode::Prose,
            "see \\cite[p.~12, \\emph{more}]{a}; \\cite{b}",
        ),
        (
            LatexCite::Plain,
            CiteMode::Year,
            "see \\cite[p.~12, \\emph{more}]{a}; \\cite{b}",
        ),
        (
            LatexCite::Plain,
            CiteMode::Author,
            "see \\cite[p.~12, \\emph{more}]{a}; \\cite{b}",
        ),
        (
            LatexCite::Plain,
            CiteMode::Full,
            "see \\cite[p.~12, \\emph{more}]{a}; \\cite{b}",
        ),
    ];
    for (style, mode, expected) in cases {
        let mut config = Config::default();
        config.latex.cite = style;
        let output = emit_checked(
            &cite_document(mode, true, true),
            &AssetRegistry::new(),
            &config,
        );
        assert_eq!(output.body, format!("{expected}\n"), "{style:?}/{mode:?}");
    }
    for style in [LatexCite::Biblatex, LatexCite::Natbib, LatexCite::Plain] {
        let mut config = Config::default();
        config.latex.cite = style;
        let output = emit_checked(
            &cite_document(CiteMode::None, true, true),
            &AssetRegistry::new(),
            &config,
        );
        assert_eq!(output.body, "\\nocite{a,b}\n");
        assert!(output.diagnostics.is_empty());
    }
}

#[test]
fn citation_mode_warnings_are_once_per_mode_and_invalid_keys_are_located_errors()
 {
    let mut blocks = Vec::new();
    for mode in
        [CiteMode::Prose, CiteMode::Year, CiteMode::Author, CiteMode::Full]
    {
        for _ in 0..3 {
            blocks.extend(cite_document(mode, true, true).blocks);
        }
    }
    let mut doc = document(blocks);
    doc.features.citations = true;
    let mut config = Config::default();
    config.latex.cite = LatexCite::Plain;
    let output = emit_checked(&doc, &AssetRegistry::new(), &config);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "unsupported-construct")
            .count(),
        4
    );
    let span = SourceSpan {
        start_line: 7,
        end_line: 8,
        cell: Some(2),
        file: Some("/source/included.ipynb".into()),
    };
    for key in [
        "has space", "a,b", "a{b", "a}b", "a%b", "a#b", "a\\b", "a~b", "a\"b",
        "a\tb", "a\nb",
    ] {
        let doc = document(vec![Block::Paragraph {
            inlines: vec![Inline::Cite {
                items: vec![cite_item(key, false)],
                mode: CiteMode::Normal,
                resolved: None,
                span: Some(span.clone()),
            }],
            span: Some(span.clone()),
        }]);
        let output = emit_checked(&doc, &AssetRegistry::new(), &config);
        let errors = output
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "latex-cite-key")
            .collect::<Vec<_>>();
        assert_eq!(errors.len(), 1, "{key:?}: {:?}", output.diagnostics);
        assert_eq!(errors[0].level, Level::Error);
        assert_eq!(errors[0].span.as_ref(), Some(&span));
        assert_eq!(errors[0].path, span.file);
    }
}

struct Equation {
    name: String,
    source: String,
    display: bool,
    path: PathBuf,
}

fn equations(kind: &str) -> Vec<Equation> {
    let extension = if kind == "latex" { "tex" } else { "typ" };
    let mut paths = fs::read_dir(fixtures().join("math").join(kind))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == extension))
        .collect::<Vec<_>>();
    paths.sort();
    assert_eq!(paths.len(), if kind == "latex" { 37 } else { 36 });
    paths
        .into_iter()
        .map(|path| {
            let text = fs::read_to_string(&path).unwrap();
            let (mode, body) = text.split_once('\n').unwrap();
            let body = body.strip_suffix('\n').unwrap_or(body);
            let body = if kind == "latex" {
                body.strip_prefix('$')
                    .and_then(|body| body.strip_suffix('$'))
                    .unwrap_or(body)
            } else {
                body
            };
            Equation {
                name: path.file_stem().unwrap().to_str().unwrap().into(),
                source: body.into(),
                display: mode.ends_with(": display"),
                path,
            }
        })
        .collect()
}

fn math_document(equation: &Equation, dialect: MathDialect) -> Document {
    let span = Some(SourceSpan {
        start_line: 2,
        end_line: 2,
        cell: None,
        file: Some(equation.path.clone()),
    });
    let mut doc = document(vec![if equation.display {
        Block::Math {
            source: equation.source.clone(),
            dialect,
            attrs: Attrs::default(),
            number: None,
            span,
        }
    } else {
        Block::Paragraph {
            inlines: vec![Inline::Math {
                source: equation.source.clone(),
                dialect,
            }],
            span,
        }
    }]);
    doc.features.math = true;
    doc
}

fn unnumbered_math(source: &str, display: bool) -> String {
    if !display {
        return format!("\\({source}\\)\n");
    }
    for name in ["align", "gather", "multline", "equation", "eqnarray"] {
        for spelling in [name.to_owned(), format!("{name}*")] {
            let begin = format!("\\begin{{{spelling}}}");
            if source.trim_start().starts_with(&begin) {
                let mut changed =
                    source.replacen(&begin, &format!("\\begin{{{name}*}}"), 1);
                let end = format!("\\end{{{spelling}}}");
                let at = changed.rfind(&end).unwrap();
                changed.replace_range(
                    at..at + end.len(),
                    &format!("\\end{{{name}*}}"),
                );
                return format!("{changed}\n");
            }
        }
    }
    format!("\\[\n{source}\n\\]\n")
}

#[test]
fn all_37_latex_equations_pass_through_even_when_other_math_backends_reject_them()
 {
    for equation in equations("latex") {
        let output = emit_checked(
            &math_document(&equation, MathDialect::Latex),
            &AssetRegistry::new(),
            &Config::default(),
        );
        assert_eq!(
            output.body,
            unnumbered_math(&equation.source, equation.display),
            "{}",
            equation.name
        );
        assert!(output.usage.math);
        assert!(
            !output
                .diagnostics
                .iter()
                .any(|finding| finding.code == "math-syntax")
        );
        if ["18-unknown-command", "19-unbalanced-brace", "32-ce", "37-sideset"]
            .contains(&equation.name.as_str())
        {
            assert!(
                !output
                    .diagnostics
                    .iter()
                    .any(|finding| finding.code == "math-conversion-failed"),
                "{}: {:?}",
                equation.name,
                output.diagnostics
            );
        }
    }
}

#[test]
fn all_36_native_equations_use_exact_t51_output_and_retain_converter_findings()
{
    for equation in equations("typst") {
        let (expected, diagnostics) = typst_to_latex::convert(&equation.source);
        let output = emit_checked(
            &math_document(&equation, MathDialect::Typst),
            &AssetRegistry::new(),
            &Config::default(),
        );
        assert_eq!(
            output.body,
            unnumbered_math(&expected, equation.display),
            "{}",
            equation.name
        );
        let codes = diagnostics
            .iter()
            .map(|finding| finding.code)
            .collect::<BTreeSet<_>>();
        let actual = output
            .diagnostics
            .iter()
            .filter(|finding| codes.contains(finding.code))
            .collect::<Vec<_>>();
        assert_eq!(
            actual.len(),
            diagnostics.len(),
            "{}: {:?}",
            equation.name,
            output.diagnostics
        );
        for (actual, expected) in actual.into_iter().zip(diagnostics) {
            assert_eq!(
                (actual.code, &actual.message, actual.level),
                (expected.code, &expected.message, expected.level)
            );
            assert_eq!(actual.path.as_ref(), Some(&equation.path));
            assert_eq!(actual.span.as_ref().unwrap().start_line, 2);
        }
    }
}

fn reference(
    target: &str, form: RefForm, supplement: Option<Inlines>,
) -> Inline {
    Inline::Ref {
        target: target.into(),
        form,
        supplement,
        span: None,
        resolved: Some(ResolvedRef {
            kind: LabelKind::Equation,
            number: "IR-NUMBER-MUST-NOT-PRINT".into(),
            text: vec![text("Resolved title")],
            anchor: "DO-NOT-USE-CACHED-ANCHOR".into(),
        }),
    }
}

fn with_label(
    mut doc: Document, target: &str, number: Option<&str>,
) -> Document {
    doc.resolved.labels.insert(
        target.into(),
        LabelInfo {
            kind: LabelKind::Equation,
            number: number.map(str::to_owned),
            supplement: "Equation".into(),
            text: Some("Resolved title".into()),
            span: None,
        },
    );
    doc
}

#[test]
fn reference_forms_keep_empty_supplements_page_precedence_and_logical_anchors()
{
    let target = "eq-logical";
    let label = anchor(target);
    for (form, supplement, cleveref, expected) in [
        (RefForm::Normal, None, false, format!("Equation~\\eqref{{{label}}}")),
        (
            RefForm::Normal,
            Some(Vec::new()),
            false,
            format!("\\eqref{{{label}}}"),
        ),
        (
            RefForm::Normal,
            Some(vec![text("Eq.")]),
            false,
            format!("Eq.~\\eqref{{{label}}}"),
        ),
        (RefForm::Normal, None, true, format!("\\Cref{{{label}}}")),
        (
            RefForm::Normal,
            Some(Vec::new()),
            true,
            format!("\\eqref{{{label}}}"),
        ),
        (RefForm::Page, None, false, format!("page~\\pageref{{{label}}}")),
        (RefForm::Page, None, true, format!("\\cpageref{{{label}}}")),
        (
            RefForm::Page,
            Some(Vec::new()),
            true,
            format!("\\pageref{{{label}}}"),
        ),
        (
            RefForm::Page,
            Some(vec![text("on page")]),
            false,
            format!("on page~\\pageref{{{label}}}"),
        ),
    ] {
        let doc = with_label(
            document(vec![paragraph(vec![reference(
                target, form, supplement,
            )])]),
            target,
            Some("99"),
        );
        let mut config = Config::default();
        config.latex.cleveref = cleveref;
        let output = emit_checked(&doc, &AssetRegistry::new(), &config);
        assert_eq!(output.body, format!("{expected}\n"));
        assert!(
            !output.body.contains("IR-NUMBER")
                && !output.body.contains("CACHED-ANCHOR")
        );
    }
    for cleveref in [false, true] {
        let mut config = Config::default();
        config.latex.cleveref = cleveref;
        let normal = with_label(
            document(vec![paragraph(vec![reference(
                target,
                RefForm::Normal,
                None,
            )])]),
            target,
            None,
        );
        assert_eq!(
            emit_checked(&normal, &AssetRegistry::new(), &config).body,
            format!("\\hyperref[{label}]{{Resolved title}}\n")
        );
        let page = with_label(
            document(vec![paragraph(vec![reference(
                target,
                RefForm::Page,
                None,
            )])]),
            target,
            None,
        );
        let expected = if cleveref {
            format!("\\cpageref{{{label}}}\n")
        } else {
            format!("page~\\pageref{{{label}}}\n")
        };
        assert_eq!(
            emit_checked(&page, &AssetRegistry::new(), &config).body,
            expected
        );
    }
    let doc = with_label(
        document(vec![paragraph(vec![
            reference(target, RefForm::Normal, None),
            text(". Then "),
            reference(target, RefForm::Normal, None),
            text(" ended. "),
            reference(target, RefForm::Normal, None),
        ])]),
        target,
        Some("99"),
    );
    let mut config = Config::default();
    config.latex.cleveref = true;
    assert_eq!(
        emit_checked(&doc, &AssetRegistry::new(), &config).body,
        format!(
            "\\Cref{{{label}}}. Then \\cref{{{label}}} ended. \\Cref{{{label}}}\n"
        )
    );
    let unknown = document(vec![paragraph(vec![inline(json!({
        "type":"ref","target":"absent","form":"normal","supplement":null,
    }))])]);
    assert_eq!(
        emit_checked(&unknown, &AssetRegistry::new(), &Config::default()).body,
        "{[}?absent{]}\n"
    );
}

#[test]
fn headings_and_tocs_use_offsets_and_moving_argument_forms_without_ir_numbers()
{
    for (top, command, depth, last) in [
        (LatexTopLevel::Section, "section", 5, "subparagraph"),
        (LatexTopLevel::Chapter, "chapter", 5, "subparagraph"),
        (LatexTopLevel::Part, "part", 4, "paragraph"),
    ] {
        let doc = document(vec![block(json!({"type":"heading","level":6,
            "inlines":[{"type":"text","text":"Deep"}],"attrs":{},"number":"SECRET-99"}))]);
        let mut config = Config::default();
        config.latex.top_level = top;
        let output = emit_checked(&doc, &AssetRegistry::new(), &config);
        assert_eq!(output.body, format!("\\{last}{{Deep}}\n"));
        assert_eq!(output.usage.secnumdepth, Some(depth));
        let doc = document(vec![block(json!({"type":"heading","level":1,
            "inlines":[{"type":"text","text":"Top"}],"attrs":{},"number":"SECRET-99"}))]);
        assert_eq!(
            emit_checked(&doc, &AssetRegistry::new(), &config).body,
            format!("\\{command}{{Top}}\n")
        );
        let doc = document(vec![
            Block::Toc { depth: 3, span: None },
            paragraph(vec![text("Body")]),
        ]);
        let output = emit_checked(&doc, &AssetRegistry::new(), &config);
        assert_eq!(output.toc, 3);
        assert_eq!(output.body, "Body\n");
        let offset = match top {
            LatexTopLevel::Section => 0,
            LatexTopLevel::Chapter => -1,
            LatexTopLevel::Part => -2,
        };
        assert_eq!(output.usage.tocdepth, Some(3 + offset));
    }
    let doc = document(vec![Block::Heading {
        level: 1,
        number: Some("SECRET-99".into()),
        span: None,
        attrs: Attrs {
            id: Some("moving".into()),
            classes: vec!["notoc".into()],
            ..Attrs::default()
        },
        inlines: vec![
            text("Title "),
            Inline::Math { source: "x_1".into(), dialect: MathDialect::Latex },
            text(" "),
            Inline::Footnote {
                blocks: vec![paragraph(vec![text("Note.")])],
                label: None,
                number: Some(1),
            },
            text(" "),
            Inline::Strikeout { inlines: vec![text("gone")] },
            text(" "),
            Inline::Link {
                url: "https://example.org".into(),
                title: None,
                attrs: Attrs::default(),
                inlines: vec![text("https://example.org")],
            },
        ],
    }]);
    let output = emit_checked(&doc, &AssetRegistry::new(), &Config::default());
    assert!(output.body.starts_with("\\writernotoc\n\\section["));
    assert_eq!(output.body.matches("\\protect\\footnote").count(), 1);
    assert!(output.body.contains("\\protect\\sout{gone}"));
    assert!(output.body.contains("\\texorpdfstring{\\(x_1\\)}{x\\_1}"));
    assert!(output.body.contains(
        "\\texorpdfstring{\\url{https://example.org}}{https://example.org}"
    ));
    assert_eq!(
        output
            .body
            .matches(&format!("\\label{{{}}}", anchor("moving")))
            .count(),
        1
    );
    assert!(!output.body.contains("SECRET-99"));
    let mut config = Config::default();
    config.latex.tables = LatexTables::Tabular;
    let outer = document(vec![Block::Figure {
        kind: FigureKind::Table,
        caption: Some(vec![text("Outer caption")]),
        body: vec![table()],
        attrs: Attrs::with_id("tbl-outer"),
        number: Some("SECRET-98".into()),
        span: None,
    }]);
    let output = emit_checked(&outer, &AssetRegistry::new(), &config);
    assert!(output.body.starts_with("\\begin{table}[htbp]\n\\centering\n"));
    assert!(
        output.body.find("\\caption{Outer caption}").unwrap()
            < output.body.find("\\begin{tabular}").unwrap()
    );
    assert!(!output.usage.longtable && !output.usage.uncaptioned_longtable);
    assert!(!output.body.contains("SECRET-98"));
    let nested = document(vec![Block::Callout {
        kind: "note".into(),
        title: None,
        attrs: Attrs::default(),
        span: None,
        blocks: vec![Block::Toc { depth: 2, span: None }],
    }]);
    let output =
        emit_checked(&nested, &AssetRegistry::new(), &Config::default());
    assert_eq!(output.toc, 0);
    assert!(
        output.body.contains("\\setcounter{tocdepth}{2}\n\\tableofcontents")
    );
}

fn table() -> Block {
    block(json!({"type":"table","columns":[
        {"align":"left","width":0.25},{"align":"center","width":null},{"align":"right","width":0.25}],
        "head":[{"cells":[
            {"inlines":[{"type":"text","text":"HEAD-SPAN"}],"rowspan":3},
            {"inlines":[{"type":"text","text":"HEAD-A"}],"colspan":2}]},
            {"cells":[{"inlines":[{"type":"text","text":"HEAD-B"}]},{"inlines":[{"type":"text","text":"HEAD-C"}]}]}],
        "body":[{"cells":[
            {"inlines":[{"type":"text","text":"BODY-SPAN"}],"rowspan":3,"colspan":2,"align":"right"},
            {"inlines":[{"type":"text","text":"BODY-A"}]}]},
            {"cells":[{"inlines":[{"type":"text","text":"BODY-B"}]}]}],
        "foot":[{"cells":[{"inlines":[{"type":"text","text":"*FOOT"}]}]},
            {"cells":[{"inlines":[{"type":"raw","format":"latex","text":"[FOOT"}]}]}],"attrs":{},
        "span":{"start_line":7,"end_line":12,"cell":2,"file":"/source/table.ipynb"}}))
}

#[test]
fn table_grid_sections_widths_alignment_and_longtable_counter_policy_are_explicit()
 {
    let doc = document(vec![table()]);
    let output = emit_checked(&doc, &AssetRegistry::new(), &Config::default());
    assert!(
        output.usage.table
            && output.usage.longtable
            && output.usage.uncaptioned_longtable
    );
    assert!(
        output.body.starts_with("{\\def\\LTcaptype{none}\n\\begin{longtable}")
    );
    for (align, width) in
        [("raggedright", "0.25"), ("centering", "0.5"), ("raggedleft", "0.25")]
    {
        assert!(output.body.contains(&format!(">{{\\{align}\\arraybackslash}}p{{\\dimexpr {width}\\linewidth-2\\tabcolsep\\relax}}")), "{}", output.body);
    }
    assert!(output.body.contains("0.75\\linewidth-2\\tabcolsep\\relax"));
    assert!(output.body.contains("\\multicolumn{2}{>{\\raggedleft\\arraybackslash}p{\\dimexpr 0.75\\linewidth-2\\tabcolsep\\relax}}{BODY-SPAN}"));
    assert!(output.body.contains(" &  & BODY-B"), "{}", output.body);
    assert!(output.body.contains("BODY-A"));
    assert!(output.body.contains("{}*FOOT"));
    assert!(output.body.contains("{}[FOOT"));
    assert_eq!(output.body.matches("HEAD-SPAN").count(), 2);
    assert_eq!(output.body.matches("\\endfirsthead").count(), 1);
    assert_eq!(output.body.matches("\\endhead").count(), 1);
    let warnings = output
        .diagnostics
        .iter()
        .filter(|finding| finding.code == "latex-rowspan")
        .collect::<Vec<_>>();
    assert_eq!(warnings.len(), 1);
    assert_eq!(
        warnings[0].path.as_deref(),
        Some(Path::new("/source/table.ipynb"))
    );
    assert_eq!(warnings[0].span.as_ref().unwrap().start_line, 7);
    let boxed = document(vec![Block::Callout {
        kind: "note".into(),
        title: None,
        attrs: Attrs::default(),
        span: None,
        blocks: vec![Block::Figure {
            kind: FigureKind::Table,
            caption: Some(vec![text("Boxed caption")]),
            body: vec![table()],
            attrs: Attrs::with_id("tbl-boxed"),
            number: Some("SECRET-99".into()),
            span: None,
        }],
    }]);
    let output =
        emit_checked(&boxed, &AssetRegistry::new(), &Config::default());
    assert!(
        !output.body.contains("\\begin{longtable}")
            && !output.body.contains("\\begin{table}")
    );
    assert!(output.body.contains("\\captionsetup{type=table}"));
    assert!(
        output.body.find("\\caption{Boxed caption}").unwrap()
            < output.body.find("\\begin{tabular}").unwrap()
    );
    assert!(!output.body.contains("SECRET-99"));
    let malformed = document(vec![block(json!({"type":"table",
        "columns":[{"align":"default"},{"align":"default"}],
        "head":[],"foot":[],"attrs":{},"body":[{"cells":[
            {"inlines":[{"type":"text","text":"TOO-WIDE"}],"colspan":3},
            {"inlines":[{"type":"text","text":"LATER-FITS"}]}]}]}))]);
    let output =
        emit_checked(&malformed, &AssetRegistry::new(), &Config::default());
    assert!(output.body.contains("LATER-FITS"), "{}", output.body);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "unsupported-construct")
            .count(),
        1
    );
    assert_eq!(
        output.body.matches("\\begin{longtable}").count(),
        output.body.matches("\\end{longtable}").count()
    );
}

#[test]
fn footnote_context_keeps_safe_code_and_restores_outer_verbatim_afterward() {
    let note_code = "\\end{Verbatim}\n  keep spaces";
    let doc = document(vec![
        paragraph(vec![
            Inline::Footnote {
                blocks: vec![Block::CodeBlock {
                    lang: None,
                    text: note_code.into(),
                    attrs: Attrs::default(),
                    span: None,
                }],
                label: Some("logical-note".into()),
                number: Some(7),
            },
            Inline::FootnoteRef {
                label: "ignored-logical-name".into(),
                number: Some(7),
            },
            Inline::Footnote {
                blocks: vec![paragraph(vec![Inline::Footnote {
                    blocks: vec![paragraph(vec![text("Nested")])],
                    label: None,
                    number: Some(9),
                }])],
                label: None,
                number: Some(8),
            },
        ]),
        Block::CodeBlock {
            lang: None,
            text: "outside code".into(),
            attrs: Attrs::default(),
            span: None,
        },
    ]);
    let output = emit_checked(&doc, &AssetRegistry::new(), &Config::default());
    assert!(output.body.contains("\\label{writer-fn-7}"));
    assert!(output.body.contains("\\footref{writer-fn-7}"));
    assert!(!output.body.contains("\\label{writer-fn-8}"));
    assert!(output.body.contains("\\ttfamily "));
    assert!(output.body.contains("\\textbackslash{}end\\{Verbatim\\}"));
    assert_eq!(output.body.matches("\\begin{Verbatim}").count(), 1);
    assert!(
        output
            .body
            .contains("\\begin{Verbatim}\noutside code\n\\end{Verbatim}")
    );
    assert!(
        !output
            .diagnostics
            .iter()
            .any(|finding| finding.code == "latex-verbatim-end")
    );
    assert_eq!(
        output
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "latex-nested-footnote")
            .count(),
        1
    );
}

fn image(id: &str, alt: &str) -> Inline {
    Inline::Image {
        alt: vec![text(alt)],
        url: format!("asset:{id}"),
        title: None,
        attrs: Attrs::default(),
    }
}

#[test]
fn assets_are_pure_plans_in_abstract_first_use_order_with_correct_rendition_flags()
 {
    let mut registry = AssetRegistry::new();
    let png = registry.register_file(Path::new("/nonexistent/picture.png"));
    let svg =
        registry.register_bytes(vec![1, 2], "image/svg+xml", "diagram.svg");
    let gif = registry.register_bytes(vec![3, 4], "image/gif", "anim.gif");
    let pdf =
        registry.register_bytes(vec![5, 6], "application/pdf", "report.PDF");
    let jpeg = registry.register_bytes(vec![7, 8], "image/jpeg", "photo.JPEG");
    let mismatch =
        registry.register_bytes(vec![9, 10], "image/png", "source.data");
    let mut doc = document(vec![paragraph(vec![
        image(&svg, "SVG"),
        image(&png, "Repeat"),
        image(&gif, "GIF"),
        image(&pdf, "PDF"),
        image(&jpeg, "JPEG"),
        image(&mismatch, "PNG"),
    ])]);
    doc.meta.abstract_ = Some(vec![paragraph(vec![image(&png, "Abstract")])]);
    let output = emit_checked(&doc, &registry, &Config::default());
    assert_eq!(
        plan(&output),
        vec![
            (png.clone(), "picture.png".into(), false),
            (svg, "diagram.svg.pdf".into(), true),
            (gif, "anim.gif.pdf".into(), true),
            (pdf, "report.PDF".into(), false),
            (jpeg, "photo.JPEG".into(), false),
            (mismatch, "source.data.png".into(), false),
        ]
    );
    assert!(output.usage.graphics);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert!(output.abstract_.contains("paper.final_assets/picture.png"));
    assert!(output.body.contains("paper.final_assets/diagram.svg.pdf"));
}

#[test]
fn hidden_or_unpicked_notebook_content_contributes_no_usage_assets_or_findings()
{
    let mut registry = AssetRegistry::new();
    let hidden =
        registry.register_bytes(vec![1], "image/svg+xml", "hidden.svg");
    let doc = document(vec![Block::Cell(Cell {
        source: "\\end{Verbatim} 😀".into(),
        language: "rust".into(),
        show_input: false,
        show_output: true,
        outputs: vec![CellOutput {
            candidates: vec![
                OutputCandidate {
                    mime: "text/html".into(),
                    blocks: vec![
                        Block::Raw {
                            format: "html".into(),
                            text: "HTML-ONLY".into(),
                            span: None,
                        },
                        paragraph(vec![image(&hidden, "NOT-SELECTED")]),
                    ],
                },
                OutputCandidate {
                    mime: "text/plain".into(),
                    blocks: vec![paragraph(vec![text("SELECTED-LATEX")])],
                },
            ],
            ..CellOutput::default()
        }],
        ..Cell::default()
    })]);
    let output = emit_checked(&doc, &registry, &Config::default());
    assert_eq!(output.body, "SELECTED-LATEX\n");
    assert!(output.images.is_empty());
    assert!(output.diagnostics.is_empty());
    assert_eq!(output.usage, Usage::default());
    for mode in
        [LatexCode::Verbatim, LatexCode::Highlighted, LatexCode::Listings]
    {
        let mut config = Config::default();
        config.latex.code = mode;
        let output_doc = document(vec![Block::CodeBlock {
            lang: Some("json".into()),
            text: "OUTPUT-BYTES".into(),
            span: None,
            attrs: Attrs {
                classes: vec!["output-json".into()],
                pairs: [("ansi".into(), "ignored".into())]
                    .into_iter()
                    .collect(),
                ..Attrs::default()
            },
        }]);
        let output = emit_checked(&output_doc, &AssetRegistry::new(), &config);
        assert_eq!(
            output.body,
            "\\begin{Verbatim}\nOUTPUT-BYTES\n\\end{Verbatim}\n"
        );
        assert!(output.usage.code);
        assert!(!output.usage.highlighted && !output.usage.listings);
        assert!(output.diagnostics.is_empty());
    }
}

#[test]
fn prompted_result_cells_box_their_tables_and_unprompted_cells_keep_outer_context()
 {
    let doc = document(vec![Block::Cell(Cell {
        show_input: false,
        show_output: true,
        outputs: vec![CellOutput {
            kind: OutputKind::Result,
            execution_count: Some(7),
            candidates: vec![OutputCandidate {
                mime: "text/markdown".into(),
                blocks: vec![table()],
            }],
            ..CellOutput::default()
        }],
        ..Cell::default()
    })]);
    let plain = emit_checked(&doc, &AssetRegistry::new(), &Config::default());
    assert!(plain.body.contains("\\begin{longtable}"));
    assert!(!plain.usage.cells);
    let mut config = Config::default();
    config.latex.cell_prompts = true;
    let prompted = emit_checked(&doc, &AssetRegistry::new(), &config);
    assert!(prompted.body.contains("\\begin{writercell}{Out}{7}"));
    assert!(prompted.body.contains("\\begin{tabular}"));
    assert!(!prompted.body.contains("\\begin{longtable}"));
    assert!(
        prompted.usage.cells
            && prompted.usage.table
            && !prompted.usage.longtable
    );
}

#[test]
fn prose_wrap_does_not_split_code_math_raw_or_url_bytes() {
    let doc = document(vec![paragraph(vec![
        text("Alpha beta gamma delta "),
        Inline::Code {
            text: "code with spaces".into(),
            attrs: Attrs::default(),
        },
        text(" tail words "),
        Inline::Math { source: "a + b".into(), dialect: MathDialect::Latex },
        text(" "),
        Inline::Raw { format: "latex".into(), text: "RAW WITH SPACES".into() },
        text(" "),
        Inline::Link {
            url: "https://example.org/a b".into(),
            inlines: vec![text("https://example.org/a b")],
            title: None,
            attrs: Attrs::default(),
        },
    ])]);
    let mut config = Config::default();
    config.latex.wrap = 18;
    let output = emit_checked(&doc, &AssetRegistry::new(), &config);
    assert!(output.body.starts_with("Alpha beta gamma\n"), "{}", output.body);
    for atomic in [
        "\\texttt{code\\ with\\ spaces}",
        "\\(a + b\\)",
        "RAW WITH SPACES",
        "\\url{https://example.org/a\\%20b}",
    ] {
        assert!(
            output.body.contains(atomic),
            "missing atomic {atomic}: {}",
            output.body
        );
    }
    let mut unicode = Config::default();
    unicode.latex.engine = LatexEngine::Xelatex;
    unicode.latex.unicode = LatexUnicode::Keep;
    unicode.latex.wrap = 3;
    assert_eq!(
        emit_checked(
            &document(vec![paragraph(vec![text("α α α α")])]),
            &AssetRegistry::new(),
            &unicode
        )
        .body,
        "α α\nα α\n"
    );
    let raw = document(vec![
        Block::Raw {
            format: "latex".into(),
            text: "RAW α 😀 % # {bytes}\n\n".into(),
            span: None,
        },
        Block::Raw {
            format: "html".into(),
            text: "DO-NOT-EMIT".into(),
            span: None,
        },
    ]);
    let output = emit_checked(&raw, &AssetRegistry::new(), &Config::default());
    assert_eq!(output.body, "RAW α 😀 % # {bytes}\n");
    assert!(output.diagnostics.is_empty());
}

#[test]
fn package_and_macro_bookkeeping_is_ordered_scoped_and_not_a_math_preflight() {
    let doc = document(vec![
        block(
            json!({"type":"math","source":"cancel(x)","dialect":"typst","attrs":{},"number":null,
        "span":{"start_line":7,"end_line":7,"file":"/source/math.md"}}),
        ),
        block(
            json!({"type":"math","source":"cancel(y)","dialect":"typst","attrs":{},"number":null,
        "span":{"start_line":9,"end_line":9,"file":"/source/math.md"}}),
        ),
    ]);
    let output = emit_checked(&doc, &AssetRegistry::new(), &Config::default());
    assert_eq!(output.usage.math_packages, vec!["cancel"]);
    let info = output
        .diagnostics
        .iter()
        .filter(|finding| finding.code == "math-latex-package")
        .collect::<Vec<_>>();
    assert_eq!(info.len(), 1);
    assert_eq!(info[0].level, Level::Info);
    assert_eq!(info[0].span.as_ref().unwrap().start_line, 7);
    let mut config = Config::default();
    config.math.katex_options.insert("macros".into(),json!({
        "bad":"x","\\bad_name":"x","\\Good":"\\frac{#1}{#2}","\\R":"\\mathbb{R}"
    }));
    let no_math = emit_checked(
        &document(vec![paragraph(vec![text("Only text")])]),
        &AssetRegistry::new(),
        &config,
    );
    assert!(
        !no_math
            .diagnostics
            .iter()
            .any(|finding| finding.code == "math-macro-invalid")
    );
    let output = emit_checked(&doc, &AssetRegistry::new(), &config);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "math-macro-invalid")
            .count(),
        2
    );
    assert!(!output.body.contains("\\providecommand"));
}

#[test]
fn shared_typst_latex_export_names_never_assign_different_bytes_to_one_path() {
    let mut registry = AssetRegistry::new();
    let shared = registry.register_bytes(vec![1], "image/png", "shared.png");
    let svg = registry.register_bytes(vec![2], "image/svg+xml", "plot.pdf");
    let pdf = registry.register_bytes(vec![3], "application/pdf", "plot.pdf");
    let unused = registry.register_bytes(vec![4], "image/png", "plot.pdf");
    let mut doc = document(vec![Block::Cell(Cell {
        show_output: true,
        outputs: vec![CellOutput {
            candidates: vec![
                OutputCandidate {
                    mime: "application/pdf".into(),
                    blocks: vec![
                        Block::Raw {
                            format: "latex".into(),
                            text: String::new(),
                            span: None,
                        },
                        paragraph(vec![image(&pdf, "PDF")]),
                    ],
                },
                OutputCandidate {
                    mime: "image/svg+xml".into(),
                    blocks: vec![paragraph(vec![image(&svg, "SVG")])],
                },
                OutputCandidate {
                    mime: "image/png".into(),
                    blocks: vec![paragraph(vec![image(&unused, "UNPICKED")])],
                },
            ],
            ..CellOutput::default()
        }],
        ..Cell::default()
    })]);
    doc.meta.abstract_ = Some(vec![paragraph(vec![image(&shared, "Shared")])]);
    let config = Config::default();
    let latex = emit_checked(&doc, &registry, &config);
    assert_eq!(
        latex.images.iter().map(|image| image.id.as_str()).collect::<Vec<_>>(),
        [shared.as_str(), pdf.as_str()]
    );
    let typst = writer::typst::emit::emit(
        &doc,
        &registry,
        &writer::typst::emit::EmitOptions {
            config: &config,
            assets: writer::typst::emit::AssetPaths::Export {
                dir: "paper.final_assets".into(),
            },
            template: None,
            bibliography: Vec::new(),
            style: "ieee".into(),
            footnote_prefix: String::new(),
            fragment: false,
        },
    )
    .unwrap();
    assert!(
        typst.assets.contains_key(&svg) && typst.assets.contains_key(&shared)
    );
    assert!(
        !typst.assets.contains_key(&pdf) && !typst.assets.contains_key(&unused)
    );
    let mut files = BTreeMap::new();
    for (id, path) in &typst.assets {
        let basename =
            Path::new(path).file_name().unwrap().to_str().unwrap().to_owned();
        files.insert(basename, registry.read(id).unwrap());
    }
    for image in &latex.images {
        assert!(!image.render);
        let bytes = registry.read(&image.id).unwrap();
        if let Some(existing) = files.insert(image.name.clone(), bytes.clone())
        {
            assert_eq!(existing, bytes, "shared basename {}", image.name);
        }
    }
}

#[test]
fn bibliography_plan_skips_yaml_preserves_order_and_never_reads_or_resolves_sources()
 {
    let mut doc = cite_document(CiteMode::Normal, false, false);
    doc.blocks.push(Block::Bibliography { span: None });
    let mut config = Config::default();
    config.references.bibliography = vec![
        "/unreadable/one/refs.bib".into(),
        "/unreadable/two/refs.bib".into(),
        "/unreadable/skip.YAML".into(),
    ];
    config.latex.emit_bibliography = true;
    for (style, command) in [
        (LatexCite::Biblatex, "\\printbibliography"),
        (
            LatexCite::Natbib,
            "\\bibliographystyle{plainnat}\n\\bibliography{paper.final_assets/refs,paper.final_assets/refs-2}",
        ),
        (
            LatexCite::Plain,
            "\\bibliographystyle{plain}\n\\bibliography{paper.final_assets/refs,paper.final_assets/refs-2}",
        ),
    ] {
        config.latex.cite = style;
        let output = emit_checked(&doc, &AssetRegistry::new(), &config);
        assert_eq!(
            output.bibliography,
            vec![
                (config.references.bibliography[0].clone(), "refs.bib".into()),
                (
                    config.references.bibliography[1].clone(),
                    "refs-2.bib".into()
                )
            ]
        );
        assert!(output.body.contains(command), "{}", output.body);
        assert!(
            !output
                .diagnostics
                .iter()
                .any(|finding| finding.code == "latex-bib-yaml")
        );
        let empty = document(vec![Block::Bibliography { span: None }]);
        assert!(
            emit_checked(&empty, &AssetRegistry::new(), &config)
                .body
                .is_empty()
        );
    }
}

#[test]
fn citation_optional_arguments_distinguish_prefix_only_post_only_and_empty_notes()
 {
    for (style, command) in
        [(LatexCite::Biblatex, "\\autocite"), (LatexCite::Natbib, "\\citep")]
    {
        for (prefix, locator, suffix, options) in [
            (Some(vec![text("see")]), None, None, "[see][]"),
            (None, Some("p. 12".to_owned()), None, "[p.~12]"),
            (
                None,
                Some("p. 12".to_owned()),
                Some(vec![text("more")]),
                "[p.~12, more]",
            ),
            (None, None, None, ""),
        ] {
            let mut doc = document(vec![paragraph(vec![Inline::Cite {
                items: vec![CiteItem {
                    key: "key:with-punctuation_2020".into(),
                    prefix,
                    locator,
                    suffix,
                }],
                mode: CiteMode::Normal,
                resolved: None,
                span: None,
            }])]);
            doc.features.citations = true;
            let mut config = Config::default();
            config.latex.cite = style;
            let output = emit_checked(&doc, &AssetRegistry::new(), &config);
            assert_eq!(
                output.body,
                format!("{command}{options}{{key:with-punctuation_2020}}\n")
            );
            assert!(output.diagnostics.is_empty());
        }
    }
}

#[test]
fn generic_subfigure_labels_share_container_counter_and_restore_outer_float_context()
 {
    let mut registry = AssetRegistry::new();
    let asset = registry.register_bytes(vec![1], "image/png", "panels.png");
    for numbered in [false, true] {
        let child = |id: &str, caption: &str| Block::Figure {
            kind: FigureKind::Image,
            caption: Some(vec![text(caption)]),
            body: vec![paragraph(vec![image(&asset, caption)])],
            attrs: Attrs::with_id(id),
            number: Some("CHILD-NUMBER-MUST-NOT-PRINT".into()),
            span: None,
        };
        let container = Block::Figure {
            kind: FigureKind::Other,
            caption: Some(vec![text("Container caption")]),
            body: vec![
                child("fig-left", "Left panel"),
                child("fig-right", "Right panel"),
            ],
            attrs: Attrs::with_id("fig-panels"),
            number: numbered.then(|| "OUTER-NUMBER-MUST-NOT-PRINT".into()),
            span: None,
        };
        let following = Block::Figure {
            kind: FigureKind::Image,
            caption: None,
            body: vec![paragraph(vec![image(&asset, "Following")])],
            attrs: Attrs::with_id("fig-following"),
            number: Some("NEXT-NUMBER-MUST-NOT-PRINT".into()),
            span: None,
        };
        let output = emit_checked(
            &document(vec![container, following]),
            &registry,
            &Config::default(),
        );
        assert_eq!(output.body.matches("\\begin{figure}").count(), 2);
        assert_eq!(output.body.matches("\\end{figure}").count(), 2);
        assert_eq!(output.body.matches("\\caption{}").count(), 1);
        assert!(output.body.contains("\\par{\\small Left panel}\\par"));
        assert!(output.body.contains("\\par{\\small Right panel}\\par"));
        assert!(!output.body.contains("NUMBER-MUST-NOT-PRINT"));
        let command = if numbered {
            "\\caption{Container caption}"
        } else {
            "\\caption*{Container caption}"
        };
        let caption_at = output.body.find(command).unwrap();
        for id in ["fig-panels", "fig-left", "fig-right"] {
            let label = format!("\\label{{{}}}", anchor(id));
            assert_eq!(output.body.matches(&label).count(), 1);
            assert!(output.body.find(&label).unwrap() > caption_at);
            let unnumbered = format!("\\phantomsection{label}");
            assert_eq!(output.body.contains(&unnumbered), !numbered);
        }
        assert!(output.body.contains("\\includegraphics[width=\\linewidth,height=\\textheight,keepaspectratio]"));
        assert!(output.usage.graphics && output.usage.caption);
        assert_eq!(output.images.len(), 1);
        assert!(output.diagnostics.is_empty());
    }
}

#[test]
fn unsupported_images_keep_escaped_alt_text_and_remote_links_without_asset_io()
{
    let mut registry = AssetRegistry::new();
    let video = registry.register_bytes(vec![1], "video/mp4", "video.mp4");
    let urls = [
        "https://example.org/image.svg?a=1&b=2".to_owned(),
        "data:image/png;base64,AAAA".to_owned(),
        "/absolute/missing.png".to_owned(),
        "relative-missing.png".to_owned(),
        format!("asset:{video}"),
    ];
    let doc = document(vec![paragraph(
        urls.iter()
            .enumerate()
            .map(|(index, url)| Inline::Image {
                alt: vec![text(&format!("ALT-{index} & content"))],
                url: url.clone(),
                title: None,
                attrs: Attrs::default(),
            })
            .collect(),
    )]);
    let output = emit_checked(&doc, &registry, &Config::default());
    assert!(output.body.contains(
        "\\href{https://example.org/image.svg?a=1&b=2}{ALT-0 \\& content}"
    ));
    for index in 0..urls.len() {
        assert!(output.body.contains(&format!("ALT-{index} \\& content")));
    }
    assert!(!output.body.contains("\\includegraphics"));
    assert!(output.images.is_empty());
    assert!(!output.usage.graphics);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "unsupported-construct")
            .count(),
        urls.len()
    );
}
