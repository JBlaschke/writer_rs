# An executed notebook as a document

`analysis.ipynb` stores a seeded noisy signal, a rolling-mean plot, summary
statistics, a Markdown display, and a saved traceback. Writer reads those
outputs; building a document never executes Python.

Build the HTML document from the repository root:

```sh
cargo run -- build examples/notebook/analysis.ipynb
```

The plot uses `#| label: fig-rolling` and `#| fig-cap:`; the prose references
`@fig-rolling`. The DataFrame uses `#| tbl-cap:`. These comments set document
options while remaining ordinary comments in Python.

The saved outputs were lifted from the executed `presenter_rs` example. The
writer adaptation changes Markdown, metadata, and option comments only; its
Python bodies and stored outputs have not been re-executed.

To regenerate the outputs in a Python environment with Jupyter, nbconvert,
ipykernel, matplotlib, numpy, and pandas installed, run:

```sh
python -m jupyter nbconvert --to notebook --execute --allow-errors --inplace \
    examples/notebook/analysis.ipynb
```

`--allow-errors` keeps the final deliberate division-by-zero traceback.
