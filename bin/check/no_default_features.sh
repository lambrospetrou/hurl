#!/bin/bash
set -Eeuo pipefail

# Checks hurl and hurl_core without their default native features (libcurl, libxml2), as built
# for WebAssembly hosts like Cloudflare Workers (see contrib/workers).
cargo clippy --package hurl --package hurl_core --package hurlfmt --lib --tests --no-default-features
cargo test --package hurl --package hurl_core --package hurlfmt --lib --no-default-features

rustup target add wasm32-unknown-unknown
RUSTFLAGS='--cfg getrandom_backend="custom"' \
    cargo clippy --package hurl --package hurl_core --package hurlfmt --lib --no-default-features --target wasm32-unknown-unknown
