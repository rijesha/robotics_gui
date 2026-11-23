#!/bin/sh
# Regenerate gen/rust from schema/. Needs struct-frame (pip install struct-frame).
set -e
cd "$(dirname "$0")/.."
SF=struct-frame
[ -x .venv/bin/struct-frame ] && SF=.venv/bin/struct-frame
"$SF" schema/rgf.sf --build-rust --rust-path gen/rust --force "$@"
# The generated lib.rs carries a doc example (`use your_crate::prelude::*`) that does not
# compile as a doctest, so `cargo test --workspace` fails without this.
grep -q '^doctest' gen/rust/Cargo.toml || echo 'doctest = false' >> gen/rust/Cargo.toml
