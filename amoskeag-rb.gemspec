require_relative 'lib/amoskeag-rb/version'

Gem::Specification.new do |spec|
  spec.name          = "amoskeag-rb"
  spec.version       = Amoskeag::VERSION
  spec.authors       = ["Durable Programming"]
  spec.email         = ["djberube@durableprogramming.com"]

  spec.summary       = "Ruby bindings for Amoskeag - a secure, functional DSL for business rules"
  spec.description   = <<~DESC
    Amoskeag is a purely functional, statically-validated Domain-Specific Language (DSL)
    designed for high-security, sandboxed evaluation. It's perfect for:

    - Business Rules Engines (insurance underwriting, loan approval)
    - Template Engines (secure alternative to ERB with more power than Liquid)
    - Spreadsheet Formula Engines (Excel-like calculations)

    This gem provides native Ruby bindings to the Amoskeag library, compiled from Rust
    for maximum performance and security.
  DESC

  spec.homepage      = "https://github.com/durable-oss/amoskeag-rb"
  spec.license       = "MIT"

  # Must match the ABI versions in misc/native.yml that we build precompiled
  # gems for. rb-sys / rake-compiler builds a fat binary per Ruby minor version.
  spec.required_ruby_version = ">= 3.1.0"

  spec.metadata["homepage_uri"] = spec.homepage
  spec.metadata["source_code_uri"] = spec.homepage
  spec.metadata["changelog_uri"] = "#{spec.homepage}/blob/main/CHANGELOG.md"

  # Files shipped in the gem. We deliberately exclude compiled artifacts
  # (*.so, *.bundle, target/, Cargo.lock build state) so the source gem stays
  # clean; precompiled platform gems add their own binary via the cross task.
  spec.files = Dir[
    "lib/**/*.rb",
    "ext/**/*.{rs,rb,toml}",
    "Cargo.toml",       # workspace root: carries the release profile + member list
    "Cargo.lock",       # workspace lock: the resolved revisions cargo builds from
    "README.md",
    "LICENSE",
    "CHANGELOG.md"
  ].select { |f| File.file?(f) }

  spec.require_paths = ["lib"]
  spec.extensions    = ["ext/amoskeag/extconf.rb"]

  # Runtime dependencies.
  spec.add_runtime_dependency "json", "~> 2.0"

  # rb_sys is needed to compile the source gem (ext/amoskeag/extconf.rb requires
  # rb_sys/mkmf). When building a precompiled platform gem, RbSys::ExtensionTask
  # strips both spec.extensions and this dependency, so binary-gem users never
  # pull rb_sys or invoke cargo.
  spec.add_runtime_dependency "rb_sys", "~> 0.9.91"

  # Development / build dependencies.
  spec.add_development_dependency "rake", "~> 13.0"
  spec.add_development_dependency "rake-compiler", "~> 1.2"
  spec.add_development_dependency "minitest", "~> 5.0"
  spec.add_development_dependency "yard", "~> 0.9"

  # Post-install message
  spec.post_install_message = <<~MSG

    Thank you for installing amoskeag-rb!

    Amoskeag is a secure, purely functional DSL for business rules, templates,
    and spreadsheet formulas. It's designed to be immune to code injection
    attacks while providing powerful expression evaluation capabilities.

    Get started:
      require 'amoskeag-rb'
      result = Amoskeag.eval_expression("2 + 2", {})  # => 4.0

    Documentation: https://github.com/durable-oss/amoskeag-rb

  MSG
end
