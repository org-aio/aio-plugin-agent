#!/bin/sh
set -eu
node scripts/generate-contract.mjs --check
node scripts/check-sdk.mjs
node scripts/check-family.mjs
node scripts/prepare-graph.mjs
KOTLIN_CLI_NO_WELCOME_BANNER=1 ./kotlin build -m frontend -p wasmJs -v release
TARGET=${AIO_RUST_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}
cargo build --locked --release --target "$TARGET" -p az-agent-server
mkdir -p dist
rm -rf dist/frontend
cp -R build/tasks/_frontend_buildWasmJsAppWasmJsRelease dist/frontend
cp "target/$TARGET/release/az-agent-server" dist/agent-server
node scripts/package-runtime.mjs
