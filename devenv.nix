{ pkgs, lib, config, ... }:

{
  # Enable languages
  languages.ruby = {
    enable = true;
    version = "3.4.7";
  };

  languages.rust = {
    enable = true;
    channel = "stable";
  };

  # System packages needed for Ruby gem development
  packages = with pkgs; [
    openssl
    # Ruby development dependencies
    ruby
    rubyPackages.rake

    # Build tools
    gcc
    gnumake
    pkg-config

    # For the C extension
    ruby.devEnv

    # Rust tooling
    cargo
    rustc
    rustfmt
    clippy

    # libclang is required by rb-sys/bindgen to generate the Ruby C bindings.
    # Only the *library* is needed — bindgen dlopens libclang.so via
    # LIBCLANG_PATH below and never shells out to the clang driver.
    #
    # The `clang` driver package is deliberately NOT listed. It is the unwrapped
    # compiler, so it has none of the crt/library paths the nix cc-wrapper
    # injects and cannot link a shared object at all ("cannot find crti.o").
    # Putting it on PATH shadowed gcc as the default CC and broke `rake compile`.
    libclang

    # Testing and development
    git
  ];

  # Environment variables
  env = {
    # rb-sys/bindgen needs to locate libclang.so to build the native extension
    LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";

    # bindgen drives libclang directly rather than through the nix cc-wrapper,
    # so it does not inherit the wrapper's -isystem flags and cannot find the
    # libc headers on its own — Ruby's defines.h includes <stdio.h> and the
    # build dies with "'stdio.h' file not found". Point it at glibc's headers
    # and the compiler's own resource dir explicitly.
    BINDGEN_EXTRA_CLANG_ARGS =
      "-isystem ${pkgs.glibc.dev}/include " +
      "-isystem ${pkgs.stdenv.cc.cc}/lib/gcc/${pkgs.stdenv.hostPlatform.config}/${pkgs.stdenv.cc.cc.version}/include";

    # Link with gcc. The nix cc-wrapper supplies the crt objects and library
    # paths that a bare compiler driver lacks; without this the cdylib fails to
    # link. See the note on the clang driver in `packages` above.
    CC = "${pkgs.stdenv.cc}/bin/cc";

    # Help the build find libraries
    LIBRARY_PATH = lib.makeLibraryPath [
      pkgs.ruby
    ];

    LD_LIBRARY_PATH = lib.makeLibraryPath [
      pkgs.ruby
      pkgs.stdenv.cc.cc.lib
    ];
  };

  # Shell initialization
  enterShell = ''
    echo "Ruby gem development environment"
    echo "Ruby version: $(ruby --version)"
    echo "Cargo version: $(cargo --version)"
    echo "Rake version: $(rake --version)"
    echo ""
    echo "Available commands:"
    echo "  rake compile  - Compile the extension"
    echo "  rake test     - Run all tests"
    echo "  rake clean    - Clean build artifacts"
    echo "  gem build amoskeag-rb.gemspec - Build the gem"
  '';

  # Pre-commit hooks for code quality
  pre-commit.hooks = {
    rustfmt.enable = true;
    clippy.enable = true;
  };
}
