#!/usr/bin/env bash
# #3709 — an alarm that fires MUST carry what it observed.
#
# The failure this pins: for months the fuseki-harvest and lancedb alarms woke
# Silas with a verdict and no measurement. Both skip paths wrote evidence; the
# fire path — the only one a human acts on — wrote none. By the time anyone
# looked, the underlying query answered differently and there was nothing to
# compare against, so a recurring alarm was permanently undiagnosable and got
# dismissed as noise instead. Firing without evidence is the defect.
#
# Jeff's bar (2026-07-28): a monitor that cries wolf is itself a real defect to
# FIX or RETIRE, not to classify.
set -uo pipefail
ALERTS="${1:-$(cd "$(dirname "$0")/../../domains/alerts" && pwd)}"
FAIL=0

pass(){ echo "  PASS: $1"; }
fail(){ echo "  FAIL: $1"; FAIL=1; }

# A YAML block scalar ends at the first DEDENT to column 0, not at the first
# line that looks like a key. ci-main-red's check embeds python whose `try:` and
# `except ...:` lines match a naive /^[a-z_]+:/ terminator, which silently
# truncated the extracted script mid-string — the extractor lying about what it
# read, which is the same class of bug this whole card is about.
extract_block(){ awk -v key="$2" '
  $0 ~ "^"key": \\|" {f=1;next}
  f && /^[^[:space:]]/ {exit}
  f {sub(/^  /,"");print}' "$1"; }
extract_check(){ extract_block "$1" check; }

echo "=== fuseki-harvest: the FIRE path records the observation ==="
# Stub a SPARQL endpoint that answers 0 — the exact condition that fires.
STUB_DIR=$(mktemp -d)
cat > "$STUB_DIR/curl" <<'STUB'
#!/usr/bin/env bash
# stand-in for curl: always a successful HTTP call returning count 0
echo '{"head":{"vars":["c"]},"results":{"bindings":[{"c":{"value":"0"}}]}}'
exit 0
STUB
chmod +x "$STUB_DIR/curl"
SKIPLOG=$(mktemp)
out=$(PATH="$STUB_DIR:$PATH" FUSEKI_SKIP_LOG="$SKIPLOG" \
      bash "$ALERTS/fuseki-harvest-stale.check.sh" 2>&1)
rc=$?
echo "  check said: '$out' (rc=$rc)"
[ "$rc" -eq 1 ] && pass "fires (rc=1) on a genuine zero" || fail "should fire rc=1 on zero, got rc=$rc"
if [ -s "$SKIPLOG" ] && grep -q "count=0" "$SKIPLOG"; then
  pass "fire recorded the observed count: $(head -1 "$SKIPLOG" | cut -c1-120)"
else
  fail "fire wrote NO evidence — a recurring fire stays undiagnosable (log empty)"
fi
case "$out" in
  *url=*) pass "fire message names the endpoint it queried" ;;
  *)      fail "fire message does not say WHERE it looked: '$out'" ;;
esac
rm -rf "$STUB_DIR" "$SKIPLOG"

echo "=== lancedb-stale: verdict carries index, age and threshold ==="
CHECK=$(extract_check "$ALERTS/lancedb-stale.yml")
STALE_DIR=$(mktemp -d); touch -t 202601010000 "$STALE_DIR/old.lance"
out=$(LANCE_DIR="$STALE_DIR" bash -c "$CHECK" 2>&1); rc=$?
echo "  check said: '$out' (rc=$rc)"
[ "$rc" -eq 1 ] && pass "fires on a stale index" || fail "should fire on stale index, got rc=$rc"
case "$out" in *dir=*)          pass "names which index" ;;   *) fail "does not name the index: '$out'" ;; esac
case "$out" in *newest_age_h=*) pass "reports observed age" ;; *) fail "does not report observed age: '$out'" ;; esac
case "$out" in *threshold_h=*)  pass "reports its own threshold" ;; *) fail "does not report the threshold it used: '$out'" ;; esac

FRESH_DIR=$(mktemp -d); touch "$FRESH_DIR/new.lance"
out=$(LANCE_DIR="$FRESH_DIR" bash -c "$CHECK" 2>&1); rc=$?
echo "  check said: '$out' (rc=$rc)"
[ "$rc" -eq 0 ] && pass "passes on a fresh index" || fail "should pass on fresh index, got rc=$rc"
rm -rf "$STALE_DIR" "$FRESH_DIR"

