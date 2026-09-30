#!/usr/bin/env bash
# @test-type: unit — runs chorus-werk-test-state against a saved /nightly page; no live services
#
# #4413 — /wts reads the nightly's suites and failed cases from the /nightly page.
# The fixture self-test is the negative proof: removing a red suite from the page
# must remove it from the answer, and type groups must stay in run order.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
out="$(python3 "$ROOT/platform/scripts/chorus-werk-test-state" --fixture)"; rc=$?
echo "$out"
if [ "$rc" = 0 ] && grep -q "NEGATIVE PROOF OK" <<<"$out"; then
  echo "=== Results: 1 passed, 0 failed ==="; exit 0
fi
echo "=== Results: 0 passed, 1 failed ==="; exit 1
