#!/usr/bin/env bash
# #3716 — the ledger must MEASURE, and a failed measurement must say unknown.
#
# Every number the ledger emits is either computed from the live system/code or
# reported as unknown. Nothing hardcoded, nothing asserted. This test pins:
#  1. the post-#3719 state (minters deleted, secret refs gone, legacy arm
#     deleted, door resolves model scope) — the ledger must MEASURE these
#     against ITS OWN tree (CHORUS_ROOT pinned below, the #3701 lesson:
#     measure the tree under test, not canonical);
#  2. the unknown-not-zero rule: when a measurement source is unreachable the
#     item reports -1/unknown, never a comforting 0;
#  3. the metric write: gauges land in the textfile collector and parse.
set -uo pipefail
SCRIPT="$(cd "$(dirname "$0")" && pwd)/security-distance-to-done.sh"
# the instrument measures the tree it ships in — FORCE CHORUS_ROOT to that tree
# (an inherited canonical CHORUS_ROOT from the session env is exactly the #3701
# measure-the-wrong-tree bug; no :-default here)
export CHORUS_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
FAIL=0
ok(){ echo "  PASS: $1"; }
no(){ echo "  FAIL: $1"; FAIL=1; }

OUT=$(bash "$SCRIPT" 2>&1); RC=$?
echo "$OUT" | sed 's/^/    | /'

# 1. Known-state pins (the post-#3719 audited facts — the ledger must discover them)
echo "$OUT" | grep -qE "hs256_minters +0 "               && ok "no HS256 minters remain (mint script deleted, TS minters migrated)" || no "still counts HS256 minters (or self-counts): $(echo "$OUT" | grep hs256_minters)"
echo "$OUT" | grep -qE "shared_secret_refs +0 "          && ok "no live shared-secret readers remain"         || no "still counts secret refs (or self-counts): $(echo "$OUT" | grep shared_secret_refs)"
echo "$OUT" | grep -qE "hs256_verify_branch +0 "         && ok "legacy verify arm confirmed deleted"          || no "verify arm item wrong: $(echo "$OUT" | grep hs256_verify_branch)"
echo "$OUT" | grep -qE "model_scope_missing +(0|unknown)" && ok "model-scope item measures the DECIDED mechanism (hasScope at the door)" || no "model_scope_missing wrong: $(echo "$OUT" | grep model_scope_missing)"

# 2. Behavioral check ran (not a config grep): the anon-write item must carry
#    evidence of an actual attempt (http code) or say unknown.
echo "$OUT" | grep -qE "fuseki_anon_write +(0|1) +\(http [0-9]{3}\)|fuseki_anon_write +unknown" \
  && ok "anon-write item is behavioral (carries the observed http code) or honest-unknown" \
  || no "anon-write item is neither measured nor honest: $(echo "$OUT" | grep fuseki_anon_write)"

# 3. Unknown-not-zero for the cards item — under single-request truth (round 6)
#    the CLI's own failure is the only unknown path, so fail the CLI itself.
FAILROOT=$(mktemp -d)
mkdir -p "$FAILROOT/platform/scripts"
C_R="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
for d in services chorus-sdk mcp-server; do ln -s "$C_R/platform/$d" "$FAILROOT/platform/$d"; done
ln -s "$C_R/platform/scripts/chorus-identity-token" "$FAILROOT/platform/scripts/chorus-identity-token" 2>/dev/null || true
printf '#!/usr/bin/env bash\nexit 7\n' > "$FAILROOT/platform/scripts/cards"
chmod +x "$FAILROOT/platform/scripts/cards"
OUT2=$(CHORUS_ROOT="$FAILROOT" bash "$SCRIPT" 2>&1)
rm -rf "$FAILROOT"
echo "$OUT2" | grep -qE "open_security_chunk_cards +unknown" \
  && ok "a failing cards CLI reports unknown, not zero" \
  || no "failing CLI reported a number: $(echo "$OUT2" | grep open_security_chunk_cards)"

# 4. Metric file written and parseable
PROM="${SECURITY_LEDGER_PROM:-/Users/jeffbridwell/CascadeProjects/shared-observability/data/textfile_collector/security_distance.prom}"
[ -s "$PROM" ] && grep -q '^security_distance_to_done{' "$PROM" \
  && ok "gauges written to the textfile collector" \
  || no "no parseable gauges at $PROM"
