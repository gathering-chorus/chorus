#!/usr/bin/env bash
# @domain: domains — placed by chorus-crawl's domain rules (#4538)
# 4578-staging-serves-card-build.test.sh — #4578 AC2 (the staging half).
#
# The athena-make launcher serves a CARD's build when its unit names a pointer
# file holding that build's path, and the installed binary otherwise. Run in a
# sandbox: stub curl (Fuseki ready), a fake installed athena-make and a fake card
# build, each dropping its own marker.
#
# NEGATIVE PROOFS: a pointer naming a missing file is refused (exit 78), and
# NEITHER binary runs — a run proving its build is never served yesterday's.
# The staging plist names the pointer; the prod plist does not.
#
# Run: bash platform/tests/4578-staging-serves-card-build.test.sh
set -uo pipefail
PASS=0; FAIL=0
ok() { echo "  PASS: $1"; PASS=$((PASS+1)); }
no() { echo "  FAIL: $1"; FAIL=$((FAIL+1)); }
echo "=== staging serves the card's athena-make (#4578) ==="

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LAUNCHER="$REPO_ROOT/platform/services/athena-make/athena-make-launch.sh"
STAGING_PLIST="$REPO_ROOT/platform/launchd/com.chorus.athena-make.staging.plist"
PROD_PLIST="$REPO_ROOT/platform/launchd/com.chorus.athena-make.plist"
[ -f "$LAUNCHER" ] || { no "launcher source missing at $LAUNCHER"; echo "=== Results: $PASS passed, $FAIL failed ==="; exit 1; }

SBX="$(mktemp -d)"; trap 'rm -rf "$SBX"' EXIT
export HOME="$SBX/home"; mkdir -p "$HOME/.chorus/bin" "$SBX/stubbin" "$SBX/card"
printf '#!/usr/bin/env bash\nexit 0\n' > "$SBX/stubbin/curl"; chmod +x "$SBX/stubbin/curl"
fake() { printf '#!/usr/bin/env bash\necho %s > "%s/ran"\n' "$1" "$SBX" > "$2"; chmod +x "$2"; }
fake installed "$HOME/.chorus/bin/athena-make"
fake card "$SBX/card/athena-make"
export CHORUS_FUSEKI="http://localhost:3030/staging"
run() { rm -f "$SBX/ran"; PATH="$SBX/stubbin:$PATH" bash "$LAUNCHER" serve >/dev/null 2>"$SBX/err"; echo $?; }
ran() { cat "$SBX/ran" 2>/dev/null || echo none; }

unset ATHENA_MAKE_BIN_FILE
rc=$(run); [ "$rc" = 0 ] && [ "$(ran)" = installed ] && ok "no pointer: the installed binary runs" || no "no pointer: rc=$rc ran=$(ran)"

export ATHENA_MAKE_BIN_FILE="$SBX/athena-make.bin"
: > "$ATHENA_MAKE_BIN_FILE"
rc=$(run); [ "$rc" = 0 ] && [ "$(ran)" = installed ] && ok "empty pointer: the installed binary runs" || no "empty pointer: rc=$rc ran=$(ran)"

echo "$SBX/card/athena-make" > "$ATHENA_MAKE_BIN_FILE"
rc=$(run); [ "$rc" = 0 ] && [ "$(ran)" = card ] && ok "pointer to the card build: the card build runs" || no "card pointer: rc=$rc ran=$(ran)"

echo "$SBX/card/gone" > "$ATHENA_MAKE_BIN_FILE"
rc=$(run)
if [ "$rc" = 78 ] && [ "$(ran)" = none ] && grep -q "REFUSED" "$SBX/err"; then
  ok "NEGATIVE PROOF: a pointer to a missing build is refused and nothing runs"
else
  no "missing build: rc=$rc ran=$(ran) (must be 78, none)"
fi

grep -q "<key>ATHENA_MAKE_BIN_FILE</key>" "$STAGING_PLIST" && ok "staging plist names the pointer" || no "staging plist lacks ATHENA_MAKE_BIN_FILE"
if [ -f "$PROD_PLIST" ]; then
  grep -q "ATHENA_MAKE_BIN_FILE" "$PROD_PLIST" && no "prod plist names a pointer (prod must run the installed binary)" || ok "prod plist names no pointer"
fi

echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
