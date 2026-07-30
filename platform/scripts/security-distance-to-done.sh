#!/usr/bin/env bash
# security-distance-to-done.sh — #3716: the security program's end-state as
# MEASURED counts from the live system, never card statuses.
#
# DONE means, in full (Jeff's arc: model-driven + automated so we can retire):
#   ONE verify path (ES256/CSS only — no HS256 arm),
#   ONE key model (no shared secret anywhere in live code),
#   identity AND scope resolved from the model (allow-set, holdsRole, scoped
#   ES256 tokens), and the store refusing unauthenticated writes.
# Every item below measures one gap. 0 = closed. A measurement that CANNOT run
# reports "unknown" — never zero. Zero you didn't measure is how fuseki-harvest
# lied for 78 days.
set -uo pipefail

C="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
API="${CHORUS_API_BASE:-http://localhost:3340}"
PROM_OUT="${SECURITY_LEDGER_PROM:-/Users/jeffbridwell/CascadeProjects/shared-observability/data/textfile_collector/security_distance.prom}"


# grep's exit codes carry three meanings and conflating them is this script's own
# disease: 0=matches, 1=ZERO MATCHES (a valid measurement!), >=2=grep itself failed
# (unknown, never zero). count_or_unknown echoes the count or "unknown".
count_or_unknown() {
  local out rc
  out=$("$@" 2>/dev/null); rc=$?
  if [ $rc -le 1 ]; then printf '%s' "${out:-0}"; else printf 'unknown'; fi
}

declare -a NAMES VALUES NOTES
item(){ NAMES+=("$1"); VALUES+=("$2"); NOTES+=("${3:-}"); }

# ── hs256_minters: live code paths that MINT HS256 tokens ────────────────────
# chorus-mint-token.py callers (scripts + rust) plus the two envelope-door
# TS minters. Counted from the tree, tests excluded.
minters_files=$(grep -rl --exclude-dir=target --exclude-dir=node_modules --exclude-dir=dist \
            "chorus-mint-token" "$C/platform/scripts" "$C/platform/services" 2>/dev/null); m_rc=$?
if [ $m_rc -ge 2 ]; then minters=unknown; else minters=$(printf '%s\n' "$minters_files" | grep -v test | grep -c . || true); fi
ts_files=$(grep -rlE 'alg.*HS256|createHmac.*sha256' \
             "$C/platform/chorus-sdk/src" "$C/platform/mcp-server/src" 2>/dev/null); t_rc=$?
if [ $t_rc -ge 2 ]; then ts_minters=unknown; else ts_minters=$(printf '%s\n' "$ts_files" | grep -v test | grep -c . || true); fi
if [ "$minters" = unknown ] || [ "$ts_minters" = unknown ]; then
  item hs256_minters unknown "grep failed — NOT measured"
else
  item hs256_minters $((minters + ts_minters)) "files minting HS256 (goal 0)"
fi

# ── shared_secret_refs: non-test live code reading the shared secret ─────────
ref_files=$(grep -rl --exclude-dir=target --exclude-dir=node_modules --exclude-dir=dist --exclude-dir=dist.prev \
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

# ── scoped_es256_missing: can chorus-identity-token mint a scoped token? ─────
if grep -q '\-\-scope' "$C/platform/scripts/chorus-identity-token" 2>/dev/null; then
  item scoped_es256_missing 0 "scoped mint exists"
else
  item scoped_es256_missing 1 "chorus-identity-token has no --scope (the #3689 rung)"
fi

# ── principals_without_holdsrole: measured live, by-design set NAMED ─────────
# services + guests + jeff hold no role BY DESIGN (#3653/#3688); anything else
# without a role is drift. unknown if the store can't answer.
BYDESIGN="bridge chorus-sdk jeff marknakib crawler-index reindex-worker embed-worker"
rows=$(curl -s -m 15 "http://localhost:3030/pods/query" \
  --data-urlencode 'query=PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?v WHERE { GRAPH ?g { ?p a chorus:Principal . FILTER NOT EXISTS { ?p chorus:holdsRole ?r } BIND(STRAFTER(STR(?p),"#principal-") AS ?v) } }' \
  -H "Accept: text/csv" 2>/dev/null | tail -n +2)
if [ -z "$rows" ] && ! curl -s -m 5 -o /dev/null "http://localhost:3030/pods/query" --data-urlencode 'query=ASK{}' 2>/dev/null; then
  item principals_without_holdsrole unknown "store unreachable — NOT measured"
else
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
board_listing=$("$C/platform/scripts/cards" chunk security 2>/dev/null); cli_rc=$?
if [ $cli_rc -ne 0 ] || ! curl -s -m 5 -o /dev/null "$API/api/chorus/context/health" 2>/dev/null; then
  item open_security_chunk_cards unknown "board/cards CLI unreachable — NOT measured"
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
