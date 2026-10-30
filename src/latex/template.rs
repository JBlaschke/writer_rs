//! Dedicated LaTeX template syntax and required-slot inspection.

use minijinja::syntax::SyntaxConfig;
use minijinja::{AutoEscape, Environment, UndefinedBehavior};

use crate::error::Result;

/// A complete sample document with the writer body/preamble hooks.
pub const SAMPLE: &str = include_str!("../../assets/latex/writer-template.tex");

/// Builds an isolated environment; TeX content is never HTML-escaped.
pub fn environment(map: bool) -> Environment<'static> {
    let mut environment = Environment::new();
    environment.set_syntax(
        SyntaxConfig::builder()
            .block_delimiters("\\BLOCK{", "}")
            .variable_delimiters("\\VAR{", "}")
            .comment_delimiters("\\#{", "}")
            .build()
            .expect("static LaTeX delimiters are valid"),
    );
    environment.set_keep_trailing_newline(true);
    environment.set_trim_blocks(true);
    environment.set_lstrip_blocks(true);
    environment.set_undefined_behavior(UndefinedBehavior::SemiStrict);
    environment.set_auto_escape_callback(|_| AutoEscape::None);
    environment.add_filter("tex", move |text: String| {
        super::escape::text(&text, map).into_owned()
    });
    environment
}

