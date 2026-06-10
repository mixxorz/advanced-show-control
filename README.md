# Advanced Show Control

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

Prefer `cargo nextest run` for Rust tests, including targeted checks such as `cargo nextest run -p advanced-show-control fade`. Use `cargo test` only when a specific Rust test-harness feature is required.

`pre-commit` runs Rust formatting and workspace-wide clippy. It does not run tests.

## License

Advanced Show Control is licensed under the GNU General Public License version 3 or later. See [LICENSE](LICENSE) for details.
