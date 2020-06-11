.PHONY: help
help: # Shows available `make` commands
	@echo 'Available `make` commands:' >/dev/stderr
	@echo >/dev/stderr
	@awk -F'#' '/^[a-z][A-Za-z0-9]+/ {if (NF > 1) { sub(/:[^#]*/, ""); print $$1 "\t\t" $$2}}' Makefile

.PHONY: watch
watch: # Continuously runs `cargo check` and `cargo test` on file changes
	cargo watch -x check -x "test --all-features" --clear

.PHONY: test
test: # Runs all tests
	cargo test --all-features

.PHONY: fuzz
fuzz: # Runs fuzzing tests
	cargo +nightly fuzz run parse_command

.PHONY: run
run: debug # Runs the `basic` example in verbose mode
	RUST_LOG=INFO ./target/debug/examples/basic

.PHONY: docs
docs: # Creates the API docs and opens it in the browser
	cargo doc --all-features --no-deps --open

.PHONY: debug
debug: # Creates a debug build
	cargo build --example basic

.PHONY: examples
examples:
	cargo build --examples

.PHONY: build
build: # Creates a release build
	cargo build --release

.PHONY: pr-prep
pr-prep: # Runs checks to ensure you're ready for a pull request
	cargo fmt --all -- --check
	cargo clippy --all-features --all-targets
	cargo test --all-features --all-targets
	cargo test --doc
	cargo build --all-features --all-targets
	cargo doc --crate-version --all-features --no-deps

.PHONY: publish
publish: # Publishes the lib to crates.io
	cargo publish --all-features --verbose
