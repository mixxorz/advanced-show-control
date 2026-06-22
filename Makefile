.PHONY: help fmt lint test build check \
	rust-fmt rust-lint rust-test rust-build \
	ui-fmt ui-lint ui-typecheck ui-build ui-test ui-storybook-test \
	visual-test visual-update dev debug-build debug-smoke

help:
	@printf '%s\n' \
	  'Common targets:' \
	  '  make fmt                  Check Rust and UI formatting' \
	  '  make lint                 Run Rust clippy and UI ESLint' \
	  '  make test                 Run Rust and UI unit tests' \
	  '  make build                Build Rust workspace and UI' \
	  '  make check                Run CI-like non-visual checks' \
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
	  '' \
	  'Debug smoke targets:' \
	  '  make debug-build          npm run build:debug' \
	  '  make debug-smoke          Run debug Tauri hardware smoke app'

fmt: rust-fmt ui-fmt

lint: rust-lint ui-lint

test: rust-test ui-test

build: rust-build ui-build

check: fmt lint ui-typecheck build test ui-storybook-test

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

debug-build:
	npm --prefix ui run build:debug

debug-smoke:
	npm run tauri -- dev --config src-tauri/tauri.debug.conf.json -- --bin advanced-show-control-debug
