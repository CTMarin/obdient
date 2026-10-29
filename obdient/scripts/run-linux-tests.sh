#!/usr/bin/env bash
# Runs cargo test inside a Linux container: the pty-based serial tests only run on Linux.
# Usage: scripts/run-linux-tests.sh [cargo test args...]
set -euo pipefail

crate_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

docker run --rm \
  -v "$crate_dir":/src:ro \
  -v obdient-cargo-registry:/usr/local/cargo/registry \
  -v obdient-target:/target \
  -e CARGO_TARGET_DIR=/target \
  -w /src \
  rust:1-slim \
  cargo test --locked "$@"
