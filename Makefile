.PHONY: viewer viewer-app build install test app app-dev app-install

# The viewer first (viewer/dist/index.html), then the binary that embeds it.
viewer:
	cd viewer && pnpm install --frozen-lockfile --ignore-scripts && pnpm run build

build: viewer
	cargo build --release -p intent-graph

install: viewer
	cargo install --path crates/ig --locked

test: viewer
	cd viewer && pnpm run test
	cargo test --workspace

# The app's frontend: viewer/dist-app/, multi-file.
viewer-app:
	cd viewer && pnpm install --frozen-lockfile --ignore-scripts && pnpm run build:app

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

# Everything on this Mac: the app in /Applications (replacing the old one) and `ig` on the PATH,
# so `ig .` opens the app on a folder.
app-install: app install
	rm -rf /Applications/ig.app
	cp -R "$${CARGO_TARGET_DIR:-target}/release/bundle/macos/ig.app" /Applications/
