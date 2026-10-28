//! Quarto-style cell options and notebook visibility (notebook-mapping.md §3).

use std::path::Path;

use serde_json::Value;

use super::nbformat::Cell;
use crate::config::NotebookConfig;
use crate::error::{Diagnostic, Diagnostics};
use crate::ir::SourceSpan;

/// The contiguous cell option comments at the beginning of a source string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionLines {
    /// Option comments with their language-specific delimiters removed.
    pub yaml: String,
    /// Number of source lines consumed.
    pub lines: usize,
    /// The remaining source, kept byte for byte.
    pub rest: String,
}

/// Splits leading option comments from code without interpreting their YAML.
pub fn split_options(source: &str, language: &str) -> OptionLines {
    let (open, close) = comment_markers(language);
    let mut yaml = Vec::new();
    let mut consumed = 0;
    for line in source.split_inclusive('\n') {
        let line_text = line.trim_end_matches(['\r', '\n']);
        let Some(after_open) = line_text.strip_prefix(open) else {
            break;
        };
        let Some(after_bar) = after_open.trim_start().strip_prefix('|') else {
            break;
        };
        let after_bar = after_bar.strip_prefix(' ').unwrap_or(after_bar);
        let text = match close {
            Some(close) => match after_bar.trim_end().strip_suffix(close) {
                Some(text) => text.trim_end(),
                None => break,
            },
            None => after_bar,
        };
        yaml.push(text);
        consumed += line.len();
    }
    OptionLines {
        lines: yaml.len(),
        yaml: yaml.join("\n"),
        rest: source[consumed..].to_owned(),
    }
}

/// Comment delimiters used by Quarto, plus kernel language aliases.
/// Unknown languages use `#`; a closing delimiter is required when present.
pub fn comment_markers(language: &str) -> (&'static str, Option<&'static str>) {
    match language.to_lowercase().as_str() {
        "scala" | "csharp" | "fsharp" | "cpp" | "cc" | "java" | "groovy"
        | "kotlin" | "js" | "javascript" | "typescript" | "d3" | "node"
        | "sass" | "scss" | "go" | "asy" | "dot" | "ojs" | "rust" => {
            ("//", None)
        }
        "matlab" | "tikz" => ("%", None),
        "sql" | "mysql" | "psql" | "lua" | "haskell" => ("--", None),
        "fortran" | "fortran95" => ("!", None),
        "stata" => ("*", None),
        "q" => ("/", None),
        "apl" => ("⍝", None),
        "c" | "css" => ("/*", Some("*/")),
        "sas" => ("*", Some(";")),
        "ocaml" => ("(*", Some("*)")),
        _ => ("#", None),
    }
}

/// Captions distinguish one shared value from a list of per-output values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TextList {
    Scalar(String),
    List(Vec<String>),
}

impl TextList {
    pub(crate) fn at(&self, index: usize) -> Option<&str> {
        match self {
            Self::Scalar(text) => Some(text),
            Self::List(texts) => texts.get(index).map(String::as_str),
        }
    }

    pub(crate) fn is_scalar(&self) -> bool {
        matches!(self, Self::Scalar(_))
    }
}

/// Validated options shared by code-cell conversion and output conversion.
#[derive(Debug, Clone)]
pub(crate) struct CellOptions {
    pub(crate) source: String,
    pub(crate) include: bool,
    pub(crate) show_input: bool,
    pub(crate) show_output: bool,
    pub(crate) output_asis: bool,
    pub(crate) warning: bool,
    pub(crate) error: bool,
    pub(crate) label: Option<String>,
    pub(crate) lst_label: Option<String>,
    pub(crate) lst_cap: Option<String>,
    pub(crate) fig_cap: Option<TextList>,
    pub(crate) tbl_cap: Option<TextList>,
    pub(crate) fig_alt: Option<TextList>,
    pub(crate) fig_subcap: Option<Vec<String>>,
    pub(crate) tbl_subcap: Option<Vec<String>>,
    pub(crate) fig_align: Option<String>,
    pub(crate) fig_pos: Option<String>,
    pub(crate) tbl_colwidths: Option<Vec<f64>>,
    pub(crate) classes: Vec<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) numbered: bool,
    pub(crate) highlight: Option<String>,
    pub(crate) html_table_processing: bool,
}

