.PHONY: viewer build test

# The viewer first (viewer/dist/index.html), then the binary that embeds it.
viewer:
	cd viewer && bun install --frozen-lockfile && bun run build

build: viewer
	cargo build --release

test:
	cd viewer && bun run test
	cargo test
