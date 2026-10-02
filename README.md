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
make cargo-setup
```

### Shared Cargo artifacts across worktrees

Run `make cargo-setup` once in each checkout or newly created Git worktree (Python 3.11+).
The setup targets use `python` on Windows and `python3` elsewhere; override with
`make cargo-setup PYTHON=python3.11` if needed.
It generates an ignored `.cargo/config.toml` with absolute paths to the main checkout's
`target/` and a pinned sccache binary downloaded from its official release and verified against
its published SHA-256 checksum. The tool lives under the main checkout's `dist/tools/sccache/`;
its disk cache lives under `dist/cache/sccache/` and is limited to 5 GB. No global installation
or shell configuration is changed. Setup refuses to overwrite an existing unmanaged Cargo config.

Plain Cargo commands, nextest, and IDE Cargo builds use this configuration. Incremental compilation
is disabled so sccache can cache Rust compilation. Worktrees share dependencies and artifacts,
but different features, toolchains, and branch contents can still grow the shared target directory.
Cargo serializes simultaneous builds that share it.

Local Make build/check targets validate setup and reject conflicting environment overrides,
including incremental-enabling Rust flags and external sccache sockets. Inherited workspace
compiler wrappers are disabled so sccache receives rustc directly.
Use `make cargo-cache-check` to check setup and `make cargo-cache-test` to test the setup tooling.
CI retains its existing runner-local caching, and packaging keeps its explicit release target
directories. Direct Cargo flags/environment overrides can bypass the setup and should not be used
for ordinary development builds.

For an older worktree without the setup script, run
`python3 /absolute/path/to/main-checkout/scripts/cargo-setup.py` from that worktree's root.
Keep the main checkout in place; if it moves, rerun setup in every worktree.
Existing worktree `target/` directories are left untouched and can be removed after checking
that Cargo uses the shared directory. **`cargo clean` now cleans shared artifacts for all worktrees.**

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

Native UI interaction and screenshot-regression tests run on macOS with `make visual-test`; update reviewed snapshots with `make visual-update`. Package the app with `make package-macos RELEASE_ID="local"` on macOS or `make package-windows RELEASE_ID="local"` on Windows. Packaging requires Python 3 and .NET 8; tooling and caches stay under `dist/`. Windows produces a per-user MSI, setup executable, and portable ZIP; macOS produces a universal app ZIP. The Windows executable is unsigned; the macOS app is ad-hoc signed, not Developer ID signed or notarized. Windows requires version 1703 or newer for system ICU.

Packaged installations check GitHub Releases automatically unless disabled in Settings. A separate setting enables nightly updates. **Software Updates…** in the session menu offers download and explicit **Update and restart**, preserving the unsaved-session prompt. Development builds and older distributions without in-app updates need a one-time installation of a new packaged release. See [Packaging and software updates](docs/releases.md) for channels, release assets, and verification limits.

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
