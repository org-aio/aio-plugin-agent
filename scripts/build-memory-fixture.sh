#!/bin/sh
set -eu
cd "${AIO_MEMORY_ROOT:-../aio-plugin-agent-memory}"
cargo build --locked -p az-memory-server
