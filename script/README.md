# Build & release scripts

Helper scripts for building and verifying the gem locally. All are run from the
repo root (they `cd` there themselves) and assume the devenv shell is active so
`cargo`, `libclang`, and the bundled gems are available.

| Script | What it does |
|--------|--------------|
| `test-build.sh` | Compile the extension in place and run the test suite. The inner dev loop — no gem packaging. |
| `test-source-gem.sh` | Build the **source** gem, install it into a throwaway `GEM_HOME` (forcing a real compile from the packaged files), and smoke-test it. Needs cargo + libclang + network (to pull `rb_sys`). |
| `test-native-gem.sh` | Build the **precompiled native** gem for the current platform, then install and run it with the Rust toolchain hidden from `PATH` — proving it needs no compiler. |
| `test-all.sh` | Runs the three above in sequence. |
| `cross-build.sh` | Cross-compile gems for other platforms via rb-sys-dock (Docker). No args = all Linux/Windows targets from `misc/native.yml`; pass tuples for a subset. Darwin must be built on a Mac. |
| `release.sh` | Build and push the gem to RubyGems with an OTP prompt. |

## Typical use

```sh
# Fast feedback while developing
script/test-build.sh

# Before tagging a release, verify both install paths end to end
script/test-all.sh

# Build the other platforms' binaries locally (needs Docker)
script/cross-build.sh
```

CI runs the equivalent of these on every push; see `.github/workflows/gem.yml`.
