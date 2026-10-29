//! A failing LaTeX template retains already emitted CLI findings.

use std::fs;

use assert_cmd::Command;

#[test]
fn failed_latex_dump_reports_body_findings_before_its_template_error() {
    for (mode, source, finding) in [
        (
            "listings",
            "```rust\nfn main() {}\n```\n",
            "listings has no language for `rust`",
        ),
        (
            "verbatim",
            "```text\n\\end{Verbatim}\n```\n",
            "code contains `\\end{Verbatim}`",
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("document.md"), source).unwrap();
        fs::write(
            root.path().join("writer.toml"),
            format!("[latex]\ncode='{mode}'\ntemplate='broken.tex'\n",),
        )
        .unwrap();
        fs::write(
            root.path().join("broken.tex"),
            "\\VAR{preamble}\n\\VAR{body}\n\\VAR{vars.missing}\n",
        )
        .unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_writer"))
            .current_dir(root.path())
            .env_remove("RUST_LOG")
            .env_remove("CLICOLOR_FORCE")
            .env_remove("NO_COLOR")
            .args(["dump", "document.md", "--latex"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert_eq!(stderr.matches(finding).count(), 1, "{stderr}");
        let body = stderr.find(finding).unwrap();
        let template = stderr
            .find("undefined value")
            .unwrap_or_else(|| panic!("{stderr}"));
        assert!(body < template, "{stderr}");
        assert!(stderr.contains("broken.tex"), "{stderr}");
        assert_eq!(
            fs::read_to_string(root.path().join("document.md")).unwrap(),
            source
        );
        assert!(!root.path().join("dist").exists());
    }
}