impl Default for CellOptions {
    fn default() -> Self {
        Self {
            source: String::new(),
            include: true,
            show_input: true,
            show_output: true,
            output_asis: false,
            warning: true,
            error: true,
            label: None,
            lst_label: None,
            lst_cap: None,
            fig_cap: None,
            tbl_cap: None,
            fig_alt: None,
            fig_subcap: None,
            tbl_subcap: None,
            fig_align: None,
            fig_pos: None,
            tbl_colwidths: None,
            classes: Vec::new(),
            tags: Vec::new(),
            numbered: false,
            highlight: None,
            html_table_processing: true,
        }
    }
}

#[derive(Default)]
struct Visibility {
    include: Option<bool>,
    input: Option<bool>,
    output: Option<bool>,
}

/// Resolves each valid option independently, then applies tag visibility.
pub(crate) fn resolve(
    cell: &Cell, index: usize, language: &str, config: &NotebookConfig,
    path: &Path, diagnostics: &mut Diagnostics,
) -> CellOptions {
    let span = cell.span(index);
    let mut options = CellOptions {
        source: cell.source.as_str().to_owned(),
        ..CellOptions::default()
    };
    let mut visibility = Visibility::default();
    let mut unsupported = Vec::new();
    if let Some(metadata) = cell.writer_options() {
        apply_layer(
            metadata, &mut options, &mut visibility, &mut unsupported, path,
            &span, diagnostics,
        );
    }
    if config.cell_options {
        let lines = split_options(cell.source.as_str(), language);
        options.source = lines.rest;
        if !lines.yaml.trim().is_empty() {
            let option_span =
                SourceSpan { end_line: lines.lines, ..span.clone() };
            match serde_norway::from_str::<Value>(&lines.yaml) {
                Ok(value) if value.is_object() => apply_layer(
                    &value, &mut options, &mut visibility, &mut unsupported,
                    path, &option_span, diagnostics,
                ),
                Ok(_) => warn(
                    "cell-options-invalid", "cell options must be a mapping",
                    path, &option_span, diagnostics,
                ),
                Err(error) => warn(
                    "cell-options-invalid",
                    format!("invalid cell options: {error}"),
                    path,
                    &option_span,
                    diagnostics,
                ),
            }
        }
    }
    let mut tags = cell.tags();
    append_unique(&mut tags, std::mem::take(&mut options.tags));
    options.tags = tags;
    options.include = visibility.include.unwrap_or_else(|| {
        !matches_tag(&options.tags, &config.remove_cell_tags)
    });
    options.show_input = visibility.input.unwrap_or_else(|| {
        !cell.is_hidden_source()
            && !matches_tag(&options.tags, &config.hide_input_tags)
    });
    options.show_output = visibility.output.unwrap_or_else(|| {
        !cell.is_hidden_outputs()
            && !matches_tag(&options.tags, &config.hide_output_tags)
    });
    if !unsupported.is_empty() {
        warn(
            "cell-option-unsupported",
            format!("unsupported cell options: {}", unsupported.join(", ")),
            path,
            &span,
            diagnostics,
        );
    }
    options
}

fn apply_layer(
    value: &Value, options: &mut CellOptions, visibility: &mut Visibility,
    unsupported: &mut Vec<String>, path: &Path, span: &SourceSpan,
    diagnostics: &mut Diagnostics,
) {
    let Some(map) = value.as_object() else { return };
    for (key, value) in map {
        if value.is_null() {
            continue;
        }
        match apply_option(key, value, options, visibility) {
            Some(true) => {}
            Some(false)
                if matches!(key.as_str(), "label" | "lst-label")
                    && value.is_string() =>
            {
                diagnostics.push(
                    Diagnostic::error(
                        "label-invalid",
                        format!("label `{}` is invalid; use letters, digits, `_`, `-`, `:` or `.`", value.as_str().unwrap_or_default()),
                    ).with_path(path).with_span(span.clone()),
                );
            }
            Some(false) => warn(
                "cell-options-invalid",
                format!("invalid value for cell option `{key}`"),
                path,
                span,
                diagnostics,
            ),
            None => append_unique(unsupported, [key.clone()]),
        }
    }
}

