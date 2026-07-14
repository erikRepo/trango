#!/usr/bin/env bash
# Runs the full workspace test suite quietly. Prints only OK/FAIL so an AI
# assistant driving this script doesn't burn tokens on passing test output —
# the full log is only surfaced when something actually fails.
set -uo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

LOG="$(mktemp)"
trap 'rm -f "$LOG"' EXIT

if cargo test --workspace --all-features --quiet >"$LOG" 2>&1; then
    echo "OK: all tests passed"
else
    echo "FAIL: tests failed"
    echo "---"
    cat "$LOG"
    exit 1
fi
