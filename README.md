# Advanced Show Control

## Disclaimer

This project is not affiliated with, endorsed by, or supported by Waves Audio Ltd.
or the Waves eMotion LV1 product team. Waves, eMotion, and LV1 are trademarks of
their respective owners. This software and documentation are provided without
warranty; use them at your own risk.

## Development Tooling

`rust-toolchain.toml` pins stable Rust and includes `rustfmt` and `clippy`.

Install the local tooling with:

```bash
cargo install cargo-nextest --locked
pre-commit install
```

Run the manual checks with:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
```

`pre-commit` runs Rust formatting and workspace-wide clippy. It does not run tests.

## License

Advanced Show Control is licensed under the GNU General Public License version 3 or later. See [LICENSE](LICENSE) for details.
