//! Bibliography library loading and merging (references.md §5.1).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use hayagriva::archive::ArchivedStyle;
use hayagriva::citationberg::{IndependentStyle, Style, StyleClass};

use crate::assets::registry::normalize_lexically;
use crate::error::{Error, Result};

const UNKNOWN_FORMAT: &str =
    "unknown bibliography format (must be .yaml/.yml or .bib)";
const DEPENDENT_STYLE: &str = "dependent CSL styles are not supported";

/// Merged bibliography entries and their configured independent CSL style.
#[derive(Debug)]
pub struct Library {
    pub(super) entries: hayagriva::Library,
    pub(super) style: IndependentStyle,
    style_spec: String,
    files: Vec<PathBuf>,
}

impl Default for Library {
    fn default() -> Self {
        let Style::Independent(style) =
            ArchivedStyle::InstituteOfElectricalAndElectronicsEngineers.get()
        else {
            unreachable!("the bundled IEEE style is independent");
        };
        Self {
            entries: hayagriva::Library::new(),
            style,
            style_spec: "ieee".into(),
            files: Vec::new(),
        }
    }
}

impl Library {
    /// Loads bibliography files in order, rejecting repeated citation keys.
    pub fn load(paths: &[PathBuf]) -> Result<Library> {
        let mut library = Self::default();
        for path in paths {
            let path = absolute(path, "bibliography")?;
            let text = fs::read_to_string(&path)
                .map_err(|error| resource("bibliography", &path, error))?;
            let extension = path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or_default();
            let loaded = if extension.eq_ignore_ascii_case("bib") {
                hayagriva::io::from_biblatex_str(&text).map_err(|errors| {
                    let message = errors
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join("; ");
                    resource("bibliography", &path, message)
                })?
            } else if extension.eq_ignore_ascii_case("yaml")
                || extension.eq_ignore_ascii_case("yml")
            {
                hayagriva::io::from_yaml_str(&text)
                    .map_err(|error| resource("bibliography", &path, error))?
            } else {
                return Err(resource("bibliography", &path, UNKNOWN_FORMAT));
            };
            let mut duplicates = BTreeSet::new();
            for entry in loaded.iter() {
                if library.entries.get(entry.key()).is_some() {
                    duplicates.insert(entry.key().to_owned());
                } else {
                    library.entries.push(entry);
                }
            }
            if !duplicates.is_empty() {
                let message = format!(
                    "duplicate bibliography keys: {}",
                    duplicates.into_iter().collect::<Vec<_>>().join(", "),
                );
                return Err(resource("bibliography", &path, message));
            }
            library.files.push(path);
        }
        Ok(library)
    }

    /// Selects a bundled style or an independent CSL file relative to base_dir.
    pub fn set_style(&mut self, spec: &str, base_dir: &Path) -> Result<()> {
        let source = Path::new(spec);
        let style = if source.extension().is_some() {
            let path = absolute(&base_dir.join(source), "citation style")?;
            let text = fs::read_to_string(&path)
                .map_err(|error| resource("citation style", &path, error))?;
            let parsed = Style::from_xml(&text)
                .map_err(|error| resource("citation style", &path, error))?;
            independent(parsed, &path)?
        } else {
            let archived = ArchivedStyle::by_name(spec).ok_or_else(|| {
                resource(
                    "citation style",
                    source,
                    format!("unknown style: {spec}"),
                )
            })?;
            independent(archived.get(), source)?
        };
        self.style = style;
        self.style_spec = spec.into();
        Ok(())
    }

    /// Every available citation key, ordered deterministically.
    pub fn keys(&self) -> BTreeSet<String> {
        self.entries.keys().map(str::to_owned).collect()
    }

    /// Whether normal-form citations become footnotes.
    pub fn is_note_style(&self) -> bool {
        self.style.settings.class == StyleClass::Note
    }

    /// The configured archive alias or file spelling, unchanged.
    pub fn style_spec(&self) -> &str {
        &self.style_spec
    }

    /// Absolute, lexically normalized bibliography files in occurrence order.
    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }
}

fn independent(style: Style, path: &Path) -> Result<IndependentStyle> {
    match style {
        Style::Independent(style) => Ok(style),
        Style::Dependent(_) => {
            Err(resource("citation style", path, DEPENDENT_STYLE))
        }
    }
}

fn absolute(path: &Path, what: &'static str) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|error| resource(what, path, error))?
            .join(path)
    };
    Ok(normalize_lexically(&path))
}

