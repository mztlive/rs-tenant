#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

run() {
  printf '\n==> %s\n' "$*"
  "$@"
}

run cargo fmt --all --check
run cargo test --offline
run cargo test --offline --features serde
run cargo test --offline --features memory-store
run cargo test --offline --features memory-store,memory-cache
run cargo test --offline --features memory-store,memory-cache,serde
run cargo test --offline --features memory-store,platform,serde
run cargo test --offline --features memory-store,memory-cache,serde,platform,axum,axum-jwt
run cargo test --offline --examples --features memory-store,memory-cache,serde,platform,axum,axum-jwt
run cargo clippy --offline --all-targets --all-features -- -D warnings