/// `None` means unsupported; `Some(false)` leaves the previous value intact.
fn apply_option(
    key: &str, value: &Value, options: &mut CellOptions,
    visibility: &mut Visibility,
) -> Option<bool> {
    let valid = match key {
        "label" => label(value, &mut options.label),
        "lst-label" => label(value, &mut options.lst_label),
        "lst-cap" => string(value, &mut options.lst_cap),
        "fig-align" => string(value, &mut options.fig_align),
        "fig-pos" => string(value, &mut options.fig_pos),
        "fig-cap" => text_list(value, &mut options.fig_cap),
        "tbl-cap" => text_list(value, &mut options.tbl_cap),
        "fig-alt" => text_list(value, &mut options.fig_alt),
        "fig-subcap" => subcaptions(value, &mut options.fig_subcap),
        "tbl-subcap" => subcaptions(value, &mut options.tbl_subcap),
        "include" => boolean(value, &mut visibility.include),
        "echo" => match value.as_str() {
            Some("fenced") => {
                visibility.input = Some(true);
                true
            }
            _ => boolean(value, &mut visibility.input),
        },
        "output" => match value {
            Value::Bool(shown) => {
                visibility.output = Some(*shown);
                options.output_asis = false;
                true
            }
            Value::String(mode) if mode == "asis" => {
                visibility.output = Some(true);
                options.output_asis = true;
                true
            }
            _ => false,
        },
        "warning" => set_bool(value, &mut options.warning),
        "error" => set_bool(value, &mut options.error),
        "classes" => strings(value, &mut options.classes, true),
        "tags" => strings(value, &mut options.tags, false),
        "code-line-numbers" => line_numbers(value, options),
        "html-table-processing" => {
            if value.as_str() != Some("none") {
                return Some(false);
            }
            options.html_table_processing = false;
            true
        }
        "tbl-colwidths" => widths(value, &mut options.tbl_colwidths),
        "eval" | "cache" | "freeze" | "execute" | "kernel" => true,
        _ => return None,
    };
    Some(valid)
}

fn label(value: &Value, destination: &mut Option<String>) -> bool {
    let Some(text) = value.as_str() else {
        return false;
    };
    if !typst_syntax::is_valid_label_literal_id(text) {
        return false;
    }
    *destination = Some(text.to_owned());
    true
}

fn string(value: &Value, destination: &mut Option<String>) -> bool {
    let Some(text) = value.as_str() else {
        return false;
    };
    *destination = Some(text.to_owned());
    true
}

fn text_list(value: &Value, destination: &mut Option<TextList>) -> bool {
    let parsed = match value {
        Value::String(text) => TextList::Scalar(text.clone()),
        Value::Array(values) => {
            let Some(texts) = values
                .iter()
                .map(Value::as_str)
                .map(|text| text.map(str::to_owned))
                .collect()
            else {
                return false;
            };
            TextList::List(texts)
        }
        _ => return false,
    };
    *destination = Some(parsed);
    true
}

fn subcaptions(value: &Value, destination: &mut Option<Vec<String>>) -> bool {
    if value.is_boolean() {
        *destination = Some(Vec::new());
        return true;
    }
    let mut captions = None;
    if !text_list(value, &mut captions) {
        return false;
    }
    *destination = captions.map(|captions| match captions {
        TextList::Scalar(text) => vec![text],
        TextList::List(texts) => texts,
    });
    true
}

