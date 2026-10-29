//! Optional real latexmk protocol; production code never invokes TeX engines.
//!
//! The original tour uses a real library and must exit successfully. Engine
//! evidence includes the actual BibTeX/Biber result, not source commands alone.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use regex::Regex;
use writer::config::LatexCite;

#[path = "common/latex_citations.rs"]
mod latex_citations;

const CONFIG: &str = "[pdf]\nsystem_fonts=false\nallow_packages=false\ntimestamp='2026-10-29T00:00:00Z'\n";

struct Case {
    name: &'static str,
    source: &'static str,
    settings: &'static str,
    template: Option<&'static str>,
    xelatex_only: bool,
    tour: bool,
}

const CASES: &[Case] = &[
    Case {
        name: "sample-biblatex",
        source: "latex/tour.md",
        settings: "cite='biblatex'\n",
        template: None,
        xelatex_only: false,
        tour: true,
    },
    Case {
        name: "sample-natbib",
        source: "latex/tour.md",
        settings: "cite='natbib'\ncode='listings'\ncleveref=true\n",
        template: None,
        xelatex_only: true,
        tour: true,
    },
    Case {
        name: "sample-highlighted",
        source: "latex/tour.md",
        settings: "code='highlighted'\ntables='tabular'\nbooktabs=false\nunicode='keep'\n",
        template: None,
        xelatex_only: true,
        tour: true,
    },
    Case {
        name: "minimal",
        source: "latex/tour.md",
        settings: "cite='plain'\nemit_bibliography=true\n",
        template: Some("template.tex"),
        xelatex_only: false,
        tour: true,
    },
    Case {
        name: "byo",
        source: "latex/tour.md",
        settings: "cite='plain'\nemit_bibliography=true\n",
        template: Some("byo-template.tex"),
        xelatex_only: false,
        tour: true,
    },
    Case {
        name: "empty",
        source: "empty.md",
        settings: "",
        template: Some("template.tex"),
        xelatex_only: false,
        tour: false,
    },
    Case {
        name: "notebook",
        source: "ipynb/outputs.ipynb",
        settings: "cell_prompts=true\n",
        template: None,
        xelatex_only: false,
        tour: false,
    },
];

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn available(tool: &str) -> bool {
    match Command::new(tool).arg("--version").output() {
        Ok(output) => {
            assert!(
                output.status.success(),
                "{tool} --version: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => panic!("cannot inspect {tool}: {error}"),
    }
}

fn copy(source: &Path, target: &Path) {
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::copy(source, target).unwrap();
}

fn inputs(root: &Path) {
    for name in [
        "latex/tour.md", "latex/images/plot.png", "latex/images/diagram.svg",
        "latex/images/anim.gif", "latex/template.tex",
        "latex/byo-template.tex", "bib/refs.bib", "ipynb/outputs.ipynb",
        "ipynb/images/pixel.png", "md/images/plot.png",
    ] {
        copy(&fixtures().join(name), &root.join(name));
    }
    // This positive engine probe supplies the deliberate missing-image case
    // in its scratch project; the notebook JSON stays byte-for-byte original.
    copy(
        &fixtures().join("ipynb/images/pixel.png"),
        &root.join("ipynb/images/missing.png"),
    );
    fs::write(root.join("empty.md"), "One plain paragraph.\n").unwrap();
}

fn record(directory: &Path, label: &str, output: &Output) {
    fs::write(directory.join(format!("{label}.stdout")), &output.stdout)
        .unwrap();
    fs::write(directory.join(format!("{label}.stderr")), &output.stderr)
        .unwrap();
    fs::write(
        directory.join(format!("{label}.status")),
        format!("{}\n", output.status),
    )
    .unwrap();
}

fn run_writer(
    root: &Path, config: &Path, input: &Path, output: &Path,
    template: Option<&Path>,
) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_writer"));
    // -v exposes the actual latex-unicode info needed by the XeLaTeX oracle.
    command
        .current_dir(root)
        .env_remove("RUST_LOG")
        .env("NO_COLOR", "1")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("SOURCE_DATE_EPOCH")
        .arg("-v")
        .arg("--config")
        .arg(config)
        .arg("build")
        .arg(input)
        .args(["--to", "latex", "-o"])
        .arg(output);
    if let Some(template) = template {
        command.arg("--latex-template").arg(template);
    }
    command.output().unwrap()
}

fn tour_citations(case: &Case, source: &str, output: &Output, tex: &str) {
    let known = Regex::new(r"@(doe2020|lee2019|knuth1984)\b").unwrap();
    assert_eq!(known.find_iter(source).count(), 8, "keep the original tour");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains(": error:"), "{stderr}");
    let family = match case.name {
        "minimal" | "byo" => LatexCite::Plain,
        "sample-natbib" => LatexCite::Natbib,
        _ => LatexCite::Biblatex,
    };
    latex_citations::assert_tour_commands(tex, family);
    if family == LatexCite::Plain {
        assert!(tex.contains("\\bibliographystyle{plain}"));
        assert!(tex.contains("\\bibliography{"));
        assert!(!tex.contains("\\usepackage{natbib}"));
        assert!(!tex.contains("{biblatex}"));
    }
}

