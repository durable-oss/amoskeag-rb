# Changelog

All notable changes to the amoskeag-rb gem will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-06-19

### Added
- Precompiled, statically-linked native gems for every supported platform, so
  `gem install amoskeag-rb` downloads a ready-to-run binary — no Rust toolchain,
  no `cargo`, no compile step. Targets: `x86_64-linux`, `aarch64-linux`,
  `x86_64-linux-musl`, `aarch64-linux-musl`, `x64-mingw-ucrt`, `arm64-darwin`,
  and `x86_64-darwin`, each built against Ruby 3.1–3.4.
- `misc/native.yml` as the single source of truth for build platforms + ABIs.
- `rakelib/cross.rake` with `rake gem:<platform>` / `gem:cross` tasks (rb-sys-dock)
  for local cross-builds.
- GitHub Actions workflow (`.github/workflows/gem.yml`) that tests, builds every
  platform gem plus a source fallback, and publishes to RubyGems on tag via
  Trusted Publishing (OIDC).

### Changed
- Build switched from `Rake::ExtensionTask` to `RbSys::ExtensionTask`; the Rust
  crate is now a Cargo workspace member (root `Cargo.toml`) so cross-compilation
  and `cargo metadata` resolve cleanly.
- The native binary loads from a per-ABI path (`lib/amoskeag-rb/<ruby>/`), with a
  fallback for source-gem installs.
- The source gem links libstdc++/libgcc statically on Linux for portability.

### Fixed
- The extension loader no longer relies on a development-only relative path that
  failed for installed gems.

## [0.1.0] - 2025-01-20

### Added
- Initial release of amoskeag-rb gem
- Native Ruby bindings to Amoskeag DSL
- FFI layer using statically compiled Rust library
- Core API methods:
  - `Amoskeag.compile(source, symbols)` - Compile programs with symbol validation
  - `Amoskeag.evaluate(program, data)` - Evaluate compiled programs
  - `Amoskeag.eval_expression(source, data, symbols)` - Compile and evaluate in one step
- Full support for all Amoskeag data types:
  - Numbers, Strings, Booleans, Nil
  - Arrays and Dictionaries
  - Symbols with compile-time validation
- Ruby-friendly API with automatic type conversion
- Thread-safe compiled programs
- Comprehensive error handling with CompileError and EvalError
- Complete documentation and examples
- JSON-based marshalling between Ruby and native code

### Features
- Business Rules Engine capabilities
- Template Engine functionality
- Spreadsheet Formula support
- 60+ standard library functions
- Excel-compatible financial functions
- Safe navigation (nil-safe property access)
- Pure functional evaluation (no side effects)
- Security-first design (immune to code injection)

[0.2.0]: https://github.com/durable-oss/amoskeag-rb/releases/tag/v0.2.0
[0.1.0]: https://github.com/durable-oss/amoskeag-rb/releases/tag/v0.1.0
