#!/usr/bin/env bash
set -euo pipefail

workspace_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cargo test \
  --release \
  --manifest-path "${workspace_dir}/src-tauri/Cargo.toml" \
  performance_matrix_report \
  -- \
  --ignored \
  --nocapture
