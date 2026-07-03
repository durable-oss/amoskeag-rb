# frozen_string_literal: true

require "bundler/gem_tasks"
require "rake/testtask"
require "rb_sys/extensiontask"
require "yaml"

GEMSPEC = Gem::Specification.load("amoskeag-rb.gemspec")

native_config   = YAML.load_file(File.expand_path("misc/native.yml", __dir__))
CROSS_RUBIES    = native_config.fetch("rubies").freeze
CROSS_PLATFORMS = native_config.fetch("platforms").freeze

# RbSys::ExtensionTask is the Rust-aware drop-in for Rake::ExtensionTask. It
# builds the cdylib via cargo, installs the artifact under lib/amoskeag-rb/<abi>/
# so each Ruby version loads its own binary, and — for `gem:<platform>` tasks —
# produces a precompiled fat gem (stripping the rb_sys runtime dep + extensions).
RbSys::ExtensionTask.new("amoskeag_native", GEMSPEC) do |ext|
  ext.lib_dir        = "lib/amoskeag-rb"
  ext.ext_dir        = "ext/amoskeag"
  ext.cross_compile  = true
  ext.cross_platform = CROSS_PLATFORMS
end

Rake::TestTask.new(:test) do |t|
  t.libs << "test"
  t.libs << "lib"
  t.test_files = FileList["test/**/*_test.rb"]
end

task default: %i[compile test]

desc "Remove all build artifacts"
task :clean_all do
  sh "cd ext/amoskeag && cargo clean" if File.exist?("ext/amoskeag/Cargo.toml")
  rm_rf Dir["lib/amoskeag-rb/*/amoskeag_native.{so,bundle}"]
  rm_rf %w[lib/amoskeag_native.so lib/amoskeag_native.bundle tmp pkg]
end

# Files under rakelib/*.rake are auto-loaded by Rake; the cross-compile tasks
# (gem:<platform>, gem:cross, etc.) live in rakelib/cross.rake.
