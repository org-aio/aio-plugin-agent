#!/bin/sh
set -eu
cargo zigbuild --locked --release --target x86_64-unknown-linux-gnu.2.17 -p az-agent-server
cp target/x86_64-unknown-linux-gnu/release/az-agent-server dist/agent-server