/// Returns required slots absent from the template's syntactic variables.
///
/// Slots inside literal text, comments or raw blocks do not count. Conditional
/// slots count even when a particular render would not take that branch.
pub fn missing_slots(source: &str) -> Result<Vec<&'static str>> {
    let environment = environment(false);
    let template = environment.template_from_str(source)?;
    let variables = template.undeclared_variables(false);
    Ok(["preamble", "body"]
        .into_iter()
        .filter(|slot| !variables.contains(*slot))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use minijinja::context;

    #[test]
    fn static_slot_inspection_observes_real_syntax_and_order() {
        assert_eq!(
            missing_slots("literal preamble/body").unwrap(),
            ["preamble", "body"]
        );
        assert_eq!(missing_slots(r"\VAR{preamble}").unwrap(), ["body"]);
        assert!(missing_slots(SAMPLE).unwrap().is_empty());
        assert!(
            missing_slots(
                r"\BLOCK{if false}\VAR{body}\BLOCK{endif}\VAR{preamble}"
            )
            .unwrap()
            .is_empty()
        );
        assert_eq!(
            missing_slots(r"\#{ \VAR{body} }\VAR{preamble}").unwrap(),
            ["body"]
        );
        assert_eq!(
            missing_slots(r"\BLOCK{raw}\VAR{body}\BLOCK{endraw}\VAR{preamble}")
                .unwrap(),
            ["body"]
        );
        assert!(missing_slots(r"\VAR{").is_err());
    }

    #[test]
    fn delimiters_preserve_tex_and_inserted_content_is_not_reparsed() {
        let environment = environment(false);
        let template = environment
            .template_from_str(concat!(
                "\\#{ ends } stays }\n",
                "% \\VAR{ {'a': 1}['a'] } \\VAR{ \"x}y\" }\n",
                "\\VAR{body}\n",
            ))
            .unwrap();
        let result = template
            .render(context!(body => r"\VAR{untouched} & <x>"))
            .unwrap();
        assert_eq!(result, " stays }\n% 1 x}y\n\\VAR{untouched} & <x>\n");
    }

    #[test]
    fn undefined_guards_are_allowed_but_printing_or_filtering_fails() {
        let environment = environment(true);
        let guard = environment
            .template_from_str(r"\BLOCK{if vars.absent}bad\BLOCK{endif}ok")
            .unwrap();
        assert_eq!(guard.render(context!(vars => context!())).unwrap(), "ok");
        for source in [r"\VAR{vars.absent}", r"\VAR{vars.absent | tex}"] {
            assert!(
                environment
                    .template_from_str(source)
                    .unwrap()
                    .render(context!(vars => context!()))
                    .is_err()
            );
        }
        let escaped =
            environment.template_from_str(r"\VAR{vars.value | tex}").unwrap();
        assert_eq!(
            escaped.render(context!(vars => context!(value => "α &"))).unwrap(),
            r"\ensuremath{\alpha} \&"
        );
    }

    #[test]
    fn sample_is_exactly_the_binding_template_and_uses_all_required_slots() {
        let specification = include_str!("../../docs/spec/latex-output.md");
        let section = specification
            .split_once("### 9.1 `assets/latex/writer-template.tex`")
            .unwrap()
            .1;
        let expected = section
            .split_once("```latex\n")
            .unwrap()
            .1
            .split_once("```")
            .unwrap()
            .0;
        assert_eq!(SAMPLE, expected);
        assert!(missing_slots(SAMPLE).unwrap().is_empty());
    }

    #[test]
    fn static_slots_ignore_attribute_names_and_local_bindings() {
        assert_eq!(
            missing_slots(r"\VAR{preamble}\VAR{vars.body}").unwrap(),
            ["body"]
        );
        assert_eq!(
            missing_slots(concat!(
                "\\BLOCK{set body = 'local'}\\VAR{body}",
                "\\VAR{preamble}",
            ))
            .unwrap(),
            ["body"]
        );
        let owned = [r"\VAR{body}", r"\VAR{preamble}"].join("\n");
        assert!(missing_slots(&owned).unwrap().is_empty());
    }

    #[test]
    fn ordinary_jinja_and_tex_comment_lines_are_not_statement_prefixes() {
        let environment = environment(false);
        let source = concat!(
            "{{ body }} {% set body = 'changed' %} {# body #}\n",
            "% set body = 'changed'\n% \\VAR{body}\n",
        );
        let template = environment
            .template_from_named_str("document.html", source)
            .unwrap();
        assert_eq!(
            template.render(context!(body => "& <kept>")).unwrap(),
            concat!(
                "{{ body }} {% set body = 'changed' %} {# body #}\n",
                "% set body = 'changed'\n% & <kept>\n",
            )
        );
    }

    #[test]
    fn statement_lines_trim_their_whitespace_and_keep_the_final_newline() {
        let environment = environment(false);
        let template = environment
            .template_from_str(concat!(
                "before\n  \\BLOCK{if show}\ninside\n",
                "  \\BLOCK{endif}\nafter\n",
            ))
            .unwrap();
        assert_eq!(
            template.render(context!(show => true)).unwrap(),
            "before\ninside\nafter\n"
        );
        assert_eq!(
            template.render(context!(show => false)).unwrap(),
            "before\nafter\n"
        );
        let fallback = environment
            .template_from_str(r"\VAR{vars.absent | default('') | tex}")
            .unwrap();
        assert_eq!(fallback.render(context!(vars => context!())).unwrap(), "");
    }

    #[test]
    fn sample_guards_optional_code_and_keeps_inserted_body_literal() {
        let environment = environment(false);
        let template = environment.template_from_str(SAMPLE).unwrap();
        let mut values = serde_json::json!({
            "preamble":"PREAMBLE", "body":"BODY & <kept> \\VAR{unparsed}",
            "title":"", "subtitle":"", "authors":[], "date":"",
            "abstract":"", "keywords":[], "lang":"en", "toc":0,
            "bibliography":[], "has_citations":false, "cite":"plain",
            "emit_bibliography":false, "has_math":false, "has_code":false,
            "has_highlighted":false, "has_cells":false, "vars":{},
        });
        let output = template.render(&values).unwrap();
        assert!(output.contains("\\date{}"));
        assert!(output.contains("BODY & <kept> \\VAR{unparsed}"));
        assert!(!output.contains("\\maketitle"));
        assert!(!output.contains("\\fvset"));
        assert!(!output.contains("\\printbibliography"));
        assert!(!output.contains("\\bibliography{"));
        assert!(output.ends_with("\\end{document}\n"));
        values["has_code"] = true.into();
        let output = template.render(&values).unwrap();
        assert!(
            output
                .contains("\\fvset{frame=lines,framesep=2mm,fontsize=\\small}")
        );
        assert_eq!(output.matches("BODY & <kept> \\VAR{unparsed}").count(), 1);
    }
}