fn resource(what: &'static str, path: &Path, message: impl ToString) -> Error {
    Error::Resource {
        what,
        path: path.to_owned(),
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOOK: &str =
        "@book{book,author={Knuth, Donald E.},title={The TeXbook},year={1984}}";
    const YAML: &str =
        "k1:\n  type: Book\n  title: Y\n  author: Kwan, Kevin\n  date: 2014\n";
    const CSL: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text">
  <info><title>Writer library test</title><id>https://example.org/writer-library-test</id>
    <updated>2026-10-29T00:00:00+00:00</updated></info>
  <citation><layout prefix="(" suffix=")"><text variable="citation-number"/></layout></citation>
  <bibliography><layout><text variable="title"/></layout></bibliography>
</style>"#;
    const DEPENDENT: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0">
  <info><title>Writer dependent test</title><id>https://example.org/writer-dependent-test</id>
    <link href="https://example.org/writer-parent" rel="independent-parent"/>
    <updated>2026-10-29T00:00:00+00:00</updated></info>
</style>"#;

    fn failure(error: Error, category: &str, expected: &Path) -> String {
        let Error::Resource { what, path, message } = error else {
            panic!("expected Resource, got {error:?}");
        };
        assert_eq!(what, category);
        assert_eq!(path, expected);
        message
    }

    #[test]
    fn default_and_empty_load_are_empty_ieee_libraries() {
        for library in [Library::default(), Library::load(&[]).unwrap()] {
            assert!(library.keys().is_empty());
            assert!(library.files().is_empty());
            assert!(!library.is_note_style());
            assert_eq!(library.style_spec(), "ieee");
            assert!(format!("{library:?}").starts_with("Library"));
        }
    }

    #[test]
    fn merged_files_preserve_order_spelling_and_case_insensitive_formats() {
        let dir = tempfile::tempdir().unwrap();
        let bib = dir.path().join("refs.BIB");
        let yaml = dir.path().join("refs.YAML");
        fs::write(&bib, BOOK).unwrap();
        fs::write(&yaml, YAML).unwrap();
        let written = dir.path().join("unused/../refs.BIB");
        let library = Library::load(&[written, yaml.clone()]).unwrap();
        assert_eq!(library.files(), [bib, yaml]);
        assert!(library.files().iter().all(|path| path.is_absolute()));
        assert_eq!(library.entries.keys().collect::<Vec<_>>(), ["book", "k1"]);
        assert_eq!(
            library.keys(),
            BTreeSet::from(["book".into(), "k1".into()])
        );
        assert_eq!(library.style_spec(), "ieee");

        let yml = dir.path().join("refs.YmL");
        fs::write(&yml, YAML).unwrap();
        assert_eq!(
            Library::load(&[yml]).unwrap().keys(),
            BTreeSet::from(["k1".into()])
        );
    }

    #[test]
    fn relative_paths_are_absolutized_without_accessing_or_canonicalizing() {
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(
            absolute(Path::new("unused/../refs.bib"), "bibliography").unwrap(),
            normalize_lexically(&cwd.join("refs.bib")),
        );
    }

    #[test]
    fn duplicate_keys_are_sorted_unique_and_never_overwrite_entries() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first.bib");
        let second = dir.path().join("second.yaml");
        let third = dir.path().join("third.bib");
        fs::write(
            &first,
            "@book{z,title={First Z}}\n@book{a,title={First A}}\n",
        )
        .unwrap();
        fs::write(&second, "z:\n  type: Book\n  title: Second Z\na:\n  type: Book\n  title: Second A\n").unwrap();
        fs::write(&third, "@book{z,title={Third Z}}\n").unwrap();
        let error = Library::load(&[first, second.clone(), third]).unwrap_err();
        assert_eq!(
            failure(error, "bibliography", &second),
            "duplicate bibliography keys: a, z",
        );
    }

    #[test]
    fn first_conflict_precedes_later_parser_and_read_errors() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first.bib");
        let conflict = dir.path().join("conflict.yaml");
        let malformed = dir.path().join("malformed.bib");
        let unreadable = dir.path().join("does-not-exist.bib");
        fs::write(&first, "@book{z,title={Z}}\n@book{a,title={A}}\n").unwrap();
        fs::write(&conflict, "z:\n  type: Book\na:\n  type: Book\n").unwrap();
        fs::write(&malformed, "@book{").unwrap();
        for later in [malformed, unreadable] {
            let error =
                Library::load(&[first.clone(), conflict.clone(), later])
                    .unwrap_err();
            assert_eq!(
                failure(error, "bibliography", &conflict),
                "duplicate bibliography keys: a, z",
            );
        }
    }

    #[test]
    fn repeated_arguments_are_parsed_and_retained_in_occurrence_order() {
        let dir = tempfile::tempdir().unwrap();
        let empty = dir.path().join("empty.bib");
        fs::write(&empty, "").unwrap();
        let paths = [empty.clone(), empty];
        assert_eq!(Library::load(&paths).unwrap().files(), paths);

        let book = dir.path().join("book.bib");
        fs::write(&book, BOOK).unwrap();
        assert_eq!(
            failure(
                Library::load(&[book.clone(), book.clone()]).unwrap_err(),
                "bibliography",
                &book,
            ),
            "duplicate bibliography keys: book",
        );
    }

    #[test]
    fn parser_errors_and_within_file_duplicates_keep_resource_attribution() {
        let dir = tempfile::tempdir().unwrap();
        for (name, contents, expected) in [
            ("bad.bib", "@book{", "biblatex parse error"),
            (
                "duplicate.bib",
                "@book{k,title={A}}\n@book{k,title={B}}",
                "duplicate key",
            ),
            ("bad.yaml", "k:\n  title: No type\n", "no entry type"),
            (
                "duplicate.yml",
                "k:\n  type: Book\nk:\n  type: Book\n",
                "duplicate key",
            ),
        ] {
            let path = dir.path().join(name);
            fs::write(&path, contents).unwrap();
            let message = failure(
                Library::load(std::slice::from_ref(&path)).unwrap_err(),
                "bibliography",
                &path,
            );
            assert!(message.contains(expected), "{name}: {message}");
        }
    }

    #[test]
    fn all_biblatex_type_errors_are_retained_in_parser_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("invalid-years.bib");
        fs::write(
            &path,
            "@book{first,title={A},year={not-a-year}}\n\
             @book{second,title={B},year={also-not-a-year}}\n",
        )
        .unwrap();
        let message = failure(
            Library::load(std::slice::from_ref(&path)).unwrap_err(),
            "bibliography",
            &path,
        );
        assert_eq!(
            message.matches("biblatex type error:").count(),
            2,
            "{message}"
        );
        assert!(message.contains("; "), "{message}");
    }

    #[test]
    fn unsupported_missing_and_non_utf8_files_fail_as_bibliography_resources() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["refs.json", "refs"] {
            let path = dir.path().join(name);
            fs::write(&path, "{}").unwrap();
            assert_eq!(
                failure(
                    Library::load(std::slice::from_ref(&path)).unwrap_err(),
                    "bibliography",
                    &path
                ),
                UNKNOWN_FORMAT,
            );
        }
        let missing = dir.path().join("missing.bib");
        assert!(
            !failure(
                Library::load(std::slice::from_ref(&missing)).unwrap_err(),
                "bibliography",
                &missing,
            )
            .is_empty()
        );
        let invalid = dir.path().join("invalid.bib");
        fs::write(&invalid, [0xff, 0xfe]).unwrap();
        assert!(
            !failure(
                Library::load(std::slice::from_ref(&invalid)).unwrap_err(),
                "bibliography",
                &invalid,
            )
            .is_empty()
        );
    }

    #[test]
    fn archive_aliases_and_note_classes_preserve_configured_style_spelling() {
        let mut library = Library::default();
        for (name, note) in [
            ("apa", false),
            ("chicago-notes", true),
            ("chicago-fullnotes", true),
            ("ieee", false),
        ] {
            library.set_style(name, Path::new(".")).unwrap();
            assert_eq!(library.style_spec(), name);
            assert_eq!(library.is_note_style(), note);
        }
    }

    #[test]
    fn file_styles_resolve_against_base_and_do_not_enter_bibliography_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("styles")).unwrap();
        let path = dir.path().join("styles/local.xml");
        fs::write(&path, CSL).unwrap();
        let bib = dir.path().join("refs.bib");
        fs::write(&bib, BOOK).unwrap();
        let mut library = Library::load(std::slice::from_ref(&bib)).unwrap();
        library.set_style("styles/local.xml", dir.path()).unwrap();
        assert_eq!(library.style_spec(), "styles/local.xml");
        assert!(!library.is_note_style());
        assert_eq!(library.files(), [bib]);
        library
            .set_style(path.to_str().unwrap(), Path::new("/unused"))
            .unwrap();
        assert_eq!(library.style_spec(), path.to_str().unwrap());
    }

    #[test]
    fn failed_style_changes_preserve_the_last_successful_style_and_library() {
        let dir = tempfile::tempdir().unwrap();
        let mut library = Library::default();
        library.set_style("chicago-notes", dir.path()).unwrap();
        for (spec, contents, expected) in [
            ("dependent.csl", DEPENDENT, DEPENDENT_STYLE),
            ("broken.csl", "<not-a-style>", ""),
        ] {
            let path = dir.path().join(spec);
            fs::write(&path, contents).unwrap();
            let message = failure(
                library.set_style(spec, dir.path()).unwrap_err(),
                "citation style",
                &path,
            );
            assert!(!message.is_empty());
            if !expected.is_empty() {
                assert_eq!(message, expected);
            }
            assert_eq!(library.style_spec(), "chicago-notes");
            assert!(library.is_note_style());
        }
        let missing = dir.path().join("missing.xml");
        failure(
            library.set_style("missing.xml", dir.path()).unwrap_err(),
            "citation style",
            &missing,
        );
        assert_eq!(
            failure(
                library.set_style("not-a-style", dir.path()).unwrap_err(),
                "citation style",
                Path::new("not-a-style"),
            ),
            "unknown style: not-a-style",
        );
        assert_eq!(library.style_spec(), "chicago-notes");
        assert!(library.is_note_style());
    }

    #[cfg(unix)]
    #[test]
    fn bibliography_symlink_spelling_is_not_canonicalized() {
        let dir = tempfile::tempdir().unwrap();
        let actual = dir.path().join("actual.bib");
        let link = dir.path().join("link.bib");
        fs::write(&actual, BOOK).unwrap();
        std::os::unix::fs::symlink(&actual, &link).unwrap();
        let library = Library::load(std::slice::from_ref(&link)).unwrap();
        assert_eq!(library.files(), [link]);
        assert_ne!(library.files()[0], fs::canonicalize(actual).unwrap());
    }
}
