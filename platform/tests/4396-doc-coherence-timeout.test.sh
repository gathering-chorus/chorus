#!/usr/bin/env bash
# @test-type: unit — stubs curl; no live services
# @domain: tests
#
# #4396 — a link probe that timed out under load is asked once more before it
# counts as broken. On 2026-09-28 09:56 a loaded box turned 0 broken hrefs into
# 16 and red #4396's test; by hand a minute later it was 0. A link that still
# does not answer stays broken, so a server that is really down stays red.
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
mkdir -p "$T/bin"
cat > "$T/bin/curl" <<'S'
#!/bin/bash
url="${@: -1}"; t=3
for ((i=1;i<=$#;i++)); do [ "${!i}" = "--max-time" ] && { j=$((i+1)); t="${!j}"; }; done
case "$url" in
  */api/doc-catalog) printf '{"groups":[{"docs":[{"href":"/ok"},{"href":"/slow"},{"href":"/dead"}]}]}'; exit 0 ;;
  */ok) printf 200 ;;
  */slow) if [ "$t" -gt 3 ]; then printf 200; else printf 000; fi ;;
  */dead) printf 000 ;;
esac
S
chmod +x "$T/bin/curl"
pass=0; fail=0
out=$(cd "$ROOT" && PATH="$T/bin:$PATH" CHORUS_API_HOST=stub CHORUS_FALLBACK_HOST=stub bash platform/scripts/doc-coherence.sh 2>&1)
n=$(printf '%s\n' "$out" | grep -E '^ *broken-hrefs:' | head -1 | grep -oE '[0-9]+')
if [ "$n" = "1" ]; then echo "PASS a slow link answered on the second ask; only the dead one counts (broken-hrefs: $n)"; pass=$((pass+1)); else echo "FAIL broken-hrefs: ${n:-none} (want 1: /dead)"; fail=$((fail+1)); fi
# NEGATIVE PROOF: the dead link is still counted — a down server cannot read as clean
if [ "${n:-0}" -ge 1 ]; then echo "PASS NEGATIVE: a link that never answers is still broken"; pass=$((pass+1)); else echo "FAIL NEGATIVE: the dead link was not counted"; fail=$((fail+1)); fi
echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
