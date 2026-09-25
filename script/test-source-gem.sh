#!/usr/bin/env bash
#
# Build the *source* gem, install it into a throwaway GEM_HOME (which forces a
# real compile from the packaged files), and smoke-test it. This is what a user
# on a platform without a precompiled binary gets, so it catches packaging bugs
# the in-place `rake compile` can't: missing files, bad require paths, a source
# tree that won't build once extracted from the gem.
#
# Requires a working Rust toolchain (cargo) and libclang on the host.
#
# Usage: script/test-source-gem.sh
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$PWD"

VERSION=$(ruby -r ./lib/amoskeag-rb/version.rb -e "puts Amoskeag::VERSION")
GEM="amoskeag-rb-${VERSION}.gem"

echo "==> Building source gem (v${VERSION})"
rm -rf pkg
bundle exec rake build
test -f "pkg/${GEM}" || { echo "ERROR: pkg/${GEM} not produced"; exit 1; }

echo "==> Listing packaged files"
tar -xOf "pkg/${GEM}" data.tar.gz | tar -tzf - | sort

echo "==> Installing into a clean GEM_HOME (compiles from source)"
# NOT --local: the source gem depends on rb_sys at build time, which must be
# resolved from the gem source (rubygems.org) into the empty sandbox prefix.
SANDBOX=$(mktemp -d)
trap 'rm -rf "$SANDBOX"' EXIT
GEM_HOME="$SANDBOX" gem install "pkg/${GEM}" --no-document

echo "==> Smoke-testing the installed gem"
GEM_HOME="$SANDBOX" EXPECT_VERSION="$VERSION" ruby -e '
  gem "amoskeag-rb"
  require "amoskeag-rb"
  raise "version mismatch" unless Amoskeag::VERSION == ENV.fetch("EXPECT_VERSION")
  result = Amoskeag.eval_expression("2 + 2 * 10", {})
  raise "bad result: #{result.inspect}" unless result == 22.0
  puts "  VERSION  = #{Amoskeag::VERSION}"
  puts "  2+2*10   = #{result}"
'

echo
echo "OK: source gem built, installed, and ran from a clean prefix."