echo "=== lancedb-stale: the nudge cannot drift from the check ==="
# The old text hardcoded "24h" while the check had used 72h since #3617 — the
# alarm misreported its own threshold to the only person who acts on it.
# Comments are prose ABOUT the drift (they may quote the old wording); only the
# executable lines can actually drift, so strip comments before asserting.
ACTION=$(extract_block "$ALERTS/lancedb-stale.yml" action \
         | sed 's/[[:space:]]*#.*$//')
if echo "$ACTION" | grep -qE '\$STATUS'; then
  pass "nudge carries the check's own \$STATUS"
else
  fail "nudge writes its own sentence — free to drift from the check"
fi
if echo "$ACTION" | grep -qE '24h|last 24 hours'; then
  fail "nudge still hardcodes 24h while the check uses 72h"
else
  pass "nudge states no threshold the check does not"
fi

echo "=== ci-main-red: exhaust the ways to see before reporting blind ==="
# #3519 made a token-less check report "unverifiable" instead of a false "ok" —
# correct, but it stopped there. alert-runner.sh sources no environment, so
# GH_TOKEN is NEVER set and the check blinded itself every night while
# quality.yml on main sat red for 39 consecutive runs (Jun 21 → Jul 29).
# `gh` is authenticated on this machine and holds a token in the keyring; a
# check that gives up without asking it is not unverifiable, it is unasked.
CI_SH="$ALERTS/ci-main-red.check.sh"
# #3713 — this assertion used to be `grep -q "gh auth token" "$CI_SH"`: it checked
# that a STRING was present in the file. That proves the author typed it, nothing
# about whether it runs — and it did not run, because the bare `gh` was unresolvable
# under launchd's PATH. The check shipped dead and this test said PASS. Run the
# thing instead. (Env-specific coverage lives in
# alert-checks-run-under-launchd-path.test.sh; here we assert it ASKS at all.)
out=$(env -u GH_TOKEN bash "$CI_SH" 2>&1)
case "$out" in
  red:*|ok) pass "obtains a token without GH_TOKEN in env and reports a verdict: ${out%%:*}" ;;
  *unverifiable*) fail "does not obtain a token when GH_TOKEN is unset — reports '$out'" ;;
  *) fail "unrecognised output with GH_TOKEN unset: '$out'" ;;
esac

# With no env token AND gh unable to produce one, unverifiable is the honest
# answer. Shadow gh with a failing stub rather than emptying PATH (that would
# take bash itself with it).
NOGH=$(mktemp -d)
printf '#!/usr/bin/env bash\nexit 1\n' > "$NOGH/gh"; chmod +x "$NOGH/gh"
out=$(GH_TOKEN="" PATH="$NOGH:$PATH" bash "$CI_SH" 2>&1); rc=$?
echo "  no token, gh cannot answer: '$out' (rc=$rc)"
case "$out" in
  ok) fail "reports plain 'ok' while genuinely blind — a red main never surfaces" ;;
  *unverifiable*) pass "still reports unverifiable when it truly cannot see" ;;
  *) fail "neither a verdict nor unverifiable: '$out'" ;;
esac
rm -rf "$NOGH"

# An answer it cannot READ is also unverifiable, never ok. The repo moved to
# gathering-chorus/chorus; the stale slug made GitHub reply 301, curl -sf did
# not follow it, the parser choked, and the catch-all branch exited 0 printing
# nothing. Three defects stacked into a silent green over a 39-run-red main.
BADAPI=$(mktemp -d)
cat > "$BADAPI/curl" <<'STUB'
#!/usr/bin/env bash
echo '{"message":"Moved Permanently","url":"https://api.github.com/repositories/1158715556"}'
exit 0
STUB
chmod +x "$BADAPI/curl"
out=$(GH_TOKEN=stub PATH="$BADAPI:$PATH" bash "$CI_SH" 2>&1); rc=$?
echo "  unreadable API answer: '$out' (rc=$rc)"
if [ "$rc" -eq 0 ]; then
  fail "swallowed an unreadable answer and exited 0 — silent green over a red main"
else
  pass "an unreadable answer reports unverifiable (rc=$rc), not a silent pass"
fi
rm -rf "$BADAPI"

# The slug must be the repo we actually have, or every query 301s.
if grep -q "gathering-chorus/chorus" "$CI_SH"; then
  pass "queries the repo this actually is"
else
  fail "queries a stale repo slug — GitHub 301s and the check learns nothing"
fi

