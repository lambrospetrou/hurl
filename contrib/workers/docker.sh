#!/bin/bash
# Runs an npm script of worker/ (default: vendor) in a Docker container that has all the build
# tools, so the host only needs Docker. Examples:
#   contrib/workers/docker.sh             # vendor/hurl-wasm/
#   contrib/workers/docker.sh test        # build, then the Vitest tests in workerd
#   contrib/workers/docker.sh typecheck
# The outputs (worker/src/hurl_wasm.wasm, vendor/hurl-wasm/) are written to the repository. The
# Cargo target, node_modules, the libxml2 build and the Cargo registry stay in Docker volumes.
set -euo pipefail
HERE=$(dirname "$(realpath "$0")")
ROOT=$(realpath "$HERE/../..")
IMAGE=hurl-workers-build
RUST_VERSION=$(grep '^rust-version' "$ROOT/packages/hurl/Cargo.toml" | cut -f2 -d'"')

docker build -q -t "$IMAGE" --build-arg RUST_VERSION="$RUST_VERSION" - < "$HERE/Dockerfile" > /dev/null
tty=()
if [ -t 1 ]; then tty=(-t); fi
docker run --rm "${tty[@]}" \
	-v "$ROOT:/src" \
	-v hurl-workers-target:/src/contrib/workers/hurl_wasm/target \
	-v hurl-workers-node-modules:/src/contrib/workers/worker/node_modules \
	-v hurl-workers-libxml2:/src/contrib/workers/libxml2/work \
	-v hurl-workers-cargo-registry:/usr/local/cargo/registry \
	-e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
	-w /src/contrib/workers/worker \
	"$IMAGE" bash -c '
		npm ci --no-audit --no-fund --loglevel=error && npm run "$@"
		status=$?
		chown -R "$HOST_UID:$HOST_GID" src ../vendor 2> /dev/null || true
		exit $status' bash "${@:-vendor}"
