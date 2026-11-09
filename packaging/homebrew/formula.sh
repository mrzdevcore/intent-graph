#!/bin/sh
# Prints Formula/ig.rb for the tap: formula.sh <version> <dir holding the release's .sha256 files>
set -eu
version=$1
dir=$2
base="https://github.com/mrzdevcore/intent-graph/releases/download/v$version"
sum() { cut -d' ' -f1 "$dir/ig-$version-$1.tar.gz.sha256"; }

cat <<EOF
class Ig < Formula
  desc "Architecture map of a codebase, checked against the architecture you intend"
  homepage "https://github.com/mrzdevcore/intent-graph"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "$base/ig-$version-aarch64-apple-darwin.tar.gz"
      sha256 "$(sum aarch64-apple-darwin)"
    end
    on_intel do
      url "$base/ig-$version-x86_64-apple-darwin.tar.gz"
      sha256 "$(sum x86_64-apple-darwin)"
    end
  end

  on_linux do
    on_arm do
      url "$base/ig-$version-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "$(sum aarch64-unknown-linux-gnu)"
    end
    on_intel do
      url "$base/ig-$version-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "$(sum x86_64-unknown-linux-gnu)"
    end
  end

  def install
    bin.install "ig"
    man5.install "ig-map.5" if File.exist?("ig-map.5")
    generate_completions_from_executable(bin/"ig", "completions")
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/ig --version")
  end
end
EOF
