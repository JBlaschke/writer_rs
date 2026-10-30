//! `writer init`: deterministic documents and a child theme from embedded files.

use std::path::PathBuf;
use std::process::ExitCode;

use super::create::{self, Directory};
use super::{
    EXIT_OK, GlobalArgs, InitArgs, UsageError, display_path, print_stdout,
};
use crate::vendor;

/// Creates a project without replacing existing scaffold files, even with force.
pub fn run(args: &InitArgs, global: &GlobalArgs) -> anyhow::Result<ExitCode> {
    let source = if args.typst {
        "document.typ"
    } else {
        "document.md"
    };
    let mut names = vec![
        "writer.toml", source, "figures/plot.png", "theme/theme.toml",
        "theme/styles/custom.css", "gitignore", "README.md",
    ];
    if args.bib {
        names.push("refs.bib");
    }
    let mut files = Vec::with_capacity(names.len());
    for name in names {
        let key = format!("scaffold/{name}");
        let mut bytes = vendor::get(&key)
            .ok_or_else(|| {
                anyhow::anyhow!("embedded scaffold file not found: {key}")
            })?
            .into_owned();
        if matches!(name, "document.md" | "document.typ" | "README.md") {
            let text = String::from_utf8(bytes)?;
            bytes = substitute(&text, args, source).into_bytes();
        }
        let name = if name == "gitignore" {
            ".gitignore"
        } else {
            name
        };
        files.push((PathBuf::from(name), bytes));
    }
    files.sort_by(|(left, _), (right, _)| left.cmp(right));
    let policy = if args.force {
        Directory::Merge
    } else {
        Directory::EmptyOrNew
    };
    if let Err(error) = create::tree(&args.dir, &files, policy) {
        if args.force && error.downcast_ref::<UsageError>().is_some() {
            return Err(UsageError(format!(
                "{error}; --force keeps unrelated files and never replaces scaffold files",
            )).into());
        }
        return Err(error);
    }
    if !global.quiet {
        let mut message = format!(
            "Initialized {} ({} files).\n",
            display_path(&args.dir).display(),
            files.len(),
        );
        for (name, _) in &files {
            message.push_str(&format!("  {}\n", name.display()));
        }
        message.push_str(&format!(
            "\nFrom the project directory:\n  writer build {source}\n  writer serve {source}",
        ));
        print_stdout(&message)?;
    }
    Ok(ExitCode::from(EXIT_OK))
}

fn substitute(text: &str, args: &InitArgs, source: &str) -> String {
    let bibliography = if !args.bib {
        ""
    } else if args.typst {
        "    bibliography: path(\"refs.bib\"), cite-style: \"ieee\",\n"
    } else {
        "bibliography: refs.bib\ncite_style: ieee\n"
    };
    let citation = if args.bib {
        "A reference to the example library: @writer2026.\n"
    } else {
        ""
    };
    let bibliography_note = if args.bib {
        "`refs.bib` holds the example bibliography. Add entries there and cite their keys.\n"
    } else {
        ""
    };
    // Every inserted value is fixed text; no value contains another marker.
    text.replace("{{writer_version}}", crate::typst::WRITER_DOC_VERSION)
        .replace("{{bibliography}}", bibliography)
        .replace("{{citation}}", citation)
        .replace("{{document}}", source)
        .replace("{{bibliography_note}}", bibliography_note)
}
