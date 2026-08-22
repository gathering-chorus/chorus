#!/usr/bin/env bash
# test-werk-resume-measure-3972.sh — guard for #3972: the resume tree-hash is
# measured race-free (read-tree HEAD, never a copy of the live index) and a
# FAILED measurement is its own typed refusal — never a phantom 'tree changed'.
# The two states this check separates (#3734): could-not-measure vs tree-changed.
#
# HERMETIC: fixture git repo + pin in a tempdir; no live werk touched.

set -uo pipefail
trap '_rc=$?; if [ $_rc -eq 0 ]; then echo "=== Results: $PASS passed, 0 failed ==="; else echo "=== Results: $PASS passed, $FAIL failed ==="; fi' EXIT

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RC="$SCRIPT_DIR/werk-resume-check"
PASS=0; FAIL=0
ok()   { PASS=$((PASS+1)); }
bad()  { FAIL=$((FAIL+1)); echo "FAIL: $1"; }

TMP=$(mktemp -d)
cleanup() { rm -rf "$TMP"; }

export CHORUS_WERK_BASE="$TMP/werks"
export CHORUS_HOME="$TMP/home"
# The test brings its own world (#3528): werk-resume-check pins live under
# $HOME/.chorus, so HOME itself points into the tempdir — never the real one.
export HOME="$TMP/fakehome"
mkdir -p "$CHORUS_WERK_BASE/kade-999" "$CHORUS_HOME/ops/logs" "$HOME/.chorus/werk-runs"
W="$CHORUS_WERK_BASE/kade-999"
git -C "$W" init -q
git -C "$W" -c user.email=t@t -c user.name=t commit -q --allow-empty -m base
echo hello > "$W/f.txt"   # uncommitted edit — the common relaunch state

# helper: write the pin
pin() { printf '%s' "$1" > "$HOME/.chorus/werk-runs/999.json"; }
PIN_DIR="$HOME/.chorus/werk-runs"; mkdir -p "$PIN_DIR"
restore_pin() { rm -f "$PIN_DIR/999.json"; }

# 1. Hash is stable and covers uncommitted files.
h1=$("$RC" --tree-hash 999 kade); h2=$("$RC" --tree-hash 999 kade)
[ -n "$h1" ] && [ "$h1" = "$h2" ] && ok || bad "hash unstable: $h1 vs $h2"
echo more > "$W/untracked.txt"
h3=$("$RC" --tree-hash 999 kade)
[ "$h3" != "$h1" ] && ok || bad "untracked file must change the hash (add -A coverage)"

# 2. CARRY: pass proof against the current tree carries.
pin "{\"runId\":\"r1\",\"legs\":[{\"leg\":\"build\",\"verdict\":\"pass\",\"tree_hash\":\"$h3\"}]}"
out=$(WERK_RESUME=1 "$RC" 999 kade build 2>&1); rc=$?
[ "$rc" -eq 0 ] && ok || bad "identical tree should carry, got rc=$rc: $out"

# 3. Tree really changed -> the honest 'tree changed' refusal, both hashes named.
echo drift >> "$W/f.txt"
out=$(WERK_RESUME=1 "$RC" 999 kade build 2>&1); rc=$?
[ "$rc" -eq 2 ] && echo "$out" | grep -q "tree changed" && ok || bad "changed tree should refuse as 'tree changed', got rc=$rc: $out"

# 4. NEGATIVE PROOF (#3734): measurement FAILURE is typed, never 'tree changed'.
#    A broken werk (no git) forces tree_hash to fail — the old code compared
#    empty-vs-prior and reported the WRONG state.
rm -rf "$W/.git"
out=$(WERK_RESUME=1 "$RC" 999 kade build 2>&1); rc=$?
[ "$rc" -eq 2 ] && ok || bad "failed measurement must refuse (rc=2), got rc=$rc"
echo "$out" | grep -q "could not MEASURE" && ok || bad "refusal must name the measurement failure, got: $out"
echo "$out" | grep -q "tree changed" && bad "MUST NOT claim 'tree changed' on a failed measurement: $out" || ok

# 5. A prior proof with no measured tree is its own typed refusal.
git -C "$W" init -q && git -C "$W" -c user.email=t@t -c user.name=t commit -q --allow-empty -m base
pin '{"runId":"r1","legs":[{"leg":"build","verdict":"pass","tree_hash":""}]}'
out=$(WERK_RESUME=1 "$RC" 999 kade build 2>&1); rc=$?
[ "$rc" -eq 2 ] && echo "$out" | grep -q "NO measured tree" && ok || bad "empty prior proof must refuse typed, got rc=$rc: $out"

# 6. GUARD: the racy live-index copy cannot come back.
grep -q 'cp "$werk/.git/index"' "$RC" && bad "the racy index copy is back in werk-resume-check" || ok
grep -q "read-tree HEAD" "$RC" && ok || bad "race-free read-tree build missing"

restore_pin
cleanup
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
