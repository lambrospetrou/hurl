# Build tools for contrib/workers: Node, Rust with the wasm32 target, bindgen and libclang.
# docker.sh builds this image and runs the npm scripts of worker/ in it.
FROM node:24-bookworm

ARG RUST_VERSION
ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH

RUN apt-get update \
    && apt-get install -y --no-install-recommends libclang-dev \
    && rm -rf /var/lib/apt/lists/*
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal \
        --default-toolchain "${RUST_VERSION:?set with --build-arg}" --target wasm32-unknown-unknown \
    && cargo install --locked bindgen-cli@0.72.1 \
    && rm -rf "$CARGO_HOME/registry"
# The repository is mounted from the host, with another owner.
RUN git config --system safe.directory '*'
