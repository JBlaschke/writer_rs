//! Verbatim environments, safe terminator recovery and listing captions.

use crate::config::LatexCode;
use crate::markdown::code::CodeAttrs;

use super::math::UnicodeContext;
use super::*;

impl Emitter<'_> {
    pub(super) fn code(
        &mut self, lang: Option<&str>, text: &str, attrs: &Attrs,
        context: Context, caption: Option<Caption<'_>>,
    ) -> Output {
        if context.footnote {
            let mut output = Output::default();
            if let Some(caption) = caption {
                self.usage.listing_caption = true;
                self.usage.caption = true;
                output.literal(
                    "\\begingroup\n\\captionsetup{type=writerlisting}\n",
                );
                let caption = self.caption(caption, context);
                if !caption.is_empty() {
                    output.append(caption);
                    output.literal("\n");
                }
            } else {
                output.literal(&self.label(attrs.id.as_deref(), false));
                if !output.is_empty() {
                    output.literal("\n");
                }
            }
            self.unicode(text, UnicodeContext::Code);
            output.literal("{\\ttfamily ");
            for (index, line) in text.split('\n').enumerate() {
                if index > 0 {
                    output.literal("\\newline ");
                }
                output.literal(&escape::code(line, self.map));
            }
            output.literal("}");
            if caption.is_some() {
                output.literal("\n\\endgroup");
            }
            return output;
        }
        let code = CodeAttrs::from_attrs(attrs);
        let notebook_output = attrs.classes.iter().any(|class| {
            matches!(
                class.as_str(),
                "output-text"
                    | "output-stream-stdout"
                    | "output-stream-stderr"
                    | "output-error"
                    | "output-json",
            )
        });
        let mut options = Vec::new();
        if code.numbered {
            options.push("numbers=left".to_owned());
        }
        if code.start != 1 {
            options.push(format!("firstnumber={}", code.start));
        }
        let listings =
            self.config.latex.code == LatexCode::Listings && !notebook_output;
        let highlighted = (self.config.latex.code == LatexCode::Highlighted
            && !notebook_output)
            .then(|| highlight::highlighted(text, lang, code.plain))
            .flatten();
        let mut env = if listings {
            "lstlisting"
        } else if highlighted.is_some() {
            "writercode"
        } else {
            "Verbatim"
        };
        if listings {
            if let Some(language) = lang.and_then(highlight::listings_language)
            {
                options.insert(0, format!("language={language}"));
            } else if let Some(language) = lang {
                self.once(format!("listings:{}", language.to_ascii_lowercase()), Level::Warning,
                    "latex-listings-language", format!(
                        "latex: listings has no language for `{language}`",
                    ));
            }
        }
        let terminator = format!("\\end{{{env}}}");
        let recovery = env != "writercode" && text.contains(&terminator);
        if recovery {
            self.finding(Level::Error, "latex-verbatim-end", format!(
                "latex: code contains `{terminator}`; emitted escaped writercode",
            ));
            env = "writercode";
            // `language`, `caption`, `title` and `label` belong to listings;
            // fancyvrb fallback retains only the ordinary line-number options.
            options.retain(|option| !option.starts_with("language="));
        }
        let inline_listing_caption = listings && !recovery && caption.is_some();
        let mut output = Output::default();
        if let Some(caption) = caption {
            if inline_listing_caption {
                if let Some(inlines) = caption.caption {
                    let previous = self.span.clone();
                    self.span = caption.span.cloned().or(previous.clone());
                    let (full, short) = self.moving_inlines(inlines, context);
                    self.span = previous;
                    let title = if caption.numbered {
                        match short {
                            Some(short) => {
                                format!("[{}]{}", short.text, full.text)
                            }
                            None => full.text,
                        }
                    } else {
                        full.text
                    };
                    options.push(format!(
                        "{}={{{title}}}",
                        if caption.numbered { "caption" } else { "title" }
                    ));
                } else if caption.numbered {
                    options.push("caption={}".into());
                }
                if let Some(id) = &caption.attrs.id {
                    options
                        .push(format!("label={}", crate::resolve::anchor(id)));
                }
            } else {
                self.usage.listing_caption = true;
                self.usage.caption = true;
                output.literal(
                    "\\begingroup\n\\captionsetup{type=writerlisting}\n",
                );
                let caption = self.caption(caption, context);
                if !caption.is_empty() {
                    output.append(caption);
                    output.literal("\n");
                }
            }
        } else {
            output.literal(&self.label(attrs.id.as_deref(), false));
            if !output.is_empty() {
                output.literal("\n");
            }
        }
        self.usage.code = true;
        self.usage.listings |= env == "lstlisting";
        self.usage.highlighted |= env == "writercode";
        self.usage.color |= highlighted.is_some();
        self.unicode(
            text,
            if env == "lstlisting" {
                UnicodeContext::Listings
            } else {
                UnicodeContext::Verbatim
            },
        );
        output.literal(&format!(
            "\\begin{{{env}}}{}\n",
            if options.is_empty() {
                String::new()
            } else {
                format!("[{}]", options.join(","))
            }
        ));
        let body = if recovery {
            escape::writercode(text).into_owned()
        } else {
            highlighted.unwrap_or_else(|| text.to_owned())
        };
        output.literal(&body);
        if !body.ends_with('\n') {
            output.literal("\n");
        }
        output.literal(&format!("\\end{{{env}}}"));
        if caption.is_some() && !inline_listing_caption {
            output.literal("\n\\endgroup");
        }
        output
    }
}
