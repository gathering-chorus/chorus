#!/usr/bin/env bash
# security-distance-to-done.sh — #3716: the security program's end-state as
# MEASURED counts from the live system, never card statuses.
#
# DONE means, in full (Jeff's arc: model-driven + automated so we can retire):
#   ONE verify path (ES256/CSS only — no HS256 arm),
#   ONE key model (no shared secret anywhere in live code),
#   identity AND scope resolved from the model (allow-set, holdsRole,
#   chorus:hasScope grants — #3689 decided scope is MODEL DATA, not a token
#   claim), and the store refusing unauthenticated writes.
#
# INSTRUMENT EXCLUSION (#3719): this script and its test measure the system —
# they are not part of it. Their own occurrences of the measured strings are
# excluded, else the ledger counts itself and the distance can never reach 0
# (the observer effect Jeff caught at 42).
# Every item below measures one gap. 0 = closed. A measurement that CANNOT run
# reports "unknown" — never zero. Zero you didn't measure is how fuseki-harvest
# lied for 78 days.
set -uo pipefail

C="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
API="${CHORUS_API_BASE:-http://localhost:3340}"
PROM_OUT="${SECURITY_LEDGER_PROM:-/Users/jeffbridwell/CascadeProjects/shared-observability/data/textfile_collector/security_distance.prom}"



declare -a NAMES VALUES NOTES
item(){ NAMES+=("$1"); VALUES+=("$2"); NOTES+=("${3:-}"); }

# ── hs256_minters: live code paths that MINT HS256 tokens ────────────────────
# UNION of (files referencing the chorus-mint-token minter) and (TS HS256
# minters), DEDUPED — token.ts references the py minter in a comment and must
# count once (Kade/Wren round 4-5 lineage). The test filter matches TEST FILES
# (test-*.sh, *.test.ts, /tests/ dirs), NOT any path containing "test" — the
# naive filter silently excluded werk-test/src/main.rs, a REAL minter, for the
# entire life of this script. chorus-mint-token.py itself counts: it is the
# artifact whose deletion the goal state requires.
a_files=$(grep -rl --exclude-dir=target --exclude-dir=node_modules --exclude-dir=dist \
          --exclude=security-distance-to-done.sh --exclude=test-security-distance-to-done.sh \
          "chorus-mint-token" "$C/platform" 2>/dev/null); a_rc=$?
b_files=$(grep -rlE 'alg.*HS256|createHmac.*sha256' \
          "$C/platform/chorus-sdk/src" "$C/platform/mcp-server/src" 2>/dev/null); b_rc=$?
if [ $a_rc -ge 2 ] || [ $b_rc -ge 2 ]; then
  item hs256_minters unknown "grep failed — NOT measured"
else
  # SOURCE FILES ONLY: a minter is code. The unrestricted walk swept
  # platform/logs/*.log and pulse/messages.db — runtime artifacts that MENTION
  # the minter in logged command lines. (Found because the interactive shell's
  # grep is ugrep with --ignore-files, which skipped them, while this script's
  # plain grep did not — the verify environment differed from the runtime one,
  # the #3713 lesson again.)
  minter_list=$(printf '%s\n%s\n' "$a_files" "$b_files" | sort -u \
    | grep -E '\.(sh|py|rs|ts|js)$' \
    | grep -vE '(^|/)(tests?)/|/test-[^/]*$|\.test\.[a-z]+$|\.bats$' | grep -c . || true)
  item hs256_minters "$minter_list" "deduped union incl. werk-test + the mint script itself (goal 0)"
fi
# ── shared_secret_refs: non-test live code reading the shared secret ─────────
ref_files=$(grep -rl --exclude-dir=target --exclude-dir=node_modules --exclude-dir=dist --exclude-dir=dist.prev \
       --exclude=security-distance-to-done.sh --exclude=test-security-distance-to-done.sh \
       "CHORUS_SERVICE_TOKEN_SECRET" "$C/platform" 2>/dev/null); r_rc=$?
if [ $r_rc -ge 2 ]; then
  item shared_secret_refs unknown "grep failed — NOT measured"
else
  refs=$(printf '%s\n' "$ref_files" | grep -vE "test|\.md$" | grep -c . || true)
  item shared_secret_refs "$refs" "files reading CHORUS_SERVICE_TOKEN_SECRET (goal 0)"
