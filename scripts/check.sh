#!/usr/bin/env bash
# Formats and lints the workspace. Prints only OK/FAIL so an AI assistant
# driving this script doesn't burn tokens on clean output — the full log is
# only surfaced when fmt or clippy actually finds something.
set -uo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

LOG="$(mktemp)"
trap 'rm -f "$LOG"' EXIT
STATUS=0

{
    echo "--- cargo fmt ---"
    cargo fmt --all -- --check
} >>"$LOG" 2>&1 || STATUS=1

{
    echo "--- cargo clippy ---"
    cargo clippy --workspace --all-targets --all-features -- -D warnings
} >>"$LOG" 2>&1 || STATUS=1

if [ "$STATUS" -eq 0 ]; then
    echo "OK: fmt + clippy clean"
else
    echo "FAIL: fmt/clippy found issues"
    echo "---"
    cat "$LOG"
    exit 1
fi
