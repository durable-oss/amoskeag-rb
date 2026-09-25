#!/usr/bin/env bash
#
# Run the full local build verification: in-place compile + tests, then a
# source-gem round-trip, then a native-gem round-trip. Cross-platform builds
# (Docker) are NOT run here -- see script/cross-build.sh.
#
# Usage: script/test-all.sh
set -euo pipefail

cd "$(dirname "$0")/.."

echo "########## 1/3  build + test (in place) ##########"
script/test-build.sh

echo
echo "########## 2/3  source gem round-trip ##########"
script/test-source-gem.sh

echo
echo "########## 3/3  native gem round-trip ##########"
script/test-native-gem.sh

echo
echo "ALL LOCAL BUILD CHECKS PASSED."
