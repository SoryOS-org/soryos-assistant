# Whisper Release Build Optimisation Patch

## Context
- Source: [Codeberg issue #226 “x10 slower in release build”](https://codeberg.org/tazz4843/whisper-rs/issues/226).
- Symptom: `whisper-rs` release builds on Windows decode ~4 s slower than dev builds despite identical configs.
- Diagnosis: `whisper.cpp`’s CMake step sometimes misses optimisation flags when invoked via Cargo.

## Local Patch
We vendor the upstream crate and add explicit optimisation flags to the native build script.

```text
third_party/whisper-rs-sys-0.14.1/build.rs
```

```rust
    } else {
        config.define("CMAKE_BUILD_TYPE", "Release");
        config.cxxflag("-O2");
        config.cxxflag("-DNDEBUG");
    }
```

We also patch the workspace so Cargo resolves to the vendored copy:

```toml
# Cargo.toml (workspace root)
[patch.crates-io]
whisper-rs = { path = "third_party/whisper-rs-0.15.1" }
whisper-rs-sys = { path = "third_party/whisper-rs-sys-0.14.1" }
```

### Why
- Forces C++ compiler to use `-O2`/`-DNDEBUG` even when CMake defaults are inconsistent.
- Allows us to ship the Release binary without regressing STT latency.

### Trade-offs
- We carry two vendored crates; remember to reapply the patch when bumping `whisper-rs`.

## Rebuild Steps
1. Ensure `third_party/whisper-rs-0.15.1` and `third_party/whisper-rs-sys-0.14.1` exist (copy from `$CARGO_HOME/registry/src/*` if needed).
2. Whenever you refresh from crates.io, reapply the `-O2`/`-DNDEBUG` addition in `whisper-rs-sys/build.rs`.
3. Build normally (`pnpm build:tauri`); Cargo will use the patched crates via `[patch.crates-io]`.

## Release Notes
- Applies C++ optimisation workaround from Codeberg issue #226 locally until upstream integrates an official fix.
