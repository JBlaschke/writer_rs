# artifacts

Artifacts of MiTeX, to build MiTeX in standalone mode

## Branches

- [`v0.1.1`]
- [`v0.2.2`]
- [`v0.2.4`]

## Reproduction

### [Default Command Specification](spec/default.rkyv), Since [`v0.1.1`]

You need to install Typst v0.10.0 for this in repository root of [MiTeX](https://github.com/mitex-rs/mitex)

To get metadata, use `typst query`:

```bash
typst query --root . packages/mitex/specs/mod.typ <mitex-packages>
```

To get compacted metadata, run `mitex spec generate`

The mitex-cli is built with following command:

```bash
> cargo build --release --bin mitex
> rustc --version
rustc 1.74.1 (a28077b28 2023-12-04)
> cargo --version
cargo 1.74.1 (ecb9851af 2023-10-18)
```

[`v0.1.1`]: https://github.com/mitex-rs/artifacts/tree/v0.1.1
[`v0.2.2`]: https://github.com/mitex-rs/artifacts/tree/v0.2.2
[`v0.2.4`]: https://github.com/mitex-rs/artifacts/tree/v0.2.4
