#!/bin/sh
set -eu
# apple-cf needs Apple's Swift, rather than the toolchain in ~/.swiftly.
PATH=$(printf '%s' "$PATH" | tr ':' '\n' | sed '/\.swiftly/d' | paste -sd ':' -)
export PATH
cargo build --release --manifest-path crates/xp4/Cargo.toml --bin xt4-camera
exec crates/xp4/target/release/xt4-camera "$@"
