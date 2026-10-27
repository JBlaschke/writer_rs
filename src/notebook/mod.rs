//! Jupyter notebook frontend and IR conversion (notebook-mapping.md §2).
//! Owner: T18. Saved outputs are read, never executed.

pub mod ansi;
pub mod convert;
pub mod nbformat;
pub mod options;
pub mod outputs;
pub mod tables;

use crate::config::Config;
use crate::error::Result;
use crate::ir::{Block, DocMeta, Parsed, SourceInfo, SourceSpan};

/// Parses notebook input into document IR, including saved outputs.
pub fn parse(
    source: &SourceInfo, json: &str, config: &Config,
) -> Result<Parsed> {
    let mut includes = crate::markdown::include::cell_includes(source);
    parse_with(
        source,
        json,
        config,
        ParseOptions {
            includes: &mut *includes,
            nested: false,
            selection: Selection::default(),
        },
    )
}

/// Include expansion and cell selection for one parse.
pub struct ParseOptions<'a> {
    /// Expands top-level include directives in Markdown cells.
    pub includes: &'a mut dyn CellIncludes,
    /// Suppresses automatic TOC insertion for an included document.
    pub nested: bool,
    /// Selects cells and optionally hides their input or output.
    pub selection: Selection,
}

/// Supplies include expansion without coupling the frontend to a resolver.
pub trait CellIncludes {
    /// Supplies the notebook's effective metadata for include comparisons.
    /// Custom hooks may ignore this context.
    fn set_parent_meta(&mut self, _: &DocMeta) {}

    /// Returns the included parse, with its paths and spans already attributed.
    fn expand(
        &mut self, path: &str, span: &SourceSpan, notebook: &SourceInfo,
        config: &Config,
    ) -> Result<Parsed>;
}

/// The union of cell references, intersected with matching tags when given.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    /// Cell indices or identifiers; `None` keeps every cell.
    pub cells: Option<Vec<CellRef>>,
    /// At least one of these tags must match; `None` keeps every tag.
    pub tags: Option<Vec<String>>,
    /// Forces selected code inputs hidden, including listing captions.
    pub hide_input: bool,
    /// Forces selected code outputs hidden, including output captions.
    pub hide_output: bool,
}

/// A cell's original file index or nbformat identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellRef {
    /// Original zero-based index, including removed and unknown cells.
    Index(usize),
    /// The nbformat cell identifier.
    Id(String),
}

/// The content category of one retained cell.
#[derive(Debug, Clone, PartialEq)]
pub enum CellKind {
    /// Markdown source.
    Markdown,
    /// Code and saved outputs.
    Code,
    /// Raw content with an explicit target format.
    Raw,
}

/// A retained cell's full, input-only and output-only block forms.
#[derive(Debug, Clone, PartialEq)]
pub struct CellParts {
    /// Original zero-based index.
    pub index: usize,
    /// Original nbformat identifier.
    pub id: Option<String>,
    /// Metadata and option tags, deduplicated in source order.
    pub tags: Vec<String>,
    /// Original cell category.
    pub kind: CellKind,
    /// Full cell content.
    pub body: Vec<Block>,
    /// Code input without its outputs; empty for other cell kinds.
    pub input: Vec<Block>,
    /// Code outputs without their input; empty for other cell kinds.
    pub outputs: Vec<Block>,
}

/// Parses a notebook with the supplied include hook and selection.
pub fn parse_with(
    source: &SourceInfo, json: &str, config: &Config, opts: ParseOptions<'_>,
) -> Result<Parsed> {
    parse_cells(source, json, config, opts).map(|(parsed, _)| parsed)
}

/// Parses a notebook and returns separately addressable cell content.
pub fn parse_cells(
    source: &SourceInfo, json: &str, config: &Config, opts: ParseOptions<'_>,
) -> Result<(Parsed, Vec<CellParts>)> {
    let _listings = crate::markdown::walker::DirListings::enter();
    let (notebook, mut diagnostics) =
        nbformat::parse_notebook(&source.path, json)?;
    let mut assets = crate::assets::registry::AssetRegistry::new();
    let (document, parts) = convert::convert(
        &notebook, source, config, opts, &mut assets, &mut diagnostics,
    )?;
    let mut diagnostics = diagnostics.into_vec();
    for diagnostic in &mut diagnostics {
        diagnostic.path.get_or_insert_with(|| source.path.clone());
    }
    diagnostics
        .sort_by_key(|d| d.span.as_ref().map(|s| (s.cell, s.start_line)));
    let diagnostics = diagnostics.into_iter().collect();
    Ok((Parsed { document, assets, diagnostics }, parts))
}
