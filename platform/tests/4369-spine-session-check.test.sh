#!/usr/bin/env bash
# @test-type: unit — runs spine-session-check on a fixture spine and a fixture owner map; no live log, no API
# @domain: identity
#
# #4369 — an event's principal must be its session's owner. The check must go red
# when they differ, and stay green when they match or the event names no session.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
now=$(TZ=America/New_York date '+%Y-%m-%dT%H:%M:%S.000-0400')
ev() { printf '{"timestamp":"%s","event":"%s","role":"silas"%s}\n' "$now" "$1" "$2"; }
echo '{"silas-s1":"principal-silas"}' > "$T/owners.json"
pass=0; fail=0
run() { bash -c "python3 '$ROOT/platform/scripts/spine-session-check' --log '$T/log' --owners '$T/owners.json'" > "$T/out" 2>&1; echo $?; }
chk() { if [ "$2" = "$3" ] && grep -qF -- "$4" "$T/out"; then echo "PASS $1"; pass=$((pass+1)); else echo "FAIL $1 (exit $2, want $3)"; cat "$T/out"; fail=$((fail+1)); fi; }
{ ev a ',"principal":"principal-silas","session":"silas-s1"'; ev b ''; } > "$T/log"
chk "an event whose principal owns its session is green; one with no session is counted, not judged" "$(run)" 0 "events 60m: 2 · with a session: 1 (50%) · principal ≠ owner: 0"
{ ev forged ',"principal":"principal-kade","session":"silas-s1"'; } > "$T/log"
chk "NEGATIVE: an event naming another principal's session is red, and named" "$(run)" 1 "RED forged: session silas-s1 is owned by principal-silas, the event says principal-kade"
echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