fn bibliography_evidence(
    root: &Path, outdir: &str, label: &str, evidence: &Path, biber: bool,
) {
    let directory = root.join(outdir);
    let bbl =
        fs::read_to_string(directory.join(format!("{label}.bbl"))).unwrap();
    let blg =
        fs::read_to_string(directory.join(format!("{label}.blg"))).unwrap();
    fs::write(evidence.join("document.bbl"), &bbl).unwrap();
    fs::write(evidence.join("document.blg"), &blg).unwrap();
    for key in ["doe2020", "lee2019", "knuth1984"] {
        assert!(
            bbl.contains(&format!("{{{key}}}")),
            "{label}: missing {key}: {bbl}",
        );
    }
    assert!(
        blg.contains(if biber {
            "This is Biber"
        } else {
            "This is BibTeX"
        }),
        "{label}: expected actual bibliography backend: {blg}",
    );
}

fn missing_characters(log: &str, build_stderr: &str) {
    let unicode = Regex::new(r"U\+([0-9A-Fa-f]{4,6})").unwrap();
    let literal = Regex::new(r"Missing character: There is no (.)").unwrap();
    for line in log.lines().filter(|line| line.contains("Missing character")) {
        let points = unicode
            .captures_iter(line)
            .map(|capture| u32::from_str_radix(&capture[1], 16).unwrap())
            .collect::<Vec<_>>();
        let points = if points.is_empty() {
            vec![
                literal.captures(line).unwrap_or_else(|| {
                    panic!("unrecognized missing-character report: {line}")
                })[1]
                    .chars()
                    .next()
                    .unwrap() as u32,
            ]
        } else {
            points
        };
        for point in points {
            assert!(
                build_stderr.contains(&format!("U+{point:04X}")),
                "XeLaTeX missing U+{point:04X} without writer latex-unicode info: {line}"
            );
        }
    }
}

