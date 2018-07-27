build:
	cargo build --release

test:
	cargo test

watch:
	cargo watch -x check -x test -x doc --clear
