//! Four scaffold variants resolve and build without replacing user files.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::Command;
use writer::config::Config;
use writer::ir::{self, Document, Inline};

fn writer(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_writer"));
    command
        .current_dir(root)
        .env_remove("RUST_LOG")
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR_FORCE")
        .env("SOURCE_DATE_EPOCH", "0");
    command
}

fn initialized(root: &Path, dir: &Path, typst: bool, bib: bool) -> Output {
    let mut command = writer(root);
    command.arg("init").arg(dir);
    if typst {
        command.arg("--typst");
    }
    if bib {
        command.arg("--bib");
    }
    command.output().unwrap()
}

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn inventory(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                directories.push(path);
            } else {
                result.insert(
                    path.strip_prefix(root).unwrap().into(),
                    fs::read(&path).unwrap(),
                );
            }
        }
    }
    result
}

#[test]
fn all_four_scaffolds_have_real_resolving_documents_and_build_all_targets() {
    for typst in [false, true] {
        for bib in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let project = root.path().join("report");
            let output = initialized(root.path(), &project, typst, bib);
            success(&output);
            assert!(
                String::from_utf8_lossy(&output.stdout)
                    .contains("Initialized ")
            );
            let source = if typst { "document.typ" } else { "document.md" };
            let mut expected = BTreeSet::from([
                PathBuf::from(source),
                "writer.toml".into(),
                "figures/plot.png".into(),
                "theme/theme.toml".into(),
                "theme/styles/custom.css".into(),
                ".gitignore".into(),
                "README.md".into(),
            ]);
            if bib {
                expected.insert("refs.bib".into());
            }
            assert_eq!(
                inventory(&project).into_keys().collect::<BTreeSet<_>>(),
                expected
            );
            let config = Config::load(&project.join("writer.toml")).unwrap();
            assert_eq!(config.html.theme, "theme");
            assert!(!config.pdf.system_fonts && !config.pdf.allow_packages);
            let theme =
                writer::theme::resolve("theme", &[project.as_path()]).unwrap();
            assert_eq!(
                theme
                    .chain
                    .iter()
                    .map(|theme| theme.name.as_str())
                    .collect::<Vec<_>>(),
                ["writer-project", "default"]
            );
            assert_eq!(
                fs::read(project.join("figures/plot.png")).unwrap(),
                writer::vendor::get("scaffold/figures/plot.png")
                    .unwrap()
                    .as_ref()
            );
            assert_eq!(
                fs::read_to_string(project.join(".gitignore")).unwrap(),
                "dist/\n"
            );
            let text = fs::read_to_string(project.join(source)).unwrap();
            assert!(!text.contains("{{"), "{text}");
            if typst {
                assert!(text.contains(&format!(
                    "@writer/doc:{}",
                    writer::typst::WRITER_DOC_VERSION
                )));
                assert_eq!(
                    text.contains("bibliography: path(\"refs.bib\")"),
                    bib
                );
            }

            let checked = writer(&project)
                .args(["check", source, "--format", "json"])
                .output()
                .unwrap();
            success(&checked);
            let findings: Vec<serde_json::Value> =
                serde_json::from_slice(&checked.stdout).unwrap();
            assert!(findings.is_empty(), "{findings:?}");
            let dumped = writer(&project)
                .args(["dump", source, "--ir"])
                .output()
                .unwrap();
            success(&dumped);
            let document: Document =
                serde_json::from_slice(&dumped.stdout).unwrap();
            assert!(document.invariant_violations().is_empty());
            assert_eq!(document.features.citations, bib);
            let mut cites = 0;
            ir::walk_inlines(&document, &mut |inline| {
                if let Inline::Cite { items, resolved, .. } = inline {
                    cites += 1;
                    assert_eq!(items[0].key, "writer2026");
                    assert!(
                        !ir::plain_text(&resolved.as_ref().unwrap().inlines)
                            .is_empty()
                    );
                }
            });
            assert_eq!(cites, usize::from(bib));
            if bib {
                let bibliography =
                    document.resolved.bibliography.as_ref().unwrap();
                assert_eq!(bibliography.entries.len(), 1);
                assert_eq!(bibliography.entries[0].key, "writer2026");
            } else {
                assert!(document.resolved.bibliography.is_none());
            }

            success(
                &writer(&project).args(["build", source]).output().unwrap(),
            );
            if typst {
                assert!(
                    fs::read(project.join("dist/document.pdf"))
                        .unwrap()
                        .starts_with(b"%PDF-")
                );
            } else {
                assert!(
                    fs::read_to_string(project.join("dist/document.html"))
                        .unwrap()
                        .contains("46rem")
                );
            }
            let targets = if typst {
                "html,docx,pdf,latex"
            } else {
                "html,docx,pdf,latex,typst"
            };
            success(
                &writer(&project)
                    .args(["build", source, "--to", targets])
                    .output()
                    .unwrap(),
            );
            for extension in ["html", "docx", "pdf", "tex"] {
                assert!(
                    project
                        .join(format!("dist/document.{extension}"))
                        .is_file()
                );
            }
            assert!(
                fs::read_to_string(project.join("dist/document.tex"))
                    .unwrap()
                    .contains("\\documentclass")
            );
            if !typst {
                let exported = project.join("dist/document.typ");
                assert!(exported.is_file());
                assert!(
                    fs::read_to_string(&exported)
                        .unwrap()
                        .contains("document_assets/")
                );
                // Compile the physical export and its local resource paths.
                // Native input already is Typst and cannot select that target.
                success(
                    &writer(&project)
                        .args([
                            "build", "dist/document.typ", "--to", "pdf", "-o",
                            "exported.pdf",
                        ])
                        .output()
                        .unwrap(),
                );
                assert!(
                    fs::read(project.join("exported.pdf"))
                        .unwrap()
                        .starts_with(b"%PDF-")
                );
            }
        }
    }
}

