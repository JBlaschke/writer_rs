//! CLI sidecar statistics describe this successful build's actual writes.

use std::fs;

use assert_cmd::Command;
use writer::cli::build::format_size;

const PNG: &[u8] = include_bytes!("fixtures/latex/images/plot.png");

#[test]
fn repeated_image_counts_one_written_file_and_ignores_stale_sidecars() {
    let project = tempfile::tempdir().unwrap();
    let root = project.path();
    fs::write(root.join("plot.png"), PNG).unwrap();
    fs::write(
        root.join("document.md"),
        "First ![one](plot.png), then ![two](plot.png).\n",
    )
    .unwrap();
    fs::write(
        root.join("writer.toml"),
        "[build]\nout_dir='out'\n[pdf]\nsystem_fonts=false\nallow_packages=false\n",
    ).unwrap();

    let sidecar = root.join("out/document_assets");
    fs::create_dir_all(sidecar.join("nested")).unwrap();
    let stale = b"unrelated old sidecar contents";
    let nested = b"unrelated nested sidecar contents";
    fs::write(sidecar.join("stale.bin"), stale).unwrap();
    fs::write(sidecar.join("nested/stale.bin"), nested).unwrap();

    let expected = format!(
        "Wrote out/document_assets (1 files, {})",
        format_size(PNG.len()),
    );
    let mut previous = None;
    for _ in 0..2 {
        let output = Command::new(env!("CARGO_BIN_EXE_writer"))
            .current_dir(root)
            .env_remove("RUST_LOG")
            .env_remove("NO_COLOR")
            .env_remove("CLICOLOR_FORCE")
            .env_remove("SOURCE_DATE_EPOCH")
            .args(["build", "document.md", "--to", "latex"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr),
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let lines: Vec<_> = stdout
            .lines()
            .filter(|line| line.contains("document_assets ("))
            .map(|line| line.replace('\\', "/"))
            .collect();
        assert_eq!(lines, std::slice::from_ref(&expected), "{stdout}");
        if let Some(previous) = previous.as_ref() {
            assert_eq!(&lines[0], previous);
        }
        previous = Some(lines[0].clone());

        let images: Vec<_> = fs::read_dir(&sidecar)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "png"))
            .collect();
        assert_eq!(images.len(), 1, "{images:?}");
        assert_eq!(fs::read(&images[0]).unwrap(), PNG);
        assert_eq!(fs::read(sidecar.join("stale.bin")).unwrap(), stale);
        assert_eq!(fs::read(sidecar.join("nested/stale.bin")).unwrap(), nested,);
        assert!(root.join("out/document.tex").is_file());
    }
}
