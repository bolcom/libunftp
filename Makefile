.PHONY: build
build:
	cargo build --release

.PHONY: debug
debug:
	cargo build

.PHONY: run
run: debug
	RUST_LOG=INFO ./target/debug/examples/basic

.PHONY: test
test:
	cargo test

.PHONY: watch
watch:
	cargo watch -x check -x test -x doc --clear
