#!/bin/bash
# @test-type: integration — needs-stack lite: reads the spine + git only
#
# #3939 AC4 — the month-old-binary DETECTOR: for every installed werk verb,
# find its last binary.deployed commit and ask git whether that verb's SOURCE
# has changed on origin/main since. Changed source + unchanged binary = DRIFT:
# the pipeline is running code someone already fixed. This is exactly the
# condition that let a Jul-24 werk-commit serve a month past its fix.
set -u
# #3949 — derive, never hardcode (#3904 rule; the path guard caught this).
CHORUS_ROOT="${CHORUS_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
SPINE="${VERB_DRIFT_SPINE:-$HOME/.chorus/chorus.log}"
BIN_DIR="${VERB_DRIFT_BIN:-$HOME/.chorus/bin}"
PASS=0; FAIL=0

# #4455 — a verb's source is its own crate plus the shared files it includes,
# not the whole shared folder. Counting every shared file called 9 verbs drifted
# when #4446 changed two files none of them include. Same rule werk-deploy uses
# to decide which crates to redeploy (#4446 shared_includers).
verb_sources() {  # <repo> <rev> <crate> → paths, one per line
  echo "$3"
  git -C "$1" grep -hoE 'shared/[A-Za-z0-9_]+\.rs' "$2" -- "$3" 2>/dev/null \
    | sort -u | sed 's|^|platform/services/|'
}

[ -f "$SPINE" ] || { echo "SKIP: no spine at $SPINE"; exit 0; }
[ -d "$BIN_DIR" ] || { echo "SKIP: no bin dir at $BIN_DIR"; exit 0; }
git -C "$CHORUS_ROOT" fetch -q origin main 2>/dev/null || true
# #4416 — read the spine ONCE. Each verb grepped the whole spine (3.6 GB, and
# never rotated) twice; ten verbs made this suite 388 s of the nightly.
DEPLOYS="$(mktemp)"
# #4455 — only installs into the shared bin count. A werk run installs its
# card's verbs into that role's own slot (target werk, #3101) and emits the same
# event; reading those called the shared werk-build current at an unlanded
# card's commit, when ~/.chorus/bin still ran a 10-03 build.
LC_ALL=C grep -aF 'binary.deployed' "$SPINE" 2>/dev/null \
  | LC_ALL=C grep -avE '"target":"werk"|target=werk([ ,"]|$)' > "$DEPLOYS" || true

for bin in "$BIN_DIR"/werk-*; do
  [ -x "$bin" ] || continue
  name="$(basename "$bin")"
  case "$name" in *-bin) continue ;; esac
  crate="platform/services/$name"
  [ -d "$CHORUS_ROOT/$crate" ] || continue
  # newest deployed commit for this binary, from the spine (source of truth)
  # #4131 — the canonical deploy installs werk-test as `werk-test-bin` (the
  # `werk-test` name is the wrapper) and emits binary.deployed under that name;
  # matching the bare verb name read the 19:08 werk-side install as the newest
  # and called a freshly landed verb a month stale. Accept either spelling.
  commit=$(grep -a '"event":"binary.deployed"' "$DEPLOYS" | grep -aE "\"binary\":\"$name(-bin)?\"" \
           | tail -1 | grep -aoE '"commit":"[0-9a-f]+"' | cut -d'"' -f4)
  if [ -z "$commit" ]; then
    # fall back to key=value payload form
    commit=$(grep -aE "binary=$name([ ,\"]|$)" "$DEPLOYS" | grep -aE "binary=$name([ ,\"]|$)" \
             | tail -1 | grep -aoE 'commit=[0-9a-f]+' | tail -1 | cut -d= -f2)
  fi
  if [ -z "$commit" ] || ! git -C "$CHORUS_ROOT" cat-file -e "$commit" 2>/dev/null; then
    echo "FAIL: $name — no traceable binary.deployed commit (unprovenanced binary)"; FAIL=$((FAIL+1)); continue
  fi
  # shellcheck disable=SC2046
  if git -C "$CHORUS_ROOT" diff --quiet "$commit" origin/main -- $(verb_sources "$CHORUS_ROOT" origin/main "$crate") 2>/dev/null; then
    echo "PASS: $name — installed binary matches main's source ($commit)"; PASS=$((PASS+1))
  else
    echo "FAIL: $name — DRIFT: $crate changed on main since deployed commit $commit (rebuild + install)"; FAIL=$((FAIL+1))
  fi
done

# NEGATIVE PROOF (#3734): a fabricated stale-deploy fixture must read as drift.
TF="$(mktemp -d)"; trap 'rm -rf "$TF" "$DEPLOYS"' EXIT
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

# NEGATIVE PROOF (#4455): a shared file the verb includes moved → drift; a
# shared file it does not include moved → no drift.
mkdir -p "$TF/platform/services/shared"
echo a1 > "$TF/platform/services/shared/used.rs"; echo b1 > "$TF/platform/services/shared/other.rs"
echo '#[path = "../../shared/used.rs"] mod used;' > "$TF/platform/services/werk-x/lib.rs"
git -C "$TF" add . && git -C "$TF" -c user.email=t@t -c user.name=t commit -q -m three
C3=$(git -C "$TF" rev-parse HEAD)
echo b2 > "$TF/platform/services/shared/other.rs"
git -C "$TF" add . && git -C "$TF" -c user.email=t@t -c user.name=t commit -q -m four
# shellcheck disable=SC2046
if git -C "$TF" diff --quiet "$C3" HEAD -- $(verb_sources "$TF" HEAD platform/services/werk-x); then
  echo "PASS: a shared file the verb does not include is not drift"; PASS=$((PASS+1))
else
  echo "FAIL: an unincluded shared file read as drift"; FAIL=$((FAIL+1))
fi
echo a2 > "$TF/platform/services/shared/used.rs"
git -C "$TF" add . && git -C "$TF" -c user.email=t@t -c user.name=t commit -q -m five
# shellcheck disable=SC2046
if git -C "$TF" diff --quiet "$C3" HEAD -- $(verb_sources "$TF" HEAD platform/services/werk-x); then
  echo "FAIL: negative proof — an included shared file moved and read as no-drift"; FAIL=$((FAIL+1))
else
  echo "PASS: negative proof — an included shared file moved reads as drift"; PASS=$((PASS+1))
fi

echo "=== verb-binary-drift: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
