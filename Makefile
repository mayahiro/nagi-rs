.PHONY: bench build check format format-check lint test unicode

bench:
	cargo bench -p nagi-tui --bench scroll_viewport

build:
	cargo build --workspace --all-targets

test:
	cargo test --workspace

lint:
	cargo clippy --workspace --all-targets -- -D warnings

format:
	cargo fmt --all

format-check:
	cargo fmt --all -- --check

check:
	$(MAKE) format-check
	$(MAKE) build
	$(MAKE) test
	$(MAKE) lint

unicode:
	@test -n "$(UNICODE_DATA)" || { echo "UNICODE_DATA must name the Unicode 17.0.0 source directory" >&2; exit 2; }
	cargo run -p nagi-unicode-gen -- --data-dir "$(UNICODE_DATA)" --out crates/nagi-text/src/generated.rs
