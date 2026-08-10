#!/usr/bin/env bash
# The row ceiling's own proofs (#3806 / #3734).
#
# The violation this exists to catch: a row that never returns starves the whole
# readout — 2026-08-09, row 7 produced no verdict in 600s and the suite hung
# with it. These fixtures run the REAL runner over fake rows in a sandbox:
#   1. a row that hangs must come back UNMEASURABLE naming the ceiling, fast;
#   2. a row whose PROOF hangs must come back UNTRUSTWORTHY (a proof that
#      cannot finish cannot vouch — its green would be void);
#   3. the ceiling must not eat verdicts: a fast RED still reads RED.
# Each fixture is the guarded condition VIOLATED, shown caught.
set -u

SRC="$(cd "$(dirname "$0")" && pwd)"
ROOT=$(mktemp -d); trap 'rm -rf "$ROOT"' EXIT
cp "$SRC/run-fitness.sh" "$ROOT/"
mkdir -p "$ROOT/fixtures"; : > "$ROOT/fixtures/login-wall-otherwise-perfect.html"

pass=0; fail=0
ok() { if eval "$2"; then echo "PASS: $1"; pass=$((pass+1)); else echo "FAIL: $1"; fail=$((fail+1)); fi; }

# Fake rows: everything green-and-provable by default.
mk() { printf '%s\n' "$2" > "$ROOT/$1"; chmod +x "$ROOT/$1"; }
for s in probe-public-root.sh probe-signin-lands.sh probe-running-is-merged.sh \
         probe-write-still-works.sh probe-lists-are-local.sh probe-nobody-walk.sh; do
  mk "$s" 'exit 0'
done

# 1. VIOLATION: the live run hangs.
mk probe-nobody-walk.sh 'case "${1:-}" in --prove-red) exit 0;; esac; sleep 999'
OUT=$(FITNESS_ROW_CEILING=2 bash "$ROOT/run-fitness.sh" 7)
ok "a hanging row concludes UNMEASURABLE, naming the ceiling" \
   "grep -q 'UNMEASURABLE.*ceiling' <<<'$OUT'"
ok "the hanging row is never RED (a timeout is not a finding)" \
   "! grep -q 'row7  RED' <<<'$OUT'"

# 2. VIOLATION: the proof hangs.
mk probe-nobody-walk.sh 'case "${1:-}" in --prove-red) sleep 999;; esac; exit 0'
OUT=$(FITNESS_ROW_CEILING=2 bash "$ROOT/run-fitness.sh" 7)
ok "a hanging PROOF voids the row as UNTRUSTWORTHY" \
   "grep -q 'UNTRUSTWORTHY.*ceiling' <<<'$OUT'"

# 3. The ceiling must not eat real verdicts.
mk probe-nobody-walk.sh 'case "${1:-}" in --prove-red) exit 0;; esac; echo broken; exit 1'
OUT=$(FITNESS_ROW_CEILING=2 bash "$ROOT/run-fitness.sh" 7)
ok "a fast RED still reads RED under the ceiling" "grep -q 'row7  RED' <<<'$OUT'"

echo "=== ceiling suite: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
