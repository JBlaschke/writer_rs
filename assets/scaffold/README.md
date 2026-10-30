# My report

From this project directory:

```sh
writer build {{document}}
writer serve {{document}}
```

The default build writes into `dist/`. Use `--to html,docx,pdf,latex` to select
other implemented outputs. Writer exports LaTeX source and resources; compiling
that source with a TeX engine is a separate step.

Edit `{{document}}` for the report, `figures/plot.png` for its example figure,
and `theme/styles/custom.css` for HTML styling. The local theme extends the
built-in default, so the parent templates and styles stay available.

{{bibliography_note}}
