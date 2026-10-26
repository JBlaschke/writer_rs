//! Best-effort dependency discovery for Markdown files that fail to parse.

use writer::markdown::include::{MAX_DEPTH, scan_includes};

fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (name, text) in files {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

#[test]
fn scanned_in_a_markdown_document_of_any_extension() {
    let dir = project(&[
        ("document.txt", "# A\n\n:::include part.md\n:::\n"),
        ("part.md", "# P\n\n:::include more.txt\n:::\n"),
        ("more.txt", ":::include not-scanned.md\n:::\n"),
    ]);
    let found = scan_includes(&dir.path().join("document.txt"));
    assert_eq!(
        found,
        [dir.path().join("part.md"), dir.path().join("more.txt")]
    );
}

#[test]
fn transitive_and_stops_at_cycles() {
    let dir = project(&[
        (
            "document.md",
            ":::include parts/a.md\n:::\n\n:::include parts/b.md\n:::\n",
        ),
        (
            "parts/a.md",
            ":::include ./b.md\n:::\n\n:::include ../document.md\n:::\n",
        ),
        ("parts/b.md", ":::include a.md\n:::\n"),
    ]);
    assert_eq!(
        scan_includes(&dir.path().join("document.md")),
        [dir.path().join("parts/a.md"), dir.path().join("parts/b.md")]
    );
}

#[test]
fn missing_files_are_listed() {
    let dir = project(&[(
        "document.md",
        ":::include \"missing directory/part.md\"\n:::\n\n:::include absent.ipynb\n:::\n",
    )]);
    assert_eq!(
        scan_includes(&dir.path().join("document.md")),
        [
            dir.path().join("missing directory/part.md"),
            dir.path().join("absent.ipynb"),
        ]
    );
    assert!(scan_includes(&dir.path().join("absent.md")).is_empty());
}

#[test]
fn malformed_paths_are_ignored_but_nested_includes_are_scanned() {
    let dir = project(&[(
        "document.md",
        ":::include\n:::\n\n:::include 'two files.md' extra\n:::\n\n:::include https://example.org/remote.md\n:::\n\n::: {.include a.md}\n:::\n\n> :::include \"nested file.md\"\n> :::\n\n```md\n:::include code.md\n:::\n```\n",
    )]);
    assert_eq!(
        scan_includes(&dir.path().join("document.md")),
        [dir.path().join("nested file.md")]
    );
}

#[test]
fn depth_limit_still_lists_the_next_dependency() {
    let dir = tempfile::tempdir().unwrap();
    for depth in 0..=MAX_DEPTH + 1 {
        std::fs::write(
            dir.path().join(format!("{depth}.md")),
            format!(":::include {}.md\n:::\n", depth + 1),
        )
        .unwrap();
    }
    let expected: Vec<_> = (1..=MAX_DEPTH)
        .map(|depth| dir.path().join(format!("{depth}.md")))
        .collect();
    assert_eq!(scan_includes(&dir.path().join("0.md")), expected);
    assert!(expected.iter().all(|path| path.is_absolute()));
}
