//! Independent read-back and optional platform import/thumbnail checks.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use writer::docx::custom_style::reference_docx;
use writer::docx::{ValidateOptions, validate};

#[path = "common/docx.rs"]
mod common_docx;
#[path = "common/math_xml.rs"]
mod math_xml;

// Reviewed against python-docx 1.2.0 and independent package XML counts.
const EXPECTED_PYTHON_COUNTS: (usize, usize, usize) = (81, 2, 2);

enum ToolResult {
    Missing,
    Finished { output: Output, timed_out: bool },
}

fn bounded(command: &mut Command) -> ToolResult {
    let mut child =
        match command.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
            Ok(child) => child,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return ToolResult::Missing;
            }
            Err(error) => panic!("cannot start {command:?}: {error}"),
        };
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let stdout = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let stderr = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break (status, false);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            break (child.wait().unwrap(), true);
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    ToolResult::Finished {
        output: Output {
            status,
            stdout: stdout.join().unwrap(),
            stderr: stderr.join().unwrap(),
        },
        timed_out,
    }
}

fn artifact(case: &str) -> (PathBuf, PathBuf) {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/task-prep/crash-recovery/t43/smoke")
        .join(format!("{case}-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join("showcase.docx");
    std::fs::write(&file, reference_docx().unwrap()).unwrap();
    eprintln!("DOCX smoke artifact: {}", file.display());
    (directory, file)
}

fn record(directory: &Path, label: &str, result: &ToolResult) {
    match result {
        ToolResult::Missing => {
            std::fs::write(
                directory.join(format!("{label}.status")),
                "executable unavailable\n",
            )
            .unwrap();
        }
        ToolResult::Finished { output, timed_out } => {
            std::fs::write(
                directory.join(format!("{label}.stdout")),
                &output.stdout,
            )
            .unwrap();
            std::fs::write(
                directory.join(format!("{label}.stderr")),
                &output.stderr,
            )
            .unwrap();
            std::fs::write(
                directory.join(format!("{label}.status")),
                format!("{}\ntimed_out={timed_out}\n", output.status),
            )
            .unwrap();
        }
    }
}

fn success(output: &Output, timed_out: bool, tool: &str) {
    assert!(!timed_out, "{tool} exceeded 30 seconds");
    assert!(
        output.status.success(),
        "{tool}: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn portable_showcase_readback_agrees_with_independently_counted_package_xml() {
    let bytes = reference_docx().unwrap();
    assert_eq!(validate(&bytes, &ValidateOptions { builtin_styles: true }), []);
    let counts = common_docx::read_back(&bytes);
    let parts = math_xml::zip_parts(&bytes);
    let document =
        math_xml::parse(&math_xml::part(&parts, "word/document.xml"));
    let body = document.child("w:body").unwrap();
    let paragraphs =
        body.children.iter().filter(|node| node.name == "w:p").count();
    let styles = math_xml::parse(&math_xml::part(&parts, "word/styles.xml"))
        .children
        .iter()
        .filter(|node| node.name == "w:style")
        .count();
    let numbers =
        math_xml::parse(&math_xml::part(&parts, "word/numbering.xml"))
            .children
            .iter()
            .filter(|node| node.name == "w:abstractNum")
            .count();
    let images =
        parts.keys().filter(|path| path.starts_with("word/media/")).count();
    assert_eq!(counts.paragraphs, paragraphs);
    assert_eq!(counts.styles, styles);
    assert_eq!(counts.abstract_nums, numbers);
    assert_eq!(counts.images, images);
    assert_eq!(
        images, 1,
        "one built-in grey PNG is shared by the showcase drawings"
    );
    assert!(paragraphs > 0 && styles > 0);
}

#[test]
fn optional_textutil_import_retains_every_showcase_heading() {
    let (directory, file) = artifact("textutil");
    let result = bounded(
        Command::new("textutil")
            .args(["-convert", "txt", "-stdout"])
            .arg(&file),
    );
    record(&directory, "textutil", &result);
    let ToolResult::Finished { output, timed_out } = result else {
        eprintln!("skipping textutil DOCX smoke: executable unavailable");
        return;
    };
    success(&output, timed_out, "textutil");
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Writer DOCX style showcase"));
    for level in 1..=6 {
        for kind in ["numbered", "unlisted"] {
            let heading = format!("Heading {level} ({kind})");
            assert!(text.contains(&heading), "textutil lost {heading:?}");
        }
    }
}

#[test]
fn optional_python_docx_import_matches_the_reviewed_counts() {
    let (directory, file) = artifact("python-docx");
    let python = std::env::var_os("WRITER_DOCX_PYTHON")
        .unwrap_or_else(|| "python3".into());
    let probe = bounded(Command::new(&python).args([
        "-I",
        "-c",
        "import docx; print(docx.__version__)",
    ]));
    record(&directory, "python-probe", &probe);
    match probe {
        ToolResult::Missing => {
            eprintln!(
                "skipping python-docx smoke: interpreter unavailable ({python:?})"
            );
            return;
        }
        ToolResult::Finished { output, timed_out } => {
            if !output.status.success() && !timed_out {
                eprintln!(
                    "skipping python-docx smoke: isolated module capability unavailable: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                );
                return;
            }
            success(&output, timed_out, "python-docx module probe");
        }
    }
    let expected = EXPECTED_PYTHON_COUNTS;
    let result = bounded(Command::new(&python).args(["-I", "-c",
        "import docx,sys; d=docx.Document(sys.argv[1]); print(len(d.paragraphs), len(d.tables), len(d.inline_shapes))"
    ]).arg(&file));
    record(&directory, "python-docx", &result);
    let ToolResult::Finished { output, timed_out } = result else {
        panic!(
            "the Python interpreter disappeared after its successful module probe"
        );
    };
    success(&output, timed_out, "python-docx");
    let actual = String::from_utf8(output.stdout).unwrap();
    let expected = format!("{} {} {}", expected.0, expected.1, expected.2);
    assert_eq!(actual.trim(), expected);
}

fn unavailable_quicklook(stderr: &[u8]) -> bool {
    let message = String::from_utf8_lossy(stderr).to_lowercase();
    [
        "sandbox initialization failed",
        "failed to initialize sandbox",
        "operation not permitted",
        "permission denied",
        "windowserver",
        "connection to service named com.apple.quicklook",
        "cannot connect to the quick look",
        "can't communicate with the application helper",
    ]
    .iter()
    .any(|evidence| message.contains(evidence))
}

#[test]
fn optional_quicklook_produces_a_retained_thumbnail_when_capability_is_available()
 {
    let (directory, file) = artifact("quicklook");
    let result = bounded(
        Command::new("qlmanage")
            .args(["-t", "-s", "1000", "-o"])
            .arg(&directory)
            .arg(&file),
    );
    record(&directory, "quicklook", &result);
    let ToolResult::Finished { output, timed_out } = result else {
        eprintln!("skipping QuickLook DOCX smoke: executable unavailable");
        return;
    };
    if (timed_out || !output.status.success())
        && unavailable_quicklook(&output.stderr)
    {
        eprintln!(
            "skipping QuickLook DOCX smoke: unavailable desktop/broker/permission capability: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        return;
    }
    success(&output, timed_out, "QuickLook");
    let thumbnails = std::fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension().is_some_and(|extension| extension == "png")
        })
        .collect::<Vec<_>>();
    assert!(
        !thumbnails.is_empty(),
        "QuickLook reported success without a PNG thumbnail"
    );
    for thumbnail in thumbnails {
        let bytes = std::fs::read(&thumbnail).unwrap();
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        let size = imagesize::blob_size(&bytes).unwrap();
        assert!(size.width > 0 && size.height > 0);
        eprintln!("QuickLook thumbnail retained: {}", thumbnail.display());
    }
}
