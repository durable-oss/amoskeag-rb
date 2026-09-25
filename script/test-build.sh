#!/usr/bin/env bash
#
# Compile the extension in place and run the test suite. This is the inner
# development loop — it does NOT build or install a gem, it just verifies the
# Rust extension compiles and the tests pass against the working tree.
#
# Usage: script/test-build.sh
set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> Cleaning previous build artifacts"
bundle exec rake clean_all >/dev/null 2>&1 || true

echo "==> Compiling extension"
bundle exec rake compile

echo "==> Running tests"
bundle exec rake test

echo
echo "OK: extension compiled and tests passed."
