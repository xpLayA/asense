#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

for command in cargo rustc pkg-config cc; do
  command -v "$command" >/dev/null 2>&1 || {
    printf 'asense-build: missing build tool: %s (see README.md for dependencies)\n' "$command" >&2
    exit 1
  }
done

printf 'Building ASense with %s and %s\n' "$(cargo --version)" "$(rustc --version)"
cargo build --release --locked --bin asensed --no-default-features
cargo build --release --locked --bin asense --features gui

asense_output_dir="${CARGO_TARGET_DIR:-$ROOT/target}/release"
asense_output_dir="$(cd -- "$asense_output_dir" && pwd)"
printf '\nBuilt binaries:\n  %s/asense\n  %s/asensed\n' "$asense_output_dir" "$asense_output_dir"
printf 'Install separately (also rebuilds the optional kernel driver):\n'
printf '  %q %q\n' "$ROOT/install.sh" "$asense_output_dir/asense"