fn boolean(value: &Value, destination: &mut Option<bool>) -> bool {
    let Some(value) = value.as_bool() else {
        return false;
    };
    *destination = Some(value);
    true
}

fn set_bool(value: &Value, destination: &mut bool) -> bool {
    let Some(value) = value.as_bool() else {
        return false;
    };
    *destination = value;
    true
}

fn strings(value: &Value, destination: &mut Vec<String>, split: bool) -> bool {
    let parsed = match value {
        Value::String(text) if split => {
            text.split_whitespace().map(str::to_owned).collect()
        }
        Value::String(text) => vec![text.clone()],
        Value::Array(values) => {
            let Some(texts) = values
                .iter()
                .map(Value::as_str)
                .map(|text| text.map(str::to_owned))
                .collect()
            else {
                return false;
            };
            texts
        }
        _ => return false,
    };
    *destination = Vec::new();
    append_unique(destination, parsed);
    true
}

fn widths(value: &Value, destination: &mut Option<Vec<f64>>) -> bool {
    if value.is_boolean() || value.as_str() == Some("auto") {
        *destination = None;
        return true;
    }
    let Some(values) = value.as_array() else {
        return false;
    };
    let Some(widths): Option<Vec<f64>> =
        values.iter().map(Value::as_f64).collect()
    else {
        return false;
    };
    let sum: f64 = widths.iter().sum();
    if !sum.is_finite() || sum <= 0.0 || widths.iter().any(|width| *width < 0.0)
    {
        return false;
    }
    *destination = Some(widths);
    true
}

fn line_numbers(value: &Value, options: &mut CellOptions) -> bool {
    match value {
        Value::Bool(numbered) => {
            options.numbered = *numbered;
            options.highlight = None;
        }
        Value::String(lines) => {
            options.numbered = true;
            options.highlight = Some(lines.replace('|', ","));
        }
        _ => return false,
    }
    true
}

fn append_unique(
    destination: &mut Vec<String>, values: impl IntoIterator<Item = String>,
) {
    for value in values {
        if !destination.contains(&value) {
            destination.push(value);
        }
    }
}

fn matches_tag(tags: &[String], configured: &[String]) -> bool {
    tags.iter().any(|tag| {
        configured
            .iter()
            .any(|name| tag.replace('_', "-") == name.replace('_', "-"))
    })
}

