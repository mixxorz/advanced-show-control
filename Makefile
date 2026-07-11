.PHONY: help fmt lint test build check \
	rust-fmt rust-lint rust-test rust-build \
	ui-fmt ui-lint ui-typecheck ui-build ui-test ui-storybook-test \
	visual-test visual-update docs-install docs-build docs-serve dev storybook probe smoke

DOCS_VENV := .venv-docs
DOCS_PYTHON := $(DOCS_VENV)/bin/python
DOCS_ZENSICAL := $(DOCS_VENV)/bin/zensical

help:
	@printf '%s\n' \
	  'Common targets:' \
	  '  make fmt                  Check Rust and UI formatting' \
	  '  make lint                 Run Rust clippy and UI ESLint' \
	  '  make test                 Run Rust and UI unit tests' \
	  '  make build                Build Rust workspace and UI' \
	  '  make check                Run CI-like non-visual checks' \
	  '' \
	  'Documentation targets:' \
	  '  make docs-install         Install pinned documentation dependencies' \
	  '  make docs-build           Build documentation site with strict validation' \
	  '  make docs-serve           Serve documentation site locally' \
	  '' \
	  'Rust targets:' \
	  '  make rust-fmt             cargo fmt --all -- --check' \
	  '  make rust-lint            cargo clippy --workspace --all-targets -- -D warnings' \
	  '  make rust-test            cargo nextest run --workspace' \
	  '  make rust-build           cargo build --workspace' \
	  '' \
	  'UI targets:' \
	  '  make ui-fmt               npm run format:check' \
	  '  make ui-lint              npm run lint' \
	  '  make ui-typecheck         npm run typecheck' \
	  '  make ui-build             npm run build' \
	  '  make ui-test              npm run test' \
	  '  make ui-storybook-test    npm run test:storybook' \
	  '' \
	  'Visual targets:' \
	  '  make visual-test          npm run test:visual:ci' \
	  '  make visual-update        npm run test:visual:update:ci' \
	  '' \
	  'Development targets:' \
	  '  make dev                  Start Tauri dev server and app' \
	  '  make storybook            Start Storybook dev server' \
	  '  make probe                Run LV1 probe CLI (pass ARGS="...")' \
	  '  make smoke                Run debug Tauri hardware smoke app quietly' \
	  '  make smoke VERBOSE=1      Run debug smoke with terminal logs'

fmt: rust-fmt ui-fmt

lint: rust-lint ui-lint

test: rust-test ui-test

build: rust-build ui-build

check: fmt lint ui-typecheck build test ui-storybook-test

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

ui-fmt:
	npm --prefix ui run format:check

ui-lint:
	npm --prefix ui run lint

ui-typecheck:
	npm --prefix ui run typecheck

ui-build:
	npm --prefix ui run build

ui-test:
	npm --prefix ui run test

ui-storybook-test:
	npm --prefix ui run test:storybook

visual-test:
	npm --prefix ui run test:visual:ci

visual-update:
	npm --prefix ui run test:visual:update:ci

dev:
	npm run tauri -- dev

storybook:
	npm --prefix ui run storybook

probe:
	cargo run --manifest-path src-tauri/dev-tools/Cargo.toml --bin lv1-probe -- $(ARGS)

smoke:
	@npm --prefix ui run build:debug
	@if [ "$(VERBOSE)" = "1" ]; then \
		perl -e 'alarm shift; exec @ARGV' $${SMOKE_TIMEOUT:-240} cargo run --manifest-path src-tauri/dev-tools/Cargo.toml --bin advanced-show-control-debug; \
	else \
		perl -e 'alarm shift; exec @ARGV' $${SMOKE_TIMEOUT:-240} cargo run --manifest-path src-tauri/dev-tools/Cargo.toml --bin advanced-show-control-debug >/dev/null 2>&1; \
	fi