# unknown must be OMITTED from prom (a gauge can't say unknown; absence is honest)
if grep -qE '^security_distance_to_done\{[^}]*\} -1' "$PROM" 2>/dev/null; then
  no "-1 exported as a gauge — absence is the honest encoding for unknown"
else
  ok "unknown items omitted from metrics rather than exported as fake numbers"
fi


# 5. THE GOAL STATE IS REPORTABLE (Kade r1) — and PROVEN NON-VACUOUSLY (Kade r2):
#    the stub cards CLI writes an invocation marker, so a passing assertion cannot
#    come from the stub never running. Stub returns a chunk listing with ZERO card
#    rows; the item must read exactly 0.
FAKEROOT=$(mktemp -d); trap 'rm -rf "$FAKEROOT"' EXIT
mkdir -p "$FAKEROOT/platform/scripts"
C_REAL="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
for d in services chorus-sdk mcp-server; do ln -s "$C_REAL/platform/$d" "$FAKEROOT/platform/$d"; done
cat > "$FAKEROOT/platform/scripts/cards" <<'STUB'
#!/usr/bin/env bash
touch "$(dirname "$0")/.stub-invoked"
echo "  No context doc yet for chunk:security."
echo ""
echo "Next (0):"
exit 0
STUB
chmod +x "$FAKEROOT/platform/scripts/cards"
ln -s "$C_REAL/platform/scripts/chorus-env-setup.sh" "$FAKEROOT/platform/scripts/chorus-env-setup.sh" 2>/dev/null || true
OUT3=$(CHORUS_ROOT="$FAKEROOT" bash "$SCRIPT" 2>&1)
if [ -f "$FAKEROOT/platform/scripts/.stub-invoked" ]; then
  ok "stub cards CLI was actually invoked (non-vacuous)"
else
  no "stub cards CLI never ran — the goal-state assertion below proves nothing"
fi
echo "$OUT3" | grep -qE "open_security_chunk_cards +0 " \
  && ok "an empty chunk reports 0 — the goal state is reachable" \
  || no "zero cards not reported as 0: $(echo "$OUT3" | grep open_security_chunk_cards)"

# 5b (r2, both peers): shared_secret_refs honors the invariant — grep failure is
#    unknown, not 0. Prove by making $C/platform unreadable to the grep via a
#    fake root whose platform/ is a dangling path for the refs walk only.
OUT4=$(CHORUS_ROOT="/nonexistent-3716-probe" bash "$SCRIPT" 2>&1)
echo "$OUT4" | grep -qE "model_scope_missing +unknown" \
  && ok "unreadable oidc.rs reports unknown, not a claimed-missing mechanism" \
  || no "model_scope_missing fabricates a verdict for an unreadable tree: $(echo "$OUT4" | grep model_scope_missing)"
echo "$OUT4" | grep -qE "shared_secret_refs +unknown" \
  && ok "refs grep against a missing tree reports unknown, not 0" \
  || no "refs still reports a number against a MISSING tree: $(echo "$OUT4" | grep shared_secret_refs)"

# 5c. SINGLE-REQUEST TRUTH (Kade r4): principals must be unknown when THIS query
#     goes unanswered — no separate liveness probe may vouch for it.
OUT5=$(FUSEKI_QUERY_URL="http://localhost:1/pods/query" bash "$SCRIPT" 2>&1)
echo "$OUT5" | grep -qE "principals_without_holdsrole +unknown" \
  && ok "unanswered principals query reports unknown (no probe races it)" \
  || no "principals fabricated a verdict without an answer: $(echo "$OUT5" | grep principals)"

# 6. Refactor-safety (Wren, round 1): the verify-branch check must key on the
#    stable API name, and an unreadable file must be unknown, not 0.
grep -q 'auth::verify_token' "$SCRIPT" && ok "verify-branch keys on the API name, not a signature" || no "verify-branch still matches a full signature"
grep -q 'hs256_verify_branch unknown' "$SCRIPT" && ok "unreadable oidc.rs reports unknown" || no "unreadable oidc.rs would report a false 0"

echo; [ $FAIL -eq 0 ] && { echo "PASS"; exit 0; } || { echo "RED"; exit 1; }
