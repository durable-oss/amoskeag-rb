#!/usr/bin/env bash
set -euo pipefail

# Get the gem version
VERSION=$(ruby -r ./lib/amoskeag-rb/version.rb -e "puts Amoskeag::VERSION")

echo "Building amoskeag-rb v${VERSION}..."

# Build the gem
gem build amoskeag-rb.gemspec

# Prompt for OTP
OTP=$(gum input --placeholder "Enter OTP code")

# Push to rubygems.org with OTP
echo "Pushing to rubygems.org..."
gem push "amoskeag-rb-${VERSION}.gem" --otp "${OTP}"

echo "Successfully released amoskeag-rb v${VERSION}!"
