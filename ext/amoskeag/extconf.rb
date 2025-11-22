require 'mkmf'
require 'rb_sys/mkmf'

# Check for cargo
unless system("which cargo > /dev/null 2>&1")
  abort "Cargo (Rust build tool) not found in PATH. Please install Rust from https://rustup.rs/"
end

# Create the Makefile using rb_sys
create_rust_makefile("amoskeag_native")
