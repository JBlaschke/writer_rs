//! Citation and bibliography rendering (references.md §5.2–§5.6).
//! Owner: T70. Stub created by T00.

use super::library::Library;
use crate::ir::{Bibliography, CiteItem, CiteMode, Inlines};

/// One citation subgroup; stub API implemented by T70.
pub struct Request<'a> {
    /// Bibliography items in document order.
    pub items: &'a [CiteItem],
    /// Citation rendering form.
    pub mode: CiteMode,
}

/// Stub result replaced by the citation engine in T70.
pub struct CiteOutput {
    /// Rendered citation subgroups in request order.
    pub citations: Vec<Inlines>,
    /// The document's formatted bibliography, when requested.
    pub bibliography: Option<Bibliography>,
}

/// Stub: citation rendering arrives in T70.
pub fn render(
    _library: &Library, _locale: &str, _requests: &[Request<'_>],
) -> CiteOutput {
    CiteOutput { citations: Vec::new(), bibliography: None }
}