#[test]
fn default_current_directory_and_quiet_scaffolding_are_supported() {
    let root = tempfile::tempdir().unwrap();
    let output = writer(root.path()).args(["-q", "init"]).output().unwrap();
    success(&output);
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
    assert!(root.path().join("document.md").is_file());
    assert!(!root.path().join("document.typ").exists());
}

#[test]
fn force_keeps_unrelated_files_and_refuses_existing_scaffold_paths() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("keep.txt"), b"unrelated").unwrap();
    let before = inventory(root.path());
    let output = writer(root.path()).arg("init").arg(".").output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("not empty"));
    assert_eq!(inventory(root.path()), before);
    let output =
        writer(root.path()).args(["init", ".", "--force"]).output().unwrap();
    success(&output);
    assert_eq!(fs::read(root.path().join("keep.txt")).unwrap(), b"unrelated");
    let before = inventory(root.path());
    let output =
        writer(root.path()).args(["init", ".", "--force"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("never replaces scaffold files")
    );
    assert_eq!(inventory(root.path()), before);
}

#[test]
fn a_late_selected_collision_is_preflighted_before_creating_other_files() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("writer.toml"), b"user config").unwrap();
    let before = inventory(root.path());
    let output = writer(root.path())
        .args(["init", ".", "--force", "--bib"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(inventory(root.path()), before);
    assert!(!root.path().join("theme").exists());
    assert!(!root.path().join("figures").exists());
}

#[test]
fn ordinary_parent_paths_and_unicode_names_work_but_missing_parent_dotdot_does_not()
 {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("existing")).unwrap();
    let output = initialized(
        &root.path().join("existing"),
        Path::new("../My λ report"),
        false,
        false,
    );
    success(&output);
    assert!(root.path().join("My λ report/document.md").is_file());
    let output =
        initialized(root.path(), Path::new("new/../unexpected"), false, false);
    assert_eq!(output.status.code(), Some(2));
    assert!(!root.path().join("new").exists());
    assert!(!root.path().join("unexpected").exists());
}

#[test]
fn an_existing_file_cannot_be_used_as_a_project_directory() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("owned");
    fs::write(&file, b"user bytes").unwrap();
    let output = initialized(root.path(), &file, false, false);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(fs::read(file).unwrap(), b"user bytes");
}

#[cfg(unix)]
#[test]
fn ordinary_linked_project_ancestors_work_and_selected_file_links_are_preserved()
 {
    let root = tempfile::tempdir().unwrap();
    let actual = root.path().join("actual");
    fs::create_dir(&actual).unwrap();
    std::os::unix::fs::symlink(&actual, root.path().join("alias")).unwrap();
    success(&initialized(root.path(), Path::new("alias/report"), false, false));
    assert!(actual.join("report/document.md").is_file());
    let project = root.path().join("collision");
    fs::create_dir(&project).unwrap();
    let owned = root.path().join("owned.md");
    fs::write(&owned, b"owned source").unwrap();
    std::os::unix::fs::symlink(&owned, project.join("document.md")).unwrap();
    let output = writer(root.path())
        .arg("init")
        .arg(&project)
        .arg("--force")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(fs::read(&owned).unwrap(), b"owned source");
    assert_eq!(fs::read_link(project.join("document.md")).unwrap(), owned);
    assert!(!project.join("writer.toml").exists());
}
