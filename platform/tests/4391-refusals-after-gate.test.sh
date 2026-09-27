#!/usr/bin/env bash
# @test-type: unit — runs refusals-after-gate on fixture transcripts; reads no live transcript
# @domain: gates
#
# #4391 — the count that found the "show your reasoning" gate must tell a refusal
# after a gate from a refusal after anything else, and count nothing when there
# is no refusal.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
D="$T/-x-roles-kade"; mkdir -p "$D"
res() { printf '{"type":"user","message":{"content":[{"type":"tool_result","content":"%s"}]}}\n' "$1"; }
stop='{"type":"user","timestamp":"2026-09-27T15:59:52Z","message":{"content":"Your response above was stopped by a safety classifier — this is not a tool or API error."}}'
{ res "PreToolUse:Write hook error: Context synthesis gate: you searched"; echo "$stop"; } > "$D/a.jsonl"
{ res "test result: ok. 21 passed"; echo "$stop"; } > "$D/b.jsonl"
{ res "PreToolUse:Write hook error: Context synthesis gate"; } > "$D/c.jsonl"
out=$("$ROOT/platform/scripts/refusals-after-gate" --dir "$T")
pass=0; fail=0
chk() { if printf '%s' "$out" | grep -qF -- "$2"; then echo "PASS $1"; pass=$((pass+1)); else echo "FAIL $1 — wanted: $2"; echo "$out" | sed 's/^/  /'; fail=$((fail+1)); fi; }
chk "a refusal right after a gate message counts as GATE" "first refusal after a gate: 1"
chk "NEGATIVE: a refusal after an ordinary result counts as OTHER, not GATE" "other: 1"
chk "NEGATIVE: a gate message with no refusal counts nothing" "refusals: 2 conversations"
echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