fn warn(
    code: &'static str, message: impl Into<String>, path: &Path,
    span: &SourceSpan, diagnostics: &mut Diagnostics,
) {
    diagnostics.push(
        Diagnostic::warning(code, message)
            .with_path(path)
            .with_span(span.clone()),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parsed(
        source: &str, metadata: Value, config: &NotebookConfig,
    ) -> (CellOptions, Diagnostics) {
        let cell = serde_json::from_value(json!({
            "cell_type": "code", "source": source, "metadata": metadata,
        }))
        .unwrap();
        let mut diagnostics = Diagnostics::new();
        let options = resolve(
            &cell,
            3,
            "python",
            config,
            Path::new("nb.ipynb"),
            &mut diagnostics,
        );
        (options, diagnostics)
    }

    #[test]
    fn option_lines_are_contiguous_and_rest_is_verbatim() {
        let source = "# | echo: false\r\n#| fig-cap: Caption\r\nprint(1)\r\n";
        let options = split_options(source, "Python");
        assert_eq!(options.lines, 2);
        assert_eq!(options.yaml, "echo: false\nfig-cap: Caption");
        assert_eq!(options.rest, "print(1)\r\n");
        for prefix in
            ["\n", " #| echo: false\n", "#!/usr/bin/python\n", "%time x\n"]
        {
            let source = format!("{prefix}#| output: false\nprint(1)");
            let options = split_options(&source, "python");
            assert_eq!(options.lines, 0, "{prefix:?}");
            assert_eq!(options.rest, source);
        }
        assert_eq!(split_options("#|echo: true", "python").yaml, "echo: true");
        assert_eq!(split_options("#|  x: 1\n", "python").yaml, " x: 1");
    }

    #[test]
    fn language_markers_and_block_comments() {
        for (language, source) in [
            ("Rust", "//| echo: false\nx"),
            ("javascript", "// | echo: false\nx"),
            ("typescript", "//| echo: false\nx"),
            ("sh", "#| echo: false\nx"),
            ("sql", "--| echo: false\nx"),
            ("matlab", "%| echo: false\nx"),
            ("fortran95", "!| echo: false\nx"),
            ("stata", "*| echo: false\nx"),
            ("q", "/| echo: false\nx"),
            ("apl", "⍝| echo: false\nx"),
            ("c", "/*| echo: false */\nx"),
            ("css", "/* | echo: false */   \nx"),
            ("sas", "*| echo: false;\nx"),
            ("ocaml", "(*| echo: false *)\nx"),
            ("unknown", "#| echo: false\nx"),
        ] {
            let lines = split_options(source, language);
            assert_eq!(lines.lines, 1, "{language}");
            assert_eq!(lines.yaml, "echo: false", "{language}");
            assert_eq!(lines.rest, "x");
        }
        assert_eq!(split_options("/*| echo: false\nx", "c").lines, 0);
        assert_eq!(split_options("/*| echo: false */ code\nx", "c").lines, 0);
    }

    #[test]
    fn valid_lines_override_metadata_one_key_at_a_time() {
        let source = "#| echo: true\n#| warning: no\n#| fig-cap: [First, Second]\n#| label: 'bad label'\n#| code-line-numbers: '1-3|5'\nx\n";
        let (options, diagnostics) = parsed(
            source,
            json!({"writer": {
                "echo": false, "warning": false, "fig-cap": "Shared",
                "label": "fig-valid", "fig-pos": "ht",
            }}),
            &NotebookConfig::default(),
        );
        assert!(options.show_input);
        assert!(!options.warning);
        assert_eq!(options.label.as_deref(), Some("fig-valid"));
        assert_eq!(
            options.fig_cap,
            Some(TextList::List(vec!["First".into(), "Second".into()]))
        );
        assert_eq!(options.fig_pos.as_deref(), Some("ht"));
        assert_eq!(options.source, "x\n");
        assert!(options.numbered);
        assert_eq!(options.highlight.as_deref(), Some("1-3,5"));
        let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(codes, ["cell-options-invalid", "label-invalid"]);
        for diagnostic in &diagnostics {
            assert_eq!(diagnostic.path.as_deref(), Some(Path::new("nb.ipynb")));
            assert_eq!(
                diagnostic.span,
                Some(SourceSpan {
                    start_line: 1,
                    end_line: 5,
                    cell: Some(3),
                    file: None,
                })
            );
        }
    }

    #[test]
    fn invalid_yaml_keeps_metadata_and_consumes_options() {
        for yaml in ["#| [unterminated", "#| [true, false]"] {
            let source = format!("{yaml}\nx");
            let (options, diagnostics) = parsed(
                &source,
                json!({"writer": {"echo": false}}),
                &NotebookConfig::default(),
            );
            assert!(!options.show_input);
            assert_eq!(options.source, "x");
            assert_eq!(
                diagnostics
                    .iter()
                    .filter(|d| d.code == "cell-options-invalid")
                    .count(),
                1
            );
        }
        let (options, diagnostics) =
            parsed("#|  \nx", Value::Null, &NotebookConfig::default());
        assert!(diagnostics.is_empty());
        assert_eq!(options.source, "x");
    }

    #[test]
    fn disabled_comments_stay_in_source_but_metadata_still_applies() {
        let config =
            NotebookConfig { cell_options: false, ..NotebookConfig::default() };
        let source = "#| echo: true\n#| unsupported: true\nx";
        let (options, diagnostics) =
            parsed(source, json!({"writer": {"echo": false}}), &config);
        assert!(!options.show_input);
        assert_eq!(options.source, source);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn explicit_visibility_wins_over_tags_and_jupyter() {
        let metadata = json!({
            "tags": ["hide_input", "remove-output", "remove_cell", "same"],
            "jupyter": {"source_hidden": true, "outputs_hidden": true},
            "writer": {"include": true, "echo": false, "output": true, "tags": ["same", "metadata"]},
        });
        let (options, diagnostics) = parsed(
            "#| echo: fenced\n#| tags: [same, final]\nx",
            metadata,
            &NotebookConfig::default(),
        );
        assert!(diagnostics.is_empty());
        assert!(options.include && options.show_input && options.show_output);
        assert_eq!(
            options.tags,
            ["hide_input", "remove-output", "remove_cell", "same", "final"]
        );
        let (options, _) = parsed(
            "x",
            json!({"tags": ["hide_input", "remove_output", "remove_cell"]}),
            &NotebookConfig::default(),
        );
        assert!(
            !options.include && !options.show_input && !options.show_output
        );
    }

    #[test]
    fn unsupported_options_warn_once_in_source_order() {
        let (options, diagnostics) = parsed(
            "#| second: 2\n#| first: 3\n#| third: 4\n#| eval: true\nx",
            json!({"writer": {"first": 1, "cache": true}}),
            &NotebookConfig::default(),
        );
        assert_eq!(options.source, "x");
        let warnings: Vec<_> = diagnostics.iter().collect();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].code, "cell-option-unsupported");
        assert_eq!(
            warnings[0].message,
            "unsupported cell options: first, second, third"
        );
    }

    #[test]
    fn caption_forms_and_remaining_options_are_preserved() {
        let (options, diagnostics) = parsed(
            "x",
            json!({"writer": {
                "fig-cap": "Shared", "tbl-cap": ["A", "B"],
                "fig-alt": ["Alt one", "Alt two"], "fig-subcap": "Panel",
                "tbl-subcap": false, "tbl-colwidths": [1, 2, 0],
                "lst-label": "lst-source", "lst-cap": "Listing", "fig-align": "center",
                "classes": "wide dark wide", "tags": "custom-tag",
                "include": true, "echo": "fenced", "output": "asis",
                "warning": false, "error": false, "html-table-processing": "none",
            }}),
            &NotebookConfig::default(),
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert!(
            options.output_asis && options.show_output && options.show_input
        );
        assert!(
            !options.warning
                && !options.error
                && !options.html_table_processing
        );
        assert_eq!(options.fig_cap.as_ref().unwrap().at(9), Some("Shared"));
        assert!(options.fig_cap.as_ref().unwrap().is_scalar());
        assert_eq!(options.tbl_cap.as_ref().unwrap().at(2), None);
        assert_eq!(options.fig_subcap, Some(vec!["Panel".into()]));
        assert_eq!(options.tbl_subcap, Some(vec![]));
        assert_eq!(options.tbl_colwidths, Some(vec![1.0, 2.0, 0.0]));
        assert_eq!(options.lst_label.as_deref(), Some("lst-source"));
        assert_eq!(options.classes, ["wide", "dark"]);
        assert_eq!(options.tags, ["custom-tag"]);
    }

    #[test]
    fn invalid_values_do_not_overwrite_valid_defaults() {
        let (options, diagnostics) = parsed(
            "x",
            json!({"writer": {
                "echo": "hidden", "output": "fenced", "include": "false",
                "fig-cap": ["Caption", 3], "fig-subcap": 1, "tags": ["good", 2],
                "classes": 2, "tbl-colwidths": [0, 0], "warning": "false",
                "html-table-processing": "all", "label": 3, "error": null,
            }}),
            &NotebookConfig::default(),
        );
        assert!(
            options.include
                && options.show_input
                && options.show_output
                && options.error
        );
        assert_eq!(options.fig_cap, None);
        assert!(options.classes.is_empty() && options.tags.is_empty());
        assert_eq!(
            diagnostics
                .iter()
                .filter(|d| d.code == "cell-options-invalid")
                .count(),
            11
        );
    }
}
