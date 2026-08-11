#!/usr/bin/env bash
# retire-identity-principal-copies.sh — phase 2 of #3785.
#
# Delete the duplicate chorus:Principal records from urn:chorus:domains:identity,
# leaving the ONE home (urn:chorus:domains:security) that the doors enforce and
# owl-api now serves. This is the step that, done wrong on 2026-08-06, locked
# Jeff out of the Clearing — so it is guarded three ways and refuses rather than
# guesses:
#
#   GUARD 1 (superset): every subject about to be deleted from identity MUST
#     also exist in security. If identity holds a Principal that security does
#     not, that record is NOT a duplicate — deleting it would lose the only
#     copy. Refuse the whole run; name the orphan.
#   GUARD 2 (served == home): owl-api MUST already serve from the security home
#     (phase 1 live). If /principals is still on the stale graph, deleting the
#     identity copies pulls the page out from under the running service. Refuse
#     until phase 1 is verified live — the strict-after-phase-1 ordering, enforced
#     in code, not in a comment.
#   GUARD 3 (dry-run default): prints the exact DELETE and the two guard results;
#     writes NOTHING without --apply.
#
# Recovery never depends on this: the records are reproducible from source
# (identity-principals-3613.ttl → SECURITY_SET), so a wrong delete is re-deployable.
set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
# Graph names are env-overridable so a test can bring its own world (#3528) —
# seed a scratch identity+security pair and exercise the real guards against it.
SEC="${RETIRE_SECURITY_GRAPH:-urn:chorus:domains:security}"
IDN="${RETIRE_IDENTITY_GRAPH:-urn:chorus:domains:identity}"
QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
UPDATE="${FUSEKI_UPDATE:-http://localhost:3030/pods/update}"
OWL="${FITNESS_OWL_API:-http://localhost:3360}"
NS="https://jeffbridwell.com/chorus#"
APPLY=0; [ "${1:-}" = "--apply" ] && APPLY=1

# shellcheck disable=SC1091
source "$CHORUS_ROOT/platform/scripts/fuseki-auth.sh"

subjects() {  # subjects <graph> — principal IRIs local-names, sorted
  curl -s -G "$QUERY" --data-urlencode \
    "query=PREFIX chorus: <$NS> SELECT ?p WHERE { GRAPH <$1> { ?p a chorus:Principal } } ORDER BY ?p" \
    -H 'Accept: application/sparql-results+json' \
  | python3 -c 'import json,sys
print("\n".join(sorted(b["p"]["value"] for b in json.load(sys.stdin)["results"]["bindings"])))'
}

IDN_SUBJ="$(subjects "$IDN")"
SEC_SUBJ="$(subjects "$SEC")"
[ -n "$IDN_SUBJ" ] || { echo "retire: identity graph already holds no Principals — nothing to do."; exit 0; }

# GUARD 1 — superset. comm -23 = in identity, NOT in security = orphans.
ORPHANS="$(comm -23 <(printf '%s\n' "$IDN_SUBJ") <(printf '%s\n' "$SEC_SUBJ"))"
if [ -n "$ORPHANS" ]; then
  echo "retire: REFUSING — identity holds Principal(s) not in the security home; deleting would lose the only copy:" >&2
  printf '%s\n' "$ORPHANS" | sed 's/^/  orphan: /' >&2
  exit 1
fi
echo "retire: GUARD 1 ok — all $(printf '%s\n' "$IDN_SUBJ" | wc -l | tr -d ' ') identity Principals also exist in the security home."

# GUARD 2 — served == home. owl-api must already serve from security (phase 1 live).
# Proof by presence: flow-probe exists ONLY in security, so if /principals serves
# it, owl-api is reading the home. If not, phase 1 is not live — refuse.
if ! curl -s --max-time 15 "$OWL/principals" \
     | python3 -c 'import json,sys
d=json.load(sys.stdin); rows=d.get("data",[])
sys.exit(0 if any(r.get("webId","").endswith("/flow-probe/profile/card#me") for r in rows) else 1)'; then
  echo "retire: REFUSING — owl-api /principals does NOT yet serve the security home (flow-probe absent)." >&2
  echo "        Phase 1 is not verified live; retiring now pulls the page out from under the running service." >&2
  exit 2
fi
echo "retire: GUARD 2 ok — owl-api serves the security home (flow-probe present on /principals)."

# The DELETE — every triple whose subject is a Principal in identity, from identity only.
DEL="PREFIX chorus: <$NS>
DELETE WHERE { GRAPH <$IDN> { ?p a chorus:Principal . ?p ?q ?v } }"
echo "retire: transaction (identity graph only, home untouched):"
printf '%s\n' "$DEL" | sed 's/^/  /'

if [ "$APPLY" -ne 1 ]; then
  echo "retire: DRY-RUN — nothing written. Re-run with --apply to retire the identity copies."
  exit 0
fi

code=$(curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -o /dev/null -w '%{http_code}' \
  -X POST -H 'Content-Type: application/sparql-update' --data "$DEL" "$UPDATE")
[ "$code" = "200" ] || [ "$code" = "204" ] || { echo "retire: DELETE failed (http $code)" >&2; exit 1; }

REMAIN="$(subjects "$IDN")"
[ -z "$REMAIN" ] || { echo "retire: POST-CHECK FAILED — identity still holds Principals after delete:" >&2; printf '%s\n' "$REMAIN" >&2; exit 1; }
echo "retire: DONE — identity Principal copies retired; the security home is the one place. Recovery via SECURITY_SET redeploy."
"$CHORUS_ROOT/platform/scripts/chorus-log" security.principal.copies.retired silas graph="$IDN" count="$(printf '%s\n' "$IDN_SUBJ" | wc -l | tr -d ' ')" 2>/dev/null || true
