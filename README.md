# writer

Write a document once — in Markdown, as a Jupyter notebook, or in typst — and get HTML, PDF,
Word (`.docx`) and LaTeX source from it, with a live preview while you write.

writer is the document-shaped sibling of [presenter_rs](https://github.com/JBlaschke/presenter_rs).
It is being built from a plan written for a fleet of independent agents:

- [`PLAN.md`](PLAN.md) — goals, architecture, decisions, milestones and the task graph.
- [`docs/spec/`](docs/spec/) — the frozen contracts (IR, dialects, outputs, config, CLI, live reload, vendored assets).
- [`docs/tasks/`](docs/tasks/) — one file per task and the [task board](docs/tasks/README.md).
- [`docs/research/`](docs/research/) — the verified facts the plan rests on (typst embedding, comrak, DOCX, math, citations, LaTeX, notebooks, presenter_rs inventory).

Nothing is implemented yet; start with the task board.
