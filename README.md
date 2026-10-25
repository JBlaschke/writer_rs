# writer

Write a document once — in Markdown, as a Jupyter notebook, or in typst — and get HTML, PDF,
Word (`.docx`) and LaTeX source from it, with a live preview while you write.

writer is the document-shaped sibling of [presenter_rs](https://github.com/JBlaschke/presenter_rs).
It is being built from a plan written for a fleet of independent agents:

- [`PLAN.md`](PLAN.md) — goals, architecture, decisions, milestones and the task graph.
- [`docs/spec/`](docs/spec/) — the frozen contracts (IR, dialects, outputs, config, CLI, live reload, vendored assets).
- [`docs/tasks/`](docs/tasks/) — one file per task and the [task board](docs/tasks/README.md).
- [`docs/research/`](docs/research/) — the verified facts the plan rests on (typst embedding, comrak, DOCX, math, citations, LaTeX, notebooks, presenter_rs inventory).

Implementation has started with T00: the Rust crate and module skeleton, pinned dependencies,
checksummed vendored assets, embedded notices, build metadata, and CI. Document conversion and
the CLI commands arrive in later tasks; the current binary reports that T16 is pending and
exits with status 2.

To validate the scaffold:

```bash
cargo build --locked
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
```

See the [task board](docs/tasks/README.md) for implementation status and
[build measurements](docs/dev/build-times.md) for the baseline.
