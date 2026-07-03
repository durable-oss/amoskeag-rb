# frozen_string_literal: true

require 'mkmf'
require 'rb_sys/mkmf'

# This file only runs when building the *source* gem (on platforms without a
# precompiled binary). Precompiled platform gems ship the cdylib directly and
# never execute extconf, so cargo is not required to install them.
unless system('cargo --version > /dev/null 2>&1')
  abort 'Cargo (the Rust build tool) was not found in PATH. ' \
        'Install Rust from https://rustup.rs/ and re-run, or install a ' \
        'precompiled gem for your platform.'
end

create_rust_makefile('amoskeag_native') do |r|
  # Build the release profile defined in Cargo.toml (lto, opt-level 3).
  r.profile = ENV.fetch('AMOSKEAG_PROFILE', 'release').to_sym

  # Statically link the C/C++ runtime on Linux so the resulting binary doesn't
  # depend on the host's exact libstdc++/libgcc — same portability goal as the
  # rapidyaml gem. Darwin links these statically by default; skip there.
  unless RbConfig::CONFIG['host_os'] =~ /darwin/
    r.extra_rustflags = ['-C', 'link-args=-static-libgcc']
  end
end
