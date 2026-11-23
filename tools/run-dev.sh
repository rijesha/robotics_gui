#!/bin/sh
# Run the parts as separate processes, the way they deploy: simulated vehicles, backend,
# and the web UI (open http://<this machine>:8080 on a desktop or phone).
# Usage: tools/run-dev.sh [rover6|tractor|skid|boat]...   e.g. tools/run-dev.sh rover6 tractor
set -e
cd "$(dirname "$0")/.."
cargo build --release -p rgf-sim -p rgf-backend
trap 'kill 0' EXIT INT TERM
[ $# -gt 0 ] || set -- rover6
# One simulator per vehicle, each on its own port; the backend numbers them in this order.
slot=0
addrs=
for vehicle in "$@"; do
  addr="127.0.0.1:$((14560 + slot))"
  target/release/rgf-sim --vehicle "$vehicle" --bind "$addr" --slot "$slot" &
  addrs="${addrs:+$addrs,}$addr"
  slot=$((slot + 1))
done
target/release/rgf-backend --vehicle "$addrs" &
cd apps/station && trunk serve --release
