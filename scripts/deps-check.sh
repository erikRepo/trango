#!/usr/bin/env bash
# Dependency health check — run occasionally (e.g. before a release or
# before adding a new dependency), not on every step. Output is informational
# so it's printed in full rather than collapsed to OK/FAIL.
set -uo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

echo "--- cargo audit ---"
cargo audit

echo
echo "--- cargo outdated ---"
cargo outdated