echo "=== every alert's check must survive the PRODUCTION extractor ==="
# alert-runner.sh:31 extracts a check with:
#   awk '/^check: \|/{found=1;next} /^[a-z]/{if(found) exit} found{print}'
# — it stops at any column-0 lowercase line. ci-main-red embedded python whose
# `import sys, json` sits at column 0, so production cut the script in half,
# mid-string, and what it ran did not parse. It never surfaced because the
# GH_TOKEN-absent early-exit fired before reaching the broken half — the moment
# a token exists, the check dies on a syntax error instead of reading CI.
# A check the runner cannot even parse is the purest form of a blind monitor.
prod_extract(){ awk '/^check: \|/{found=1; next} /^[a-z]/{if(found) exit} found{print}' "$1"; }
for yml in "$ALERTS"/*.yml; do
  name=$(basename "$yml")
  script=$(prod_extract "$yml")
  [ -n "$script" ] || continue   # no check block — not this test's concern
  if printf '%s\n' "$script" | bash -n 2>/dev/null; then
    pass "$name: check parses as extracted by the runner"
  else
    fail "$name: the runner extracts a check that does not parse (truncated block)"
  fi
done

echo "=== the ACTION must receive the check's verdict ==="
# alert-runner.sh runs the action with a bare `bash -c "$action_script"` — a
# fresh shell with no environment. Four alerts interpolate $STATUS into their
# nudge text, so every one of those nudges has been sending an EMPTY verdict.
# The 2026-07-29 00:00 nudge arrived as "ci-main-red:  (quality.yml on main --
# red:<url> means ... unverifiable:GH_TOKEN-absent means ...)" — note the double
# space. Silas read the explanatory legend as if it were the verdict and spent
# two days reasoning from a value the alarm never actually sent. An action that
# cannot see its own check is the purest case of this card's defect class.
RUNNER="$(cd "$(dirname "$0")/../.." && pwd)/scripts/alert-runner.sh"
if grep -qE 'STATUS="?\$(result|\{result)' "$RUNNER"; then
  pass "alert-runner passes the check result into the action as \$STATUS"
else
  fail "alert-runner runs the action with no environment — every \$STATUS nudge sends an empty verdict"
fi
# End-to-end: a check that prints a verdict, an action that echoes $STATUS.
RT=$(mktemp -d)
cat > "$RT/probe.yml" <<'YML'
name: probe-status-passthrough
description: fixture
severity: warning
schedule: "0 0 * * *"

check: |
  echo "verdict-token-42"
  exit 1

action: |
  echo "ACTION_SAW:$STATUS"
YML
# Drive the runner's own extraction + invocation shape rather than re-implementing it.
chk=$(awk '/^check: \|/{f=1;next} /^[a-z]/{if(f)exit} f{print}' "$RT/probe.yml")
act=$(awk '/^action: \|/{f=1;next} /^[a-z]/{if(f)exit} f{print}' "$RT/probe.yml")
result=$(bash -c "$chk" 2>&1) || true
seen=$(STATUS="$result" bash -c "$act" 2>&1)
case "$seen" in
  *verdict-token-42*) pass "an action invoked with STATUS in env sees the verdict" ;;
  *) fail "action could not see the verdict even when passed: '$seen'" ;;
esac
rm -rf "$RT"

echo "=== extracted checks must be valid PYTHON too, not just valid bash ==="
# bash -n cannot see inside a python heredoc. The ci-main-red/startup-sync
# extraction dedented python that had been at a different indent level inside
# the yml, producing an IndentationError that 2>/dev/null swallowed — the check
# still returned the right verdict but its error detail silently became
# 'unknown'. A degraded message is exactly what this card is about.
for sh in "$ALERTS"/*.check.sh; do
  [ -f "$sh" ] || continue
  n=$(basename "$sh")
  # pull each python3 -c "..." body and compile it
  bad=0
  python3 - "$sh" <<'PY' || bad=1
import re, sys, ast
src = open(sys.argv[1]).read()
for m in re.finditer(r'python3 -c "\n(.*?)\n" ', src, re.S):
    body = m.group(1).replace('\\"', '"')
    try:
        ast.parse(body)
    except SyntaxError as e:
        print(f"  python syntax error at line {e.lineno}: {e.msg}", file=sys.stderr)
        raise SystemExit(1)
PY
  [ "$bad" -eq 0 ] && pass "$n: embedded python compiles" || fail "$n: embedded python has a syntax error (message detail silently degrades)"
done

echo
[ "$FAIL" -eq 0 ] && { echo "PASS: fires carry evidence"; exit 0; }
echo "RED: at least one alarm fires without saying what it saw"; exit 1
