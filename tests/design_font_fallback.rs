//! Web-only and undecodable design fonts do not prevent unrelated outputs.

use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;

use assert_cmd::Command;

fn writer(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_writer"));
    command
        .current_dir(root)
        .env_remove("RUST_LOG")
        .env_remove("SOURCE_DATE_EPOCH");
    command
}

#[test]
fn web_and_unreadable_faces_keep_html_and_docx_builds_available() {
    let bundled = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets/vendor/katex/fonts/KaTeX_Main-Regular.woff2");
    for (name, bytes, format) in [
        ("web.woff2", fs::read(bundled).unwrap(), "woff2"),
        ("unreadable.ttf", b"unsupported font bytes".to_vec(), "truetype"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::write(root.join(name), bytes).unwrap();
        fs::write(root.join("document.md"), "Body only.\n").unwrap();
        fs::write(root.join("writer.toml"), format!(
            "[pdf]\nsystem_fonts=false\n[design.fonts]\nbody={{family='Web Only Audit Family',files=['{name}']}}\n",
        )).unwrap();
        writer(root)
            .args(["build", "document.md", "--to", "html", "-o", "output.html"])
            .assert()
            .success();
        let html = fs::read_to_string(root.join("output.html")).unwrap();
        assert!(
            html.contains("@font-face{font-family:\"Web Only Audit Family\"")
        );
        assert!(
            html.contains(&format!(
                " format(\"{format}\");font-display:swap}}"
            ))
        );
        assert!(!html.contains("asset:"));

        writer(root)
            .args(["build", "document.md", "--to", "docx", "-o", "output.docx"])
            .assert()
            .success();
        let bytes = fs::read(root.join("output.docx")).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
        let mut styles = String::new();
        archive
            .by_name("word/styles.xml")
            .unwrap()
            .read_to_string(&mut styles)
            .unwrap();
        assert!(styles.contains("Web Only Audit Family"));
        assert!(writer::docx::validate(&bytes, &Default::default()).is_empty());

        let output = writer(root)
            .args([
                "check", "document.md", "--target", "pdf", "--format", "json",
            ])
            .assert()
            .success()
            .get_output()
            .clone();
        let findings: serde_json::Value =
            serde_json::from_slice(&output.stdout).unwrap();
        assert!(findings.as_array().unwrap().iter().any(|finding| {
            finding["code"] == "design-font-family"
                && finding["message"]
                    .as_str()
                    .unwrap()
                    .contains("Web Only Audit Family")
        }));
    }
}
