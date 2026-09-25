#!/usr/bin/env bash
#
# Cross-compile precompiled gems for non-native platforms using rb-sys-dock
# (Docker). With no argument it builds every Linux/Windows platform from
# misc/native.yml; pass one or more platform tuples to build a subset.
#
# Darwin gems cannot be cross-built here -- run script/test-native-gem.sh on a
# Mac instead. This script skips any darwin platform it is asked for.
#
# Requires Docker and the rb-sys-dock executable (from the rb_sys gem).
#
# Usage:
#   script/cross-build.sh                       # all docked (linux/windows) platforms
#   script/cross-build.sh x86_64-linux          # one platform
#   script/cross-build.sh aarch64-linux x64-mingw-ucrt
set -euo pipefail

cd "$(dirname "$0")/.."

if ! command -v docker >/dev/null 2>&1; then
  echo "ERROR: docker is required for cross-compilation but was not found." >&2
  echo "Install Docker, or build native gems on each target OS instead." >&2
  exit 1
fi

if ! command -v rb-sys-dock >/dev/null 2>&1 && ! bundle exec rb-sys-dock --help >/dev/null 2>&1; then
  echo "ERROR: rb-sys-dock not found. Run 'bundle install' (the rb_sys gem ships it)." >&2
  exit 1
fi

# Resolve the platform list. Explicit args win; otherwise read every non-darwin
# platform from misc/native.yml.
if [ "$#" -gt 0 ]; then
  PLATFORMS=("$@")
else
  mapfile -t PLATFORMS < <(
    ruby -ryaml -e '
      YAML.load_file("misc/native.yml")["platforms"]
        .reject { |p| p =~ /darwin/ }
        .each { |p| puts p }
    '
  )
fi

rm -rf pkg

for platform in "${PLATFORMS[@]}"; do
  if [[ "$platform" == *darwin* ]]; then
    echo "==> Skipping ${platform} (build natively on macOS, not via Docker)"
    continue
  fi
  echo "==> Cross-building ${platform}"
  bundle exec rake "gem:${platform}"
done

echo
echo "Built gems:"
ls -1 pkg/*.gem 2>/dev/null || echo "  (none)"
