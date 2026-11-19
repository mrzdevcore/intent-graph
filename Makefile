.PHONY: viewer viewer-app build install test app app-dev app-install

# The viewer first (viewer/dist/index.html), then the binary that embeds it.
viewer:
	cd viewer && pnpm install --frozen-lockfile --ignore-scripts && pnpm run build

build: viewer
	cargo build --release -p intent-graph

# man finds the page through PATH: <prefix>/bin on the PATH means <prefix>/share/man is searched.
CARGO_HOME ?= $(HOME)/.cargo

install: viewer
	cargo install --path crates/ig --locked
	install -d "$(CARGO_HOME)/share/man/man5"
	install -m 644 crates/ig/assets/ig-map.5 "$(CARGO_HOME)/share/man/man5/ig-map.5"

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

# ig is a sidecar so signing covers it; only the bundle config names it, as tauri-build needs it to exist.
APP_TRIPLE = $(or $(TARGET),$(shell rustc -vV | sed -n 's/^host: //p'))

app: viewer viewer-app
	cargo build --release -p intent-graph --target $(APP_TRIPLE)
	mkdir -p crates/ig-desktop/bin && cp "$${CARGO_TARGET_DIR:-target}/$(APP_TRIPLE)/release/ig" crates/ig-desktop/bin/ig-$(APP_TRIPLE)
	$(TAURI) build --target $(APP_TRIPLE) --config tauri.bundle.conf.json

# Everything on this Mac: the app in /Applications (replacing the old one) and `ig` on the PATH,
# so `ig .` opens the app on a folder.
app-install: app install
	rm -rf /Applications/ig.app
	cp -R "$${CARGO_TARGET_DIR:-target}/$(APP_TRIPLE)/release/bundle/macos/ig.app" /Applications/
