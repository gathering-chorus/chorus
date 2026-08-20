#!/bin/bash
# @test-type: integration — needs-stack lite: reads the spine + git only
#
# #3939 AC4 — the month-old-binary DETECTOR: for every installed werk verb,
# find its last binary.deployed commit and ask git whether that verb's SOURCE
# has changed on origin/main since. Changed source + unchanged binary = DRIFT:
# the pipeline is running code someone already fixed. This is exactly the
# condition that let a Jul-24 werk-commit serve a month past its fix.
set -u
CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
SPINE="${VERB_DRIFT_SPINE:-$HOME/.chorus/chorus.log}"
BIN_DIR="${VERB_DRIFT_BIN:-$HOME/.chorus/bin}"
PASS=0; FAIL=0

[ -f "$SPINE" ] || { echo "SKIP: no spine at $SPINE"; exit 0; }
[ -d "$BIN_DIR" ] || { echo "SKIP: no bin dir at $BIN_DIR"; exit 0; }
git -C "$CHORUS_ROOT" fetch -q origin main 2>/dev/null || true

for bin in "$BIN_DIR"/werk-*; do
  [ -x "$bin" ] || continue
  name="$(basename "$bin")"
  case "$name" in *-bin) continue ;; esac
  crate="platform/services/$name"
  [ -d "$CHORUS_ROOT/$crate" ] || continue
  # newest deployed commit for this binary, from the spine (source of truth)
  commit=$(grep -a '"event":"binary.deployed"' "$SPINE" | grep -aE "\"binary\":\"$name\"" \
           | tail -1 | grep -aoE '"commit":"[0-9a-f]+"' | cut -d'"' -f4)
  if [ -z "$commit" ]; then
    # fall back to key=value payload form
    commit=$(grep -a 'binary.deployed' "$SPINE" | grep -aE "binary=$name([ ,\"]|$)" \
             | tail -1 | grep -aoE 'commit=[0-9a-f]+' | tail -1 | cut -d= -f2)
  fi
  if [ -z "$commit" ] || ! git -C "$CHORUS_ROOT" cat-file -e "$commit" 2>/dev/null; then
    echo "FAIL: $name — no traceable binary.deployed commit (unprovenanced binary)"; FAIL=$((FAIL+1)); continue
  fi
  if git -C "$CHORUS_ROOT" diff --quiet "$commit" origin/main -- "$crate" "platform/services/shared" 2>/dev/null; then
    echo "PASS: $name — installed binary matches main's source ($commit)"; PASS=$((PASS+1))
  else
    echo "FAIL: $name — DRIFT: $crate changed on main since deployed commit $commit (rebuild + install)"; FAIL=$((FAIL+1))
  fi
done

# NEGATIVE PROOF (#3734): a fabricated stale-deploy fixture must read as drift.
TF="$(mktemp -d)"; trap 'rm -rf "$TF"' EXIT
git -C "$TF" init -q -b main .
mkdir -p "$TF/platform/services/werk-x" && echo v1 > "$TF/platform/services/werk-x/lib.rs"
git -C "$TF" add . && git -C "$TF" -c user.email=t@t -c user.name=t commit -q -m one
C1=$(git -C "$TF" rev-parse HEAD)
echo v2 > "$TF/platform/services/werk-x/lib.rs"
git -C "$TF" add . && git -C "$TF" -c user.email=t@t -c user.name=t commit -q -m two
if git -C "$TF" diff --quiet "$C1" HEAD -- platform/services/werk-x; then
  echo "FAIL: negative proof — a moved source read as no-drift"; FAIL=$((FAIL+1))
else
  echo "PASS: negative proof — moved source reads as drift"; PASS=$((PASS+1))
fi

echo "=== verb-binary-drift: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