fi

# ── hs256_verify_branch: the dual-verify arm still present? ──────────────────
# Matches the legacy entry-point NAME (auth::verify_token), not a full signature —
# a renamed argument must not silently flip this to 0 (Wren, #3716 review). An
# unreadable file is unknown, never a claimed-deleted arm.
OIDC_RS="$C/platform/services/chorus-oidc/src/oidc.rs"
if [ ! -r "$OIDC_RS" ]; then
  item hs256_verify_branch unknown "oidc.rs unreadable — NOT measured"
elif grep -q "auth::verify_token" "$OIDC_RS"; then
  item hs256_verify_branch 1 "verify_any still dual-verifies (goal 0)"
else
  item hs256_verify_branch 0 "legacy arm deleted (no auth::verify_token call remains)"
fi

# ── model_scope_missing: does the door resolve scope from the MODEL? ─────────
# REDEFINED by #3689/#3719: the old item measured a --scope mint flag we decided
# NEVER to build (CSS can't issue scoped client_credentials — spiked live; a
# self-declared scope claim was the disease). The decided mechanism is
# chorus:hasScope grants resolved at the door. Two legs, both required:
# the resolver in the verify path (code) AND live grant edges (model).
OIDC_RS="$C/platform/services/chorus-oidc/src/oidc.rs"
if [ ! -r "$OIDC_RS" ]; then
  item model_scope_missing unknown "oidc.rs unreadable — NOT measured"
elif ! grep -q "resolve_principal_scopes" "$OIDC_RS"; then
  item model_scope_missing 1 "door has no chorus:hasScope resolver (the #3689 mechanism)"
else
  # single-request truth: the CSV header proves THIS query was answered
  sresp=$(curl -s -m 15 "${FUSEKI_QUERY_URL:-http://localhost:3030/pods/query}" \
    --data-urlencode 'query=PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT (COUNT(?s) AS ?n) WHERE { GRAPH <urn:chorus:domains:security> { ?p chorus:hasScope ?s } }' \
    -H "Accept: text/csv" 2>/dev/null)
  if ! printf '%s' "$sresp" | head -1 | grep -q '^n'; then
    item model_scope_missing unknown "no CSV header — grant query not answered"
  else
    edges=$(printf '%s\n' "$sresp" | sed -n '2p' | tr -dc '0-9')
    if [ "${edges:-0}" -gt 0 ]; then
      item model_scope_missing 0 "door resolves chorus:hasScope; $edges live grant edges"
    else
      item model_scope_missing 1 "resolver present but ZERO grant edges in the security graph"
    fi
  fi
fi

# ── principals_without_holdsrole: measured live, by-design set NAMED ─────────
# services + guests + jeff hold no role BY DESIGN (#3653/#3688); anything else
# without a role is drift. unknown if the store can't answer.
BYDESIGN="bridge chorus-sdk jeff marknakib crawler-index reindex-worker embed-worker"
# SINGLE-REQUEST TRUTH (Kade, round 4): the answer's own CSV header is the proof
# the store answered THIS query. A separate liveness probe races — the rows query
# can fail while the probe succeeds a moment later, and empty-rows would read as
# drift=0: a false zero through a timing gap. Same family as verify-against-git-
# never-a-pin. FUSEKI_QUERY_URL override exists for tests.
FQ="${FUSEKI_QUERY_URL:-http://localhost:3030/pods/query}"
resp=$(curl -s -m 15 "$FQ" \
  --data-urlencode 'query=PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?v WHERE { GRAPH ?g { ?p a chorus:Principal . FILTER NOT EXISTS { ?p chorus:holdsRole ?r } BIND(STRAFTER(STR(?p),"#principal-") AS ?v) } }' \
  -H "Accept: text/csv" 2>/dev/null)
if ! printf '%s' "$resp" | head -1 | grep -q '^v'; then
  item principals_without_holdsrole unknown "no CSV header in response — THIS query was not answered"
