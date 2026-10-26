.PHONY: viewer build install test app app-dev

# The viewer first (viewer/dist/index.html), then the binary that embeds it.
viewer:
	cd viewer && bun install --frozen-lockfile && bun run build

build: viewer
	cargo build --release -p intent-graph

install: viewer
	cargo install --path crates/ig --locked

test: viewer
	cd viewer && bun run test
	cargo test --workspace

# The desktop app (crates/ig-desktop), with the Tauri CLI from the viewer's devDependencies.
# The CLI checks frontendDist (crates/ig-desktop/dist) before cargo's build.rs fills it.
TAURI = mkdir -p crates/ig-desktop/dist && cp viewer/dist/index.html crates/ig-desktop/dist/ && cd crates/ig-desktop && ../../viewer/node_modules/.bin/tauri

app-dev: viewer
	$(TAURI) dev

# A local build, unsigned: target/release/bundle/macos/ig.app on macOS.
app: viewer
	$(TAURI) build
