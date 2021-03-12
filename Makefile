CRATES_DIR = ./crates
SUB_CRATES = $(sort $(dir $(wildcard $(CRATES_DIR)/*/)))

.PHONY: help
help: # Shows available `make` commands
	@echo 'Available `make` commands:' >/dev/stderr
	@echo >/dev/stderr
	@awk -F'#' '/^[a-z][A-Za-z0-9]+/ {if (NF > 1) { sub(/:[^#]*/, ""); print $$1 "\t\t" $$2}}' Makefile

.PHONY: test
test: # Runs unit and integration tests
test: $(SUB_CRATES)
	cargo test --all-features
	$(foreach sub, $^, (cd $(sub); echo "---\nIn $(sub)\n---"; cargo test);)

.PHONY: docs
docs: # Creates the API docs and opens it in the browser
	cargo doc --all-features --no-deps --open

.PHONY: examples
examples:
	cargo build --examples

.PHONY: build
build: # Creates a release build
	cargo build --release

.PHONY: pr-prep
pr-prep: # Runs checks to ensure you're ready for a pull request
	cargo fmt --all -- --check
	cargo build --examples
	cargo build --all-features --all-targets
	cargo clippy --all-features --all-targets
	cargo test --all-features --all-targets
	cargo test --doc
	cargo doc --all-features --no-deps

.PHONY: publish
publish: # Publishes the lib to crates.io
	cargo publish --all-features --verbose
