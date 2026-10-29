//! Keep the user guide aligned with the installed writer command tree.
//!
//! Normal tests are read-only. WRITER_UPDATE_GUIDE=1 updates generated help
//! and default-config fences; rerun without it to validate the whole guide.
//! Notebook execution, package installation, TeX and walkthrough commands are
//! separate explicit checks, never inferred by executing every shell fence.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use clap::CommandFactory;
use comrak::nodes::NodeValue;
use comrak::{Anchorizer, Arena, Options, parse_document};
use regex::Regex;
use writer::cli::Cli;
use writer::config::Config;

const CONFIG_MARKER: &str = "<!-- generated: writer dump --default-config -->";
const GUIDE_PAGES: &[&str] = &[
    "README.md", "getting-started.md", "authoring.md", "notebooks.md",
    "typst.md", "outputs.md", "customization.md", "configuration.md", "cli.md",
    "troubleshooting.md",
];
const EXAMPLES: &[&str] =
    &["basic", "notebook", "mixed", "typst", "latex-template"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn guide(name: &str) -> PathBuf {
    root().join("docs/guide").join(name)
}

fn read(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn updating() -> bool {
    std::env::var_os("WRITER_UPDATE_GUIDE").is_some_and(|v| v == "1")
}

fn writer_stdout(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_writer"))
        .args(args)
        .current_dir(root())
        .env("COLUMNS", "100")
        .env("NO_COLOR", "1")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("RUST_LOG")
        .stdin(Stdio::null())
        .output()
        .expect("run writer");
    assert!(
        output.status.success(),
        "writer {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("UTF-8 command output")
}

// Editors can strip trailing spaces and blank lines from help fences.
fn normalize(text: &str) -> String {
    let mut lines: Vec<_> = text.lines().map(str::trim_end).collect();
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

struct Fence {
    info: String,
    start: usize,
    end: usize,
    content: String,
}

fn fence_open(line: &str) -> Option<(char, usize, &str)> {
    let c = line.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let len = line.chars().take_while(|&x| x == c).count();
    (len >= 3).then(|| (c, len, &line[len..]))
}

fn fences(text: &str) -> Vec<Fence> {
    let lines: Vec<_> = text.lines().collect();
    let mut found = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if let Some((c, len, info)) = fence_open(lines[index]) {
            let start = index + 1;
            let end = (start..lines.len())
                .find(|&next| {
                    let line = lines[next].trim_end();
                    line.chars().count() >= len
                        && line.chars().all(|next| next == c)
                })
                .unwrap_or_else(|| {
                    panic!("unclosed guide fence at line {}", index + 1)
                });
            found.push(Fence {
                info: info.trim().to_string(),
                start,
                end,
                content: lines[start..end].join("\n"),
            });
            index = end + 1;
        } else {
            index += 1;
        }
    }
    found
}

fn replace_lines(
    text: &str, start: usize, end: usize, content: &str,
) -> String {
    let lines: Vec<_> = text.lines().collect();
    let mut output = lines[..start].to_vec();
    output.extend(content.lines());
    output.extend(&lines[end..]);
    output.join("\n") + "\n"
}

fn command_paths(
    command: &clap::Command, prefix: Vec<String>, out: &mut Vec<Vec<String>>,
) {
    out.push(prefix.clone());
    for child in command.get_subcommands() {
        if child.get_name() == "help" {
            continue;
        }
        let mut next = prefix.clone();
        next.push(child.get_name().to_string());
        command_paths(child, next, out);
    }
}

fn help_words(fence: &Fence) -> Option<Vec<String>> {
    if fence.info != "text" {
        return None;
    }
    let first = fence.content.lines().next()?;
    let words = first.strip_prefix("$ writer")?.strip_suffix("--help")?;
    Some(words.split_whitespace().map(str::to_string).collect())
}

fn help_command(words: &[String]) -> String {
    let mut parts = vec!["writer".to_string()];
    parts.extend_from_slice(words);
    parts.push("--help".into());
    parts.join(" ")
}

fn config_fence(text: &str) -> Fence {
    let marker = text
        .lines()
        .position(|line| line.trim() == CONFIG_MARKER)
        .expect("configuration guide has its generated marker");
    fences(text)
        .into_iter()
        .find(|fence| fence.start == marker + 2 && fence.info == "toml")
        .expect("TOML fence directly follows the generated marker")
}

#[test]
fn generated_reference_blocks_match_the_binary() {
    let path = guide("cli.md");
    let text = read(&path);
    let blocks: Vec<_> = fences(&text)
        .into_iter()
        .filter_map(|fence| help_words(&fence).map(|words| (words, fence)))
        .collect();
    let mut expected = Vec::new();
    command_paths(&Cli::command(), Vec::new(), &mut expected);
    let expected: BTreeSet<_> = expected.into_iter().collect();
    let actual: BTreeSet<_> =
        blocks.iter().map(|(words, _)| words.clone()).collect();
    assert_eq!(actual, expected, "every command needs exactly one help fence");
    assert_eq!(blocks.len(), actual.len(), "duplicate help fence");

    let mut updated = text.clone();
    for (words, fence) in blocks.iter().rev() {
        let mut args: Vec<_> = words.iter().map(String::as_str).collect();
        args.push("--help");
        let help = normalize(&writer_stdout(&args));
        if updating() {
            let content = format!("$ {}\n{help}", help_command(words));
            updated = replace_lines(&updated, fence.start, fence.end, &content);
        } else {
            let shown =
                fence.content.lines().skip(1).collect::<Vec<_>>().join("\n");
            assert_eq!(
                normalize(&shown),
                help,
                "stale `{}` fence; regenerate with WRITER_UPDATE_GUIDE=1",
                help_command(words)
            );
        }
    }
    if updating() {
        fs::write(path, updated).expect("update generated CLI help");
    }

    let path = guide("configuration.md");
    let text = read(&path);
    let fence = config_fence(&text);
    let printed = writer_stdout(&["dump", "--default-config"]);
    if updating() {
        fs::write(
            path,
            replace_lines(&text, fence.start, fence.end, &normalize(&printed)),
        )
        .expect("update generated default config");
    } else {
        assert_eq!(normalize(&fence.content), normalize(&printed));
    }
}

type Key = (String, String);

fn config_keys() -> BTreeMap<Key, Option<toml::Value>> {
    let json = serde_json::to_value(Config::default()).expect("config JSON");
    let mut keys = BTreeMap::new();
    for (table, values) in json.as_object().expect("config object") {
        for (key, value) in values.as_object().expect("config table") {
            let value = if value.is_null() {
                None
            } else {
                Some(toml::Value::try_from(value).expect("config TOML value"))
            };
            keys.insert((table.clone(), key.clone()), value);
        }
    }
    keys
}

fn documented_defaults(text: &str) -> BTreeMap<Key, String> {
    let heading = Regex::new(r"^#+ .*`\[([a-z]+)\]`").unwrap();
    let row = Regex::new(r"^\| `([A-Za-z_]+)` \| (.+?) \|").unwrap();
    let mut table = None;
    let mut rows = BTreeMap::new();
    let mut fence = None;
    for line in text.lines() {
        if let Some((c, len)) = fence {
            let line = line.trim_end();
            if line.chars().count() >= len && line.chars().all(|x| x == c) {
                fence = None;
            }
            continue;
        }
        if let Some((c, len, _)) = fence_open(line) {
            fence = Some((c, len));
            continue;
        }
        if let Some(captures) = heading.captures(line) {
            table = Some(captures[1].to_string());
        } else if line.starts_with('#') {
            table = None;
        } else if let (Some(table), Some(captures)) =
            (&table, row.captures(line))
        {
            let key = (table.clone(), captures[1].to_string());
            assert!(
                rows.insert(key.clone(), captures[2].trim().to_string())
                    .is_none(),
                "configuration key {key:?} is documented twice"
            );
        }
    }
    rows
}

fn default_cell(cell: &str) -> Option<toml::Value> {
    if cell == "unset" {
        return None;
    }
    let literal = cell
        .strip_prefix('`')
        .and_then(|cell| cell.strip_suffix('`'))
        .expect("default cell is a backtick-delimited TOML value");
    let mut table: toml::Table = format!("value = {literal}")
        .parse()
        .expect("default cell is valid TOML");
    table.remove("value")
}

#[test]
fn configuration_tables_cover_every_key_and_actual_default() {
    if updating() {
        return;
    }
    let documented = documented_defaults(&read(&guide("configuration.md")));
    let expected = config_keys();
    assert_eq!(
        documented.keys().collect::<BTreeSet<_>>(),
        expected.keys().collect::<BTreeSet<_>>()
    );
    for (key, value) in expected {
        assert_eq!(default_cell(&documented[&key]), value, "{key:?}");
    }
}

#[test]
fn printed_default_config_is_complete_and_changes_no_defaults() {
    if updating() {
        return;
    }
    let printed = writer_stdout(&["dump", "--default-config"]);
    assert_eq!(Config::from_toml_str(&printed).unwrap(), Config::default());
    let parsed: toml::Table = printed.parse().expect("default output is TOML");
    let mut actual = BTreeMap::new();
    for (table, values) in parsed {
        for (key, value) in values.as_table().expect("top-level config table") {
            actual.insert((table.clone(), key.clone()), Some(value.clone()));
        }
    }
    // Writer prints active defaults, unlike the donor's commented settings.
    assert_eq!(actual, config_keys());
}

fn documents() -> Vec<PathBuf> {
    let mut documents = vec![root().join("README.md")];
    for name in GUIDE_PAGES {
        assert!(guide(name).is_file(), "required guide page {name}");
    }
    for name in EXAMPLES {
        assert!(
            root().join("examples").join(name).join("README.md").is_file(),
            "required example README {name}"
        );
    }
    let mut pages: Vec<_> = fs::read_dir(guide(""))
        .expect("guide directory")
        .map(|entry| entry.expect("guide entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .collect();
    pages.sort();
    documents.extend(pages);
    let mut examples: Vec<_> = fs::read_dir(root().join("examples"))
        .expect("example directory")
        .map(|entry| entry.expect("example entry").path().join("README.md"))
        .filter(|path| path.is_file())
        .collect();
    examples.sort();
    documents.extend(examples);
    documents
}

fn link_targets(text: &str) -> Vec<String> {
    let arena = Arena::new();
    let node = parse_document(&arena, text, &Options::default());
    node.descendants()
        .filter_map(|node| match &node.data.borrow().value {
            NodeValue::Link(link) | NodeValue::Image(link) => {
                Some(link.url.clone())
            }
            _ => None,
        })
        .collect()
}

fn anchors(text: &str) -> BTreeSet<String> {
    let arena = Arena::new();
    let node = parse_document(&arena, text, &Options::default());
    let mut anchorizer = Anchorizer::new();
    let mut anchors = BTreeSet::new();
    for heading in node.descendants() {
        if !matches!(&heading.data.borrow().value, NodeValue::Heading(_)) {
            continue;
        }
        let mut title = String::new();
        for child in heading.descendants() {
            match &child.data.borrow().value {
                NodeValue::Text(text) => title.push_str(text),
                NodeValue::Code(code) => title.push_str(&code.literal),
                NodeValue::SoftBreak | NodeValue::LineBreak => title.push(' '),
                _ => {}
            }
        }
        anchors.insert(anchorizer.anchorize(&title));
    }
    anchors
}

fn external(target: &str) -> bool {
    if target.starts_with("//") {
        return true;
    }
    target.split_once(':').is_some_and(|(scheme, _)| {
        !scheme.is_empty()
            && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    })
}

fn percent_decode(text: &str) -> Result<String, String> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let digits = bytes
                .get(index + 1..index + 3)
                .and_then(|digits| std::str::from_utf8(digits).ok())
                .and_then(|digits| u8::from_str_radix(digits, 16).ok())
                .ok_or_else(|| format!("invalid percent escape: {text}"))?;
            decoded.push(digits);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|error| error.to_string())
}

fn link_problem(document: &Path, target: &str) -> Option<String> {
    if external(target) {
        return None;
    }
    let (path, fragment) = target
        .split_once('#')
        .map_or((target, None), |(path, fragment)| (path, Some(fragment)));
    let path = path.split_once('?').map_or(path, |(path, _)| path);
    let path = match percent_decode(path) {
        Ok(path) => path,
        Err(error) => return Some(error),
    };
    if Path::new(&path).is_absolute() {
        return Some("local documentation link must be relative".into());
    }
    let file = if path.is_empty() {
        document.to_path_buf()
    } else {
        document.parent().expect("document parent").join(path)
    };
    if !file.exists() {
        return Some("no such local file".into());
    }
    if file.extension().is_some_and(|ext| ext == "md")
        && let Some(fragment) = fragment.filter(|fragment| !fragment.is_empty())
    {
        match percent_decode(fragment) {
            Ok(fragment) if anchors(&read(&file)).contains(&fragment) => {}
            Ok(_) => return Some("no heading with this anchor".into()),
            Err(error) => return Some(error),
        }
    }
    None
}

#[test]
fn user_document_links_images_and_anchors_resolve() {
    if updating() {
        return;
    }
    let mut broken = Vec::new();
    let mut local = 0;
    for document in documents() {
        for target in link_targets(&read(&document)) {
            if !external(&target) {
                local += 1;
            }
            if let Some(problem) = link_problem(&document, &target) {
                broken.push(format!(
                    "{}: {target}: {problem}",
                    document.display()
                ));
            }
        }
    }
    assert!(broken.is_empty(), "{}", broken.join("\n"));
    assert!(local > 0, "guide must link to local documentation and examples");
}

#[test]
fn user_documents_have_no_internal_milestone_identifiers() {
    if updating() {
        return;
    }
    let identifier = Regex::new(r"\bT(?:[0-9]{2}|[0-9][A-Z])\b").unwrap();
    for document in documents() {
        let text = read(&document);
        assert!(
            !identifier.is_match(&text),
            "internal milestone identifier in {}",
            document.display()
        );
    }
}

#[test]
fn markdown_scan_handles_code_reference_images_and_duplicate_anchors() {
    let text = "# A & *B*\n\n# A & B\n\n\
        [go][ref] ![image](picture%20one.png) `<x>[fake](missing)`\n\n\
        [ref]: page.md#one\n\n```text\n[ignored](also-missing)\n```\n";
    assert_eq!(link_targets(text), ["page.md#one", "picture%20one.png"]);
    assert_eq!(anchors(text), BTreeSet::from(["a--b".into(), "a--b-1".into()]));
    assert_eq!(percent_decode("picture%20one.png").unwrap(), "picture one.png");
    assert!(percent_decode("bad%xx").is_err());
}

#[test]
fn anchors_preserve_combining_marks_and_avoid_literal_suffix_collisions() {
    let text = "# a\n\n# a\n\n# a-1\n\n# a\n\n# Cafe\u{301}\n";
    assert_eq!(
        anchors(text),
        BTreeSet::from([
            "a".into(),
            "a-1".into(),
            "a-1-1".into(),
            "a-2".into(),
            "cafe\u{301}".into(),
        ])
    );
}

#[test]
fn link_validation_rejects_missing_file_and_fragment() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("page.md");
    fs::write(&path, "# First\n\n# First\n").unwrap();
    assert!(link_problem(&path, "#first-1").is_none());
    assert!(link_problem(&path, "missing.md").is_some());
    assert!(link_problem(&path, "page.md#absent").is_some());
    assert!(link_problem(&path, "https://example.com/not-fetched").is_none());
    assert!(link_problem(&path, "mailto:author@example.com").is_none());
}

#[test]
fn config_default_cells_compare_values_without_layout_assumptions() {
    let text = "## `[build]`\n\n\
        | Key | Default | Purpose |\n|---|---|---|\n\
        | `targets` | `[ \"html\", \"pdf\" ]` | Output choices |\n";
    let rows = documented_defaults(text);
    let key = ("build".to_string(), "targets".to_string());
    assert_eq!(
        default_cell(&rows[&key]),
        Some(toml::Value::Array(vec!["html".into(), "pdf".into()]))
    );
    assert_eq!(default_cell("unset"), None);
}
