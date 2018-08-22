.PHONY: watch
watch:
	cargo watch -x check -x test --clear

.PHONY: run
run: debug
	RUST_LOG=INFO ./target/debug/examples/basic

.PHONY: doc
doc:
	cargo doc --open

.PHONY: build
build:
	cargo build --release

.PHONY: debug
debug:
	cargo build

.PHONY: test
test:
	cargo test

clippy:
	cargo +nightly clippy
