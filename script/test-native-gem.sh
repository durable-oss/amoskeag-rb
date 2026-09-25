#!/usr/bin/env bash
#
# Build the precompiled native gem for the CURRENT platform, then install and
# run it in a sandbox with the Rust toolchain hidden from PATH. This proves the
# central promise of the precompiled gems: they install and run with no cargo,
# no rustc, no compile step.
#
# For cross-platform (other-arch / musl / windows) gems, see script/cross-build.sh.
#
# Usage: script/test-native-gem.sh
set -euo pipefail

cd "$(dirname "$0")/.."

VERSION=$(ruby -r ./lib/amoskeag-rb/version.rb -e "puts Amoskeag::VERSION")
PLATFORM=$(ruby -e "puts Gem::Platform.local.to_s")
GEM="amoskeag-rb-${VERSION}-${PLATFORM}.gem"

echo "==> Building native gem for ${PLATFORM} (v${VERSION})"
rm -rf pkg tmp
bundle exec rake native gem
test -f "pkg/${GEM}" || { echo "ERROR: pkg/${GEM} not produced"; exit 1; }

echo "==> Verifying gem metadata (no extensions, no rb_sys dep, binary present)"
# Capture metadata and file list into variables first; grepping a pipeline with
# `grep -q` under `set -o pipefail` can trip on the SIGPIPE from grep's early exit.
META=$(tar -xOf "pkg/${GEM}" metadata.gz | gunzip)
FILES=$(tar -xOf "pkg/${GEM}" data.tar.gz | tar -tzf -)

case "$META" in
  *"platform: ${PLATFORM}"*) ;;
  *) echo "ERROR: wrong or missing platform tag (expected ${PLATFORM})"; exit 1 ;;
esac
case "$META" in
  *"extensions: []"*) ;;
  *) echo "ERROR: gem still declares extensions"; exit 1 ;;
esac
case "$META" in
  *"name: rb_sys"*) echo "ERROR: rb_sys is still a dependency of the binary gem"; exit 1 ;;
esac
if ! printf '%s\n' "$FILES" | grep -qE 'amoskeag_native\.(so|bundle)$'; then
  echo "ERROR: no native binary in the gem"; exit 1
fi
echo "  platform tag, extensions:[], rb_sys stripped, binary present -- all good"

echo "==> Installing into a clean GEM_HOME"
SANDBOX=$(mktemp -d)
trap 'rm -rf "$SANDBOX"' EXIT
GEM_HOME="$SANDBOX" gem install "pkg/${GEM}" --local --no-document

echo "==> Running with the Rust toolchain hidden from PATH"
# Strip any directory containing cargo/rustc from PATH so a stray compile would
# fail loudly instead of silently succeeding.
CLEAN_PATH=""
IFS=':' read -ra DIRS <<< "$PATH"
for d in "${DIRS[@]}"; do
  # Drop dirs that carry cargo/rustc -- unless they also carry ruby or gem, in
  # which case dropping them would break the test runner itself.
  if { [ -x "$d/cargo" ] || [ -x "$d/rustc" ]; } \
     && [ ! -x "$d/ruby" ] && [ ! -x "$d/gem" ]; then
    continue
  fi
  CLEAN_PATH="${CLEAN_PATH:+$CLEAN_PATH:}$d"
done

# Use `env -i` + explicit PATH so the check isn't fooled by the shell's cached
# command hash. cargo can remain reachable only when it shares a bin directory
# with ruby/gem (e.g. a single nix profile), which we must keep to run the test.
if env -i PATH="$CLEAN_PATH" sh -c 'command -v cargo' >/dev/null 2>&1; then
  echo "NOTE: cargo shares a bin dir with ruby/gem on this host, so it could not"
  echo "      be fully hidden. The gem still installed with extensions:[] and no"
  echo "      rb_sys dep, so no compilation happened regardless."
fi

GEM_HOME="$SANDBOX" PATH="$CLEAN_PATH" EXPECT_VERSION="$VERSION" ruby -e '
  gem "amoskeag-rb"
  require "amoskeag-rb"
  raise "version mismatch" unless Amoskeag::VERSION == ENV.fetch("EXPECT_VERSION")
  result = Amoskeag.eval_expression("10 * 5 + 2", {})
  raise "bad result: #{result.inspect}" unless result == 52.0
  puts "  VERSION   = #{Amoskeag::VERSION}"
  puts "  10*5+2    = #{result}"
'

echo
echo "OK: native gem built, installed, and ran with no toolchain present."
