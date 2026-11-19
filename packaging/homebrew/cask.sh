#!/bin/sh
# Prints Casks/ig-app.rb for the tap: cask.sh <version> <dir holding the release's .sha256 files>
set -eu
version=$1
dir=$2
sum() { cut -d' ' -f1 "$dir/ig-app-$version-$1-apple-darwin.zip.sha256"; }

cat <<EOF
cask "ig-app" do
  arch arm: "aarch64", intel: "x86_64"

  version "$version"
  sha256 arm:   "$(sum aarch64)",
         intel: "$(sum x86_64)"

  url "https://github.com/mrzdevcore/intent-graph/releases/download/v#{version}/ig-app-#{version}-#{arch}-apple-darwin.zip"
  name "ig"
  desc "Desktop app for ig: the architecture map, editable, beside a Claude Code chat"
  homepage "https://github.com/mrzdevcore/intent-graph"

  app "ig.app"

  # The app is not signed: without this, Gatekeeper refuses to open it.
  postflight do
    system_command "/usr/bin/xattr", args: ["-dr", "com.apple.quarantine", "#{appdir}/ig.app"]
  end
end
EOF
