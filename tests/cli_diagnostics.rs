//! CLI excerpt quality does not re-report a primary compiler failure.

use std::fs;

use assert_cmd::Command;

#[test]
fn native_compiler_headline_and_its_source_excerpt_are_each_printed_once() {
    for quiet in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("writer.toml"),
            concat!(
                "[build]\nout_dir='out'\n[pdf]\nsystem_fonts=false\n",
                "allow_packages=false\ntimestamp='2026-10-29T00:00:00Z'\n",
            ),
        )
        .unwrap();
        let marker = "native-excerpt-once-marker";
        let offending = format!("#panic(\"{marker}\")");
        fs::write(
            directory.path().join("native.typ"),
            format!("Native source.\n{offending}\n"),
        )
        .unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_writer"));
        command
            .current_dir(directory.path())
            .env_remove("RUST_LOG")
            .env_remove("CLICOLOR_FORCE")
            .env_remove("SOURCE_DATE_EPOCH")
            .env("NO_COLOR", "1");
        if quiet {
            command.arg("-q");
        }
        let output = command
            .args(["build", "native.typ", "--to", "pdf"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        let stderr = String::from_utf8(output.stderr).unwrap();
        let headlines: Vec<_> = stderr
            .lines()
            .filter(|line| line.contains(": error:") && line.contains(marker))
            .collect();
        assert_eq!(headlines.len(), 1, "{stderr}");
        assert!(headlines[0].starts_with("native.typ:2:"), "{stderr}");
        assert_eq!(
            stderr.matches(&format!("2 | {offending}")).count(),
            1,
            "{stderr}"
        );
        // The two textual occurrences are the one message and one source
        // quotation, not two primary diagnostic reports.
        assert_eq!(stderr.matches(marker).count(), 2, "{stderr}");
        assert!(!stderr.contains("/__writer-main.typ"), "{stderr}");
        assert!(!directory.path().join("out/native.pdf").exists());
    }
}
