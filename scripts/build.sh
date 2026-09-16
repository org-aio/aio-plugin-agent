#!/bin/sh
set -eu
node scripts/check-sdk.mjs
node scripts/check-family.mjs
dx build --package az-agent-frontend --platform web --release
node scripts/package-frontend.mjs
TARGET=${AIO_RUST_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}
if [ "${1:-}" = "--process" ]; then
  TARGET=x86_64-unknown-linux-gnu
  cargo zigbuild --locked --release --target "$TARGET.2.17" -p az-agent-server
else
  cargo build --locked --release --target "$TARGET" -p az-agent-server
fi
cp "target/$TARGET/release/az-agent-server" dist/agent-server
