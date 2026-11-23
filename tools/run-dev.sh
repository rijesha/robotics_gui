#!/bin/sh
# Run the three parts as separate processes, the way they deploy: simulated vehicle,
# backend, and the web UI (open http://<this machine>:8080 on a desktop or phone).
# Usage: tools/run-dev.sh [rover6|tractor|skid|boat]
set -e
cd "$(dirname "$0")/.."
cargo build --release -p rgf-sim -p rgf-backend
trap 'kill 0' EXIT INT TERM
target/release/rgf-sim --vehicle "${1:-rover6}" &
target/release/rgf-backend &
cd apps/station && trunk serve --release
