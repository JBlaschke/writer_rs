//! Exact positive tour commands from latex-output.md §7.1, without rerendering.

use writer::config::LatexCite;

pub fn assert_tour_commands(tex: &str, family: LatexCite) {
    assert!(!tex.contains("{[}?doe2020; lee2019{]}"));
    assert!(!tex.contains("{[}?knuth1984{]}"));
    // The unchanged tour's negative shorthand is a second Year citation.
    // Hidden/None citations have separate positive fixture and unit coverage.
    assert!(!tex.contains("\\nocite{knuth1984}"), "{tex}");
    let commands: &[&str] = match family {
        LatexCite::Biblatex => &[
            "\\autocites{doe2020}[p.~12]{lee2019}",
            "\\autocite[see][]{knuth1984}",
            "\\textcite{knuth1984}",
            "\\autocite*{knuth1984}",
            "\\citeauthor{knuth1984}",
            "\\fullcite{knuth1984}",
        ],
        LatexCite::Natbib => &[
            "\\citetext{\\citealp{doe2020}; \\citealp[p.~12]{lee2019}}",
            "\\citep[see][]{knuth1984}",
            "\\citet{knuth1984}",
            "\\citeyearpar{knuth1984}",
            "\\citeauthor{knuth1984}",
            "\\citep{knuth1984}",
        ],
        LatexCite::Plain => &[
            "\\cite{doe2020}; \\cite[p.~12]{lee2019}",
            "see \\cite{knuth1984}",
        ],
    };
    for command in commands {
        assert!(tex.contains(command), "missing {command}: {tex}");
    }
    if family == LatexCite::Plain {
        assert_eq!(tex.matches("\\cite{knuth1984}").count(), 6, "{tex}");
    } else {
        let year = if family == LatexCite::Biblatex {
            "\\autocite*{knuth1984}"
        } else {
            "\\citeyearpar{knuth1984}"
        };
        assert_eq!(tex.matches(year).count(), 2, "{tex}");
    }
}
