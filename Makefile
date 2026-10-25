.PHONY: viewer build install test

# The viewer first (viewer/dist/index.html), then the binary that embeds it.
viewer:
	cd viewer && bun install --frozen-lockfile && bun run build

build: viewer
	cargo build --release

install: viewer
	cargo install --path crates/ig --locked

test: viewer
	cd viewer && bun run test
	cargo test --workspace
