.PHONY: help
help: # Shows available `make` commands
	@echo 'Available `make` commands:' >/dev/stderr
	@echo >/dev/stderr
	@awk -F'#' '/^[a-z][A-Za-z0-9]+/ {if (NF > 1) { sub(/:[^#]*/, ""); printf("%-25s %s\n", $$1, $$2)}}' Makefile

.PHONY: docs
docs: # Creates the API docs and opens it in the browser
	cargo doc --no-deps --open

.PHONY: pr-prep
pr-prep: # Runs checks to ensure you're ready for a pull request
	cargo fmt --all -- --check
	cargo clippy
	cargo test
	cargo test --doc
	cargo build
	cargo build --examples
	cargo doc --no-deps

.PHONY: publish
publish: # Publishes the lib to crates.io
	cargo publish --verbose

.PHONY: key-generator-image
key-generator-image: # Generate a Docker image for the unftp key generator script (files/run.sh)
	docker build -f key-generator.Dockerfile -t bolcom/unftp-key-generator:latest .

