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

# Can the linker driver actually build a shared object with -static-libgcc?
#
# The flag is a GCC one. Clang parses it but does not implement it, and when it
# is combined with the -nodefaultlibs that rustc passes, some toolchains (nix's
# clang wrapper among them) then fail to locate crti.o/crtbeginS.o and libgcc.
# Assuming "not darwin" implies "gcc" breaks the build outright on those hosts,
# so probe the real driver with a throwaway link instead of guessing.
def static_libgcc_supported?
  cc = RbConfig::CONFIG['CC'] || 'cc'

  require 'tmpdir'
  Dir.mktmpdir do |dir|
    src = File.join(dir, 'probe.c')
    obj = File.join(dir, 'probe.so')
    File.write(src, "int probe(void) { return 0; }\n")

    # -Werror so a mere "argument unused" warning counts as unsupported.
    # system returns nil (not false) when the command cannot be spawned at all,
    # so coerce to a real boolean.
    system(
      "#{cc} -shared -fPIC -Werror -static-libgcc #{src} -o #{obj}",
      out: File::NULL, err: File::NULL
    ) || false
  end
rescue StandardError
  # If the probe itself cannot run, fall back to not passing the flag: a binary
  # that links dynamically is worse than a binary that does not link at all.
  false
end

create_rust_makefile('amoskeag_native') do |r|
  # Build the release profile defined in Cargo.toml (lto, opt-level 3).
  r.profile = ENV.fetch('AMOSKEAG_PROFILE', 'release').to_sym

  # Statically link the C runtime on Linux so the resulting binary doesn't
  # depend on the host's exact libgcc — same portability goal as the rapidyaml
  # gem. Darwin links these statically by default; skip there. Elsewhere only
  # pass it if the linker driver really supports it (see above).
  if RbConfig::CONFIG['host_os'] !~ /darwin/ && static_libgcc_supported?
    r.extra_rustflags = ['-C', 'link-args=-static-libgcc']
  end
end
