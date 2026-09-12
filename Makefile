.PHONY: help fmt lint test build check \
	rust-fmt rust-lint rust-test rust-build \
	dev-tools-fmt dev-tools-lint dev-tools-test dev-tools-check dev-tools-build \
	docs-install docs-build docs-serve dev dev-watch gallery probe smoke \
	visual-test visual-update package-macos package-windows

DOCS_VENV := .venv-docs
DOCS_PYTHON := $(DOCS_VENV)/bin/python
DOCS_ZENSICAL := $(DOCS_VENV)/bin/zensical

help:
	@printf '%s\n' \
	  'Common targets:' \
	  '  make fmt                  Check Rust formatting' \
	  '  make lint                 Run Clippy for the app and development tools' \
	  '  make test                 Run app and development-tool tests' \
	  '  make build                Build the app and development tools' \
	  '  make check                Run CI-like formatting, lint, test, and build checks' \
	  '' \
	  'Documentation targets:' \
	  '  make docs-install         Install pinned documentation dependencies' \
	  '  make docs-build           Build documentation with strict validation' \
	  '  make docs-serve           Serve documentation locally' \
	  '' \
	  'Native application targets:' \
	  '  make dev                  Run the GPUI application' \
	  '  make dev-watch            Rebuild and relaunch the GPUI app when sources change' \
	  '  make gallery              Open the native component/state gallery' \
	  '  make visual-test          Run GPUI native visual/component tests (macOS)' \
	  '  make visual-update        Update reviewed native visual snapshots (macOS)' \
	  '  make package-macos        Build an ad-hoc-signed universal macOS app archive' \
	  '  make package-windows      Build an unsigned Windows x64 app archive' \
	  '' \
	  'Development-tool targets:' \
	  '  make probe                Run LV1 probe CLI (pass ARGS="...")' \
	  '  make smoke                Run the non-GUI hardware smoke suite quietly' \
	  '  make smoke VERBOSE=1      Run hardware smoke with terminal output'

fmt: rust-fmt dev-tools-fmt

lint: rust-lint dev-tools-lint

test: rust-test dev-tools-test

build: rust-build dev-tools-build

check: fmt lint test build

docs-install:
	@test -x "$(DOCS_PYTHON)" || python3 -m venv "$(DOCS_VENV)"
	"$(DOCS_PYTHON)" -m pip install -r requirements-docs.txt

docs-build: docs-install
	"$(DOCS_ZENSICAL)" build --clean --strict --config-file site/zensical.toml

docs-serve: docs-install
	"$(DOCS_ZENSICAL)" serve --config-file site/zensical.toml

rust-fmt:
	cargo fmt --all -- --check

rust-lint:
	cargo clippy --workspace --all-targets -- -D warnings

rust-test:
	cargo nextest run --workspace

rust-build:
	cargo build --workspace

dev-tools-fmt:
	cargo fmt --manifest-path dev-tools/Cargo.toml -- --check

dev-tools-lint:
	cargo clippy --manifest-path dev-tools/Cargo.toml --all-targets -- -D warnings

dev-tools-test:
	cargo nextest run --manifest-path dev-tools/Cargo.toml

dev-tools-check: dev-tools-fmt dev-tools-lint dev-tools-test dev-tools-build

dev-tools-build:
	cargo build --manifest-path dev-tools/Cargo.toml --all-targets

dev:
	cargo run -p advanced-show-control --bin advanced-show-control

dev-watch:
	cargo watch --poll --no-dot-ignores -w app -w Cargo.toml -w Cargo.lock -x 'run -p advanced-show-control --bin advanced-show-control'

gallery:
	cargo run -p advanced-show-control --features debug-tools --bin native-gallery

visual-test:
	cargo run -p advanced-show-control --features debug-tools --bin native-visual-test -- dist/visual

visual-update:
	ASC_UPDATE_NATIVE_VISUALS=1 cargo run -p advanced-show-control --features debug-tools --bin native-visual-test -- dist/visual

package-macos:
	./scripts/package-macos.sh "$(or $(RELEASE_ID),local)"

package-windows:
	powershell -ExecutionPolicy Bypass -File scripts/package-windows.ps1 -ReleaseId "$(or $(RELEASE_ID),local)"

probe:
	cargo run --manifest-path dev-tools/Cargo.toml --bin lv1-probe -- $(ARGS)

smoke:
	@if [ "$(VERBOSE)" = "1" ]; then \
		perl -e 'alarm shift; exec @ARGV' $${SMOKE_TIMEOUT:-240} cargo run --manifest-path dev-tools/Cargo.toml --bin advanced-show-control-smoke; \
	else \
		perl -e 'alarm shift; exec @ARGV' $${SMOKE_TIMEOUT:-240} cargo run --manifest-path dev-tools/Cargo.toml --bin advanced-show-control-smoke >/dev/null 2>&1; \
	fi
