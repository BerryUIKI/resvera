#!/usr/bin/env bash
set -euo pipefail

echo "==> Running diff hygiene check..."
if [ "${1:-}" != "" ]; then
    git diff --check "$1"
elif git rev-parse --verify HEAD~1 >/dev/null 2>&1; then
    git diff --check HEAD~1
else
    git diff --check
fi

echo "==> Running Rustfmt check across workspace..."
cargo fmt --all -- --check

echo "==> Running Clippy with warnings denied..."
cargo clippy --all-targets -- -D warnings

echo "==> Running export & parity toolchain tests..."
if command -v python3 >/dev/null 2>&1; then
    python3 -m unittest discover -s tools/export
else
    python -m unittest discover -s tools/export
fi

echo "==> All hygiene checks passed successfully!"
