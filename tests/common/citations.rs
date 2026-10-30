//! Real fixture libraries after each caller has merged its effective config.

use writer::cite::Library;
use writer::config::Config;
use writer::ir::Document;

pub fn load(document: &Document, config: &Config) -> Option<Library> {
    if config.references.bibliography.is_empty() {
        return None;
    }
    let base = &document.source.base_dir;
    let paths = config
        .references
        .bibliography
        .iter()
        .map(|path| base.join(path))
        .collect::<Vec<_>>();
    let mut library = Library::load(&paths).unwrap();
    library.set_style(&config.references.style, base).unwrap();
    Some(library)
}
