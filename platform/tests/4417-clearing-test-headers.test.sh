#!/usr/bin/env bash
# @test-type: fitness — walks the Clearing and pulse test files; no live services
# @card: #4417
# @owner: wren
#
# #4417 — every test of the Clearing, messages, streams, cards, pulse and spine
# says which card it serves and who owns it (Jeff 10-01: these are Wren's), so
# the runner and /nightly can group card → domain → type without guessing.
# Names every offender, never a count alone. A target that moves must fail loud.
# NEGATIVE PROOF: HEADERS_4417_FILES=platform/tests/fixtures/4417-headers/no-headers.test.ts → FAIL.
set -u
ROOT="${CHORUS_ROOT_OVERRIDE:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
cd "$ROOT" || exit 1
files=$(git ls-files directing/clearing/tests platform/pulse/src proving/flows 2>/dev/null \
  | grep -E '\.(test\.ts|spec\.cjs)$' | grep -vE '^proving/flows/(lib/|[^c]|c[^l])' )
[ -n "${HEADERS_4417_FILES:-}" ] && files="$HEADERS_4417_FILES"
n=$(printf '%s\n' "$files" | grep -c . || true)
if [ -z "${HEADERS_4417_FILES:-}" ] && [ "$n" -lt 50 ]; then
  echo "FAIL only $n files found under the Clearing/pulse test paths — the target moved; fix the path list"
  echo "=== Results: 0 passed, 1 failed ==="; exit 1
fi
bad=""
for f in $files; do
  head -6 "$f" | grep -qE '^// @card: (#[0-9]+|none)$' || bad="$bad $f(card)"
  head -6 "$f" | grep -qE '^// @owner: (wren|silas|kade)$' || bad="$bad $f(owner)"
done
if [ -n "$bad" ]; then
  echo "FAIL test files without @card / @owner in their first lines:"; printf '  %s\n' $bad
  echo "=== Results: 0 passed, 1 failed ==="; exit 1
fi
echo "PASS $n test files name their card and owner"
echo "=== Results: 1 passed, 0 failed ==="
