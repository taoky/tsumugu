#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.."

if output=$(bash scripts/check-release-version.sh refs/tags/20260424 2>&1); then
    status=0
else
    status=$?
fi
if [[ "$status" -eq 0 ]]; then
    echo "expected mismatched release tag to fail" >&2
    exit 1
fi
if [[ "$output" != *"0.20260424.0"* || "$output" != *"0.20260323.1"* ]]; then
    echo "mismatched release tag output did not mention expected and actual versions" >&2
    echo "$output" >&2
    exit 1
fi

bash scripts/check-release-version.sh refs/tags/20260323.1 >/dev/null
GITHUB_REF=refs/heads/pudding bash scripts/check-release-version.sh >/dev/null

echo "release version checks passed"
