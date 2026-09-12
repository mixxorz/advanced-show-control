# Advanced Show Control

## Disclaimer

This project is not affiliated with, endorsed by, or supported by Waves Audio Ltd.
or the Waves eMotion LV1 product team. Waves, eMotion, and LV1 are trademarks of
their respective owners. This software and documentation are provided without
warranty; use them at your own risk.

Advanced Show Control is a native Rust desktop application built with GPUI Kit. It supports macOS 15 or newer and Windows 10 or newer. LV1 networking, fades, persistence, and other asynchronous work run on a dedicated Tokio runtime; GPUI owns the native event loop and rendering. The repository has no JavaScript frontend or Tauri host.

## Repository Layout

- `app/`: production `advanced-show-control` crate, including domain actors and the GPUI Kit UI.
- `dev-tools/`: separate non-publishable crate containing the hardware-smoke and LV1 probe CLIs.
- `docs/`: engineering and architecture documentation.
- `site/`: published user manual.

## Development Tooling

`rust-toolchain.toml` pins stable Rust and includes `rustfmt` and `clippy`.

Install the local tooling with:

```bash
cargo install cargo-nextest --locked
pre-commit install
```

Run the native app and standard checks with:

```bash
make dev
make check
```

Equivalent focused Rust commands include:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo build --workspace
```

Prefer `cargo nextest run` for Rust tests, including targeted checks such as `cargo nextest run -p advanced-show-control fade`. Use `cargo test` only when a specific Rust test-harness feature is required.

`pre-commit` runs Rust formatting and workspace-wide clippy. It does not run tests.

Native UI interaction and screenshot-regression tests run on macOS with `make visual-test`; update reviewed snapshots with `make visual-update`. Build distributable archives with `make package-macos RELEASE_ID="local"` on macOS or `make package-windows RELEASE_ID="local"` on Windows. The Windows executable is unsigned; the macOS app is ad-hoc signed, not Developer ID signed or notarized.

The separate tools run with `make probe ARGS="..."` and `make smoke`. Hardware smoke requires an LV1-compatible environment. After every smoke run, inspect `logs/debug-smoke-report.txt`; it is the authoritative suite result, not terminal output.

## Documentation

The documentation targets create or reuse the ignored `.venv-docs` virtual
environment and install the pinned dependencies there. No virtual-environment
activation is required. Build or serve the local site with:

```bash
make docs-build
make docs-serve
```

Run `make docs-install` to install or refresh the documentation dependencies
without building the site.

The published manual is available at https://mitchel.me/advanced-show-control/.
`stable` is the latest numbered release documentation, while `latest` tracks the current `main` branch.

## License

Advanced Show Control is licensed under the GNU General Public License version 3 or later. See [LICENSE](LICENSE) for details.