fn execute(engine: &str, case: &Case, biber: bool) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    inputs(root);
    let label =
        format!("{}{}-{engine}", case.name, if biber { "-biber" } else { "" });
    let evidence = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/task-prep/crash-recovery/t70/tex")
        .join(format!("{label}-{}", std::process::id()));
    fs::create_dir_all(&evidence).unwrap();
    let input = root.join(case.source);
    if biber {
        let source = fs::read_to_string(&input).unwrap();
        assert!(source.contains("biblatex_backend: bibtex\n"));
        fs::write(&input, source.replacen("biblatex_backend: bibtex\n", "", 1))
            .unwrap();
    }
    let source = fs::read_to_string(&input).unwrap();
    let config = root.join(format!("{label}.toml"));
    fs::write(
        &config,
        format!("{CONFIG}[latex]\nengine='{engine}'\n{}", case.settings),
    )
    .unwrap();
    let output = root.join(format!("{label}.tex"));
    let template = case.template.map(|name| root.join("latex").join(name));
    let built = run_writer(root, &config, &input, &output, template.as_deref());
    record(&evidence, "writer", &built);
    assert!(output.is_file(), "{}", String::from_utf8_lossy(&built.stderr));
    let tex = fs::read_to_string(&output).unwrap();
    if case.tour {
        tour_citations(case, &source, &built, &tex);
    } else {
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
    }
    fs::write(evidence.join("document.tex"), &tex).unwrap();
    let repeated =
        run_writer(root, &config, &input, &output, template.as_deref());
    record(&evidence, "writer-repeat", &repeated);
    assert_eq!(fs::read_to_string(&output).unwrap(), tex);
    if case.tour {
        tour_citations(case, &source, &repeated, &tex);
    } else {
        assert!(
            repeated.status.success(),
            "{}",
            String::from_utf8_lossy(&repeated.stderr)
        );
    }
    let outdir = format!("out-{label}");
    let compiled = Command::new("latexmk")
        .current_dir(root)
        .arg(if engine == "pdflatex" {
            "-pdf"
        } else {
            "-xelatex"
        })
        .args(["-interaction=nonstopmode", "-halt-on-error"])
        .arg(format!("-outdir={outdir}"))
        .arg(format!("{label}.tex"))
        .output()
        .unwrap();
    record(&evidence, "latexmk", &compiled);
    let log =
        fs::read(root.join(&outdir).join(format!("{label}.log"))).unwrap();
    let log = String::from_utf8_lossy(&log).into_owned();
    fs::write(evidence.join("document.log"), &log).unwrap();
    let pdf = root.join(&outdir).join(format!("{label}.pdf"));
    if pdf.is_file() {
        fs::copy(&pdf, evidence.join("document.pdf")).unwrap();
    }
    assert!(
        compiled.status.success(),
        "{label}; evidence {}\n{}\n{}\n{log}",
        evidence.display(),
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert!(!log.lines().any(|line| line.starts_with('!')), "{label}: {log}");
    if case.tour {
        bibliography_evidence(root, &outdir, &label, &evidence, biber);
        let undefined = Regex::new(
            r"(?im)^.*(?:citation.*undefined|undefined citations).*$",
        )
        .unwrap();
        assert!(!undefined.is_match(&log), "{label}: {log}");
    }
    if engine == "pdflatex" {
        assert!(!log.contains("Missing character"), "{label}: {log}");
    } else {
        missing_characters(&log, &String::from_utf8_lossy(&built.stderr));
    }
    assert!(pdf.is_file());
    let text = Command::new("gs")
        .args(["-q", "-sDEVICE=txtwrite", "-o", "-"])
        .arg(&pdf)
        .output()
        .unwrap();
    record(&evidence, "ghostscript", &text);
    assert!(
        text.status.success(),
        "{label}: {}",
        String::from_utf8_lossy(&text.stderr)
    );
    let text = String::from_utf8_lossy(&text.stdout);
    assert!(!text.contains("??"), "{label}: {text}");
    if case.tour {
        let plain = text.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(plain.contains("Table 1"), "{label}: {plain}");
        assert!(plain.contains("Listing 1"), "{label}: {plain}");
        if case.name == "byo" {
            assert!(plain.contains("Test venue"));
        }
    }
    eprintln!("{label}: real TeX protocol evidence {}", evidence.display());
}

fn protocol(engine: &str) {
    for tool in ["latexmk", engine, "gs"] {
        if !available(tool) {
            eprintln!("skipping: latexmk/{engine}/{tool} not found");
            return;
        }
    }
    let biber = available("biber");
    let bibtex = available("bibtex");
    for case in CASES {
        if case.xelatex_only && engine != "xelatex" {
            continue;
        }
        if !case.tour || bibtex {
            execute(engine, case, false);
        } else {
            eprintln!(
                "skipping: {} BibTeX protocol; bibtex not found",
                case.name
            );
        }
        if case.name == "sample-biblatex" && biber {
            execute(engine, case, true);
        }
    }
    if !biber {
        eprintln!(
            "skipping: optional sample-biblatex biber variant; biber not found"
        );
    }
}

#[test]
fn optional_pdflatex_protocol() {
    protocol("pdflatex");
}

#[test]
fn optional_xelatex_protocol() {
    protocol("xelatex");
}
