# frozen_string_literal: true

# Cross-compilation tasks for building precompiled platform gems locally.
#
# Two paths, depending on the target:
#
#   * Linux / Windows  -> `rake gem:<platform>` shells into the rb-sys-dock
#     docker image for that platform (carries the cross-toolchain + every
#     supported Ruby ABI) and runs `native:<platform> gem` inside it. Requires
#     Docker and the `rb-sys-dock` executable from the rb_sys gem.
#
#   * macOS            -> `rake gem:<platform>` builds the darwin gem directly
#     on the host (rb-sys-dock has no darwin images). Run these on a Mac.
#
# In CI we instead use the oxidize-rb/actions/cross-gem action, which wraps the
# same rb-sys-dock images; these tasks exist for local/manual builds.
#
# CROSS_PLATFORMS / CROSS_RUBIES come from misc/native.yml via the Rakefile.

DARWIN_PLATFORMS = CROSS_PLATFORMS.grep(/darwin/).freeze
DOCKED_PLATFORMS = (CROSS_PLATFORMS - DARWIN_PLATFORMS).freeze

namespace :gem do
  DOCKED_PLATFORMS.each do |platform|
    desc "Build precompiled gem for #{platform} (via rb-sys-dock + Docker)"
    task platform do
      ruby_versions = CROSS_RUBIES.join(",")
      sh "RUBY_CC_VERSION=#{ruby_versions.tr(',', ':')} " \
         "rb-sys-dock --platform #{platform} --ruby-versions #{ruby_versions} --build"
    end
  end

  DARWIN_PLATFORMS.each do |platform|
    desc "Build precompiled gem for #{platform} (run on a macOS host)"
    task platform do
      sh "bundle exec rake native:#{platform} gem"
    end
  end

  desc "Build precompiled gems for every platform in misc/native.yml"
  multitask cross: CROSS_PLATFORMS.map { |p| "gem:#{p}" }

  desc "Build precompiled gems for all Linux platforms"
  multitask linux: CROSS_PLATFORMS.grep(/linux/).map { |p| "gem:#{p}" }

  desc "Build precompiled gems for all Windows platforms"
  multitask windows: CROSS_PLATFORMS.grep(/mingw|mswin/).map { |p| "gem:#{p}" }

  desc "Build precompiled gems for all macOS platforms"
  multitask darwin: DARWIN_PLATFORMS.map { |p| "gem:#{p}" }
end
