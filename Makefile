.PHONY: viewer viewer-app board-wasm board-test build install test app app-dev

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

# The board engine (crates/ig-board) as wasm, for the desktop app only: viewer/src/board/pkg.
# wasm-bindgen-cli is installed pinned into target/tools; the rustup target is never installed here.
WASM_BINDGEN = target/tools/bin/wasm-bindgen
board-wasm:
	@rustup target list --installed | grep -qx wasm32-unknown-unknown || { echo "missing the wasm32 target: run rustup target add wasm32-unknown-unknown"; exit 1; }
	@test -x $(WASM_BINDGEN) || cargo install wasm-bindgen-cli --version =0.2.129 --locked --root target/tools
	cargo build -p ig-board --target wasm32-unknown-unknown --release
	cargo clippy -p ig-board --target wasm32-unknown-unknown -- -D warnings
	$(WASM_BINDGEN) --target web --out-dir viewer/src/board/pkg "$${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/ig_board.wasm"

# The wasm smoke test (skipped by `make test` when pkg/ is absent).
board-test: board-wasm
	cd viewer && bun run test -- wasm.smoke

# The app's frontend: viewer/dist-app/, multi-file, with the lazy board chunk and its .wasm.
viewer-app: board-wasm
	cd viewer && bun install --frozen-lockfile && bun run build:app

# The desktop app (crates/ig-desktop), with the Tauri CLI from the viewer's devDependencies.
# The CLI checks frontendDist (crates/ig-desktop/dist) before cargo's build.rs fills it from viewer/dist-app.
TAURI = rm -rf crates/ig-desktop/dist && mkdir -p crates/ig-desktop/dist && cp -R viewer/dist-app/. crates/ig-desktop/dist/ && cd crates/ig-desktop && ../../viewer/node_modules/.bin/tauri

# The app's agent runs the `ig` next to the app's binary (target/<profile>/ig, Contents/MacOS/ig).
app-dev: viewer viewer-app
	cargo build -p intent-graph
	$(TAURI) dev

# A local build, unsigned: target/release/bundle/macos/ig.app on macOS.
# The bundle takes ig from crates/ig-desktop/bin/ig (gitignored), wherever cargo's target dir is.
app: viewer viewer-app
	cargo build --release -p intent-graph
	mkdir -p crates/ig-desktop/bin && cp "$${CARGO_TARGET_DIR:-target}/release/ig" crates/ig-desktop/bin/ig
	$(TAURI) build