else
  rows=$(printf '%s\n' "$resp" | tail -n +2)
  drift=0; names=""
  while read -r p; do
    p="${p%$'\r'}"   # curl CSV rows end \r\n; unstripped, every name misses the match
    [ -z "$p" ] && continue
    case " $BYDESIGN " in *" $p "*) ;; *) drift=$((drift+1)); names="$names $p";; esac
  done <<< "$rows"
  item principals_without_holdsrole "$drift" "outside the by-design set:${names:- none}"
fi

# ── fuseki_anon_write: BEHAVIORAL — attempt an unauthenticated write ─────────
# A refused write (401/403) = closed. Anything 2xx = the hole is open. The
# attempt targets a scratch graph and, if it ever succeeds, deletes only its
# own marker triple.
code=$(curl -s -o /dev/null -m 8 -w "%{http_code}" -X POST \
  "http://localhost:3030/pods/update" \
  --data-urlencode 'update=INSERT DATA { GRAPH <urn:chorus:scratch:3716-probe> { <urn:probe:3716> <urn:probe:ts> "probe" } }' 2>/dev/null)
case "$code" in
  401|403) item fuseki_anon_write 0 "(http $code) anon write refused" ;;
  2*)      item fuseki_anon_write 1 "(http $code) ANON WRITE ACCEPTED — hole open"
           curl -s -m 8 -X POST "http://localhost:3030/pods/update" \
             --data-urlencode 'update=DROP GRAPH <urn:chorus:scratch:3716-probe>' >/dev/null 2>&1 || true ;;
  *)       item fuseki_anon_write unknown "(http ${code:-none}) store did not answer — NOT measured" ;;
esac

# ── open_security_chunk_cards: the board's remaining ladder ──────────────────
# grep -c exits 1 on ZERO matches — which is the GOAL STATE. Only a failed cards
# CLI or unreachable board is unknown (Kade, #3716 review: the ledger must be able
# to report its own success).
# Single-request truth, symmetric case (Kade, round 6): the cards CLI's own exit
# code is the only witness. The curl health-check that used to sit here could
# drop a VALID measurement to unknown on an unrelated transient — the round-4
# timing-gap race, mirrored.
board_listing=$("$C/platform/scripts/cards" chunk security 2>/dev/null); cli_rc=$?
if [ $cli_rc -ne 0 ]; then
  item open_security_chunk_cards unknown "cards CLI failed (rc=$cli_rc) — NOT measured"
else
  cards=$(printf '%s\n' "$board_listing" | grep -cE "^  [0-9]{3,4} " || true)
  item open_security_chunk_cards "$cards" "open cards tagged chunk:security (goal 0)"
fi

# ── render ───────────────────────────────────────────────────────────────────
echo "SECURITY — distance to done ($(date '+%Y-%m-%d %H:%M'))"
echo "DONE = one verify path · one key model · no shared secret · identity+scope from the model"
printf "%-30s %-9s %s\n" "item" "value" "evidence"
total=0; unknowns=0
for i in "${!NAMES[@]}"; do
  printf "%-30s %-9s %s\n" "${NAMES[$i]}" "${VALUES[$i]}" "${NOTES[$i]}"
  case "${VALUES[$i]}" in
    unknown) unknowns=$((unknowns+1)) ;;
    *) total=$((total + VALUES[$i])) ;;
  esac
done
echo "distance: $total (+ $unknowns unmeasured)"

# ── metrics: gauges for measured items; UNKNOWN IS OMITTED (absence is the
#    honest encoding — a fake number in a gauge is the disease) ───────────────
# security.ledger.run — the ledger's own execution is observable (Wren, round 4).
# Best-effort: a missing chorus-log must never break a measurement run.
command -v chorus-log >/dev/null 2>&1 && \
  chorus-log security.ledger.run silas distance="$total" unmeasured="$unknowns" >/dev/null 2>&1 || true

TMP="${PROM_OUT}.tmp"
mkdir -p "$(dirname "$PROM_OUT")"
{
  echo "# HELP security_distance_to_done Remaining gaps to the security program's end-state. 0=closed. Unknown items are omitted."
  echo "# TYPE security_distance_to_done gauge"
  for i in "${!NAMES[@]}"; do
    [ "${VALUES[$i]}" = "unknown" ] && continue
    echo "security_distance_to_done{item=\"${NAMES[$i]}\"} ${VALUES[$i]}"
  done
} > "$TMP" && mv "$TMP" "$PROM_OUT"
