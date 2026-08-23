#!/usr/bin/env bash
# #3870 — service harvester, load phase: put generated instance TTL into the
# dedicated harvested-state graph, ONLY when it differs from what the graph
# already holds. Second run over the same world writes 0 — the live half of
# "idempotent or it isn't a harvest, it's an append" (wren).
#
# Graph name: urn:chorus:domains:services — Silas's OWL-DBA ruling (ADR-051
# domain-named spine, 2026-08-14 16:06). Authored Services live in
# urn:chorus:ontology (#3675 pin), so the harvester OWNS this graph wholesale:
# PUT-replace is safe by construction. Kept OUT of MODEL_SET staging.
#
# Comparison is CANONICAL (riot → sorted N-Triples), never text: a comment or
# reordering is 0 changes; one changed triple is a write.
#
# Hermetic: --current <file> + --dry-run exercise the whole decision without
# Fuseki (the bats path). Live: current is fetched from the graph, and the
# write is a graph-store PUT (replace, not append). Exit 2 on missing/empty
# inputs or unreachable store — absence never impersonates an empty harvest.
set -euo pipefail

# Fuseki write auth (#3870): the shared auth helper exports FUSEKI_AUTH as a
# curl-arg array; the bash-3.2-safe expansion form below is mandatory (see
# fuseki-auth.sh header). Absent creds → empty array → anonymous, and the
# store's 401 surfaces as the loader's refuse path.
HERE_AUTH="$(cd "$(dirname "$0")" && pwd)/fuseki-auth.sh"
# shellcheck source=/dev/null
[ -f "$HERE_AUTH" ] && source "$HERE_AUTH" || FUSEKI_AUTH=()

RIOT="${RIOT:-/opt/homebrew/Cellar/jena/6.0.0/bin/riot}"
[ -x "$RIOT" ] || RIOT="$(command -v riot || true)"
FUSEKI="${CHORUS_FUSEKI:-http://localhost:3030}"
DATASET="${CHORUS_FUSEKI_DATASET:-pods}"
GRAPH="${CHORUS_HARVEST_GRAPH:-urn:chorus:domains:services}"

GENERATED="" CURRENT="" DRY=0 SERVED=""
while [ $# -gt 0 ]; do
  case "$1" in
    --generated)       GENERATED="$2"; shift 2 ;;
    --current)         CURRENT="$2"; shift 2 ;;
    --dry-run)         DRY=1; shift ;;
    --served-services) SERVED="$2"; shift 2 ;;
    *) echo "service-harvest-load: unknown arg $1" >&2; exit 2 ;;
  esac
done
[ -n "$GENERATED" ] && [ -s "$GENERATED" ] || { echo "service-harvest-load: generated TTL missing or empty" >&2; exit 2; }
[ -n "$RIOT" ] && [ -x "$RIOT" ] || { echo "service-harvest-load: riot not found" >&2; exit 2; }

WORK="$(mktemp -d)"; trap 'rm -rf "$WORK"' EXIT

canon() { # file -> sorted canonical N-Triples on stdout
  "$RIOT" --syntax=turtle --output=ntriples "$1" 2>/dev/null | LC_ALL=C sort
}

canon "$GENERATED" > "$WORK/generated.nt" \
  || { echo "service-harvest-load: generated TTL does not parse" >&2; exit 2; }

# Dangling-edge guard (caught live 2026-08-14; wren: structural, not
# discipline): every deploys target must be a Service the STORE serves.
# Fixture path takes a served-list file; live fetches athena-make /services;
# no source at all = refuse to write edges blind.
DEPLOY_TARGETS="$(grep -E 'chorus#runsService>' "$WORK/generated.nt" | grep -oE 'chorus#service-[A-Za-z0-9-]+' | sed 's/.*chorus#//' | sort -u || true)"
if [ -n "$DEPLOY_TARGETS" ]; then
  if [ -n "$SERVED" ]; then
    SERVED_LIST="$(cat "$SERVED")"
  else
    SERVED_LIST="$(curl -sf --max-time 10 "${CHORUS_OWL_API:-http://localhost:3360}/services" \
      | python3 -c 'import json,sys
for r in json.load(sys.stdin).get("data", []):
    n = r.get("name","")
    print(n if n.startswith("service-") else "service-" + n)' 2>/dev/null)" \
      || { echo "service-harvest-load: cannot fetch served Services — refusing to write deploys edges blind" >&2; exit 2; }
  fi
  UNSERVED=""
  for t in $DEPLOY_TARGETS; do
    printf '%s\n' "$SERVED_LIST" | grep -qx "$t" || UNSERVED="$UNSERVED $t"
  done
  if [ -n "$UNSERVED" ]; then
    echo "service-harvest-load: REFUSE — deploys targets unserved by the store:$UNSERVED" >&2
    exit 2
  fi
fi

if [ -n "$CURRENT" ]; then
  canon "$CURRENT" > "$WORK/current.nt" 2>/dev/null || : > "$WORK/current.nt"
else
  HTTP="$(curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" --max-time 15 -H 'Accept: application/n-triples' \
      -o "$WORK/current.raw" -w '%{http_code}' "$FUSEKI/$DATASET/data?graph=$GRAPH" || echo 000)"
  case "$HTTP" in
    200) LC_ALL=C sort "$WORK/current.raw" > "$WORK/current.nt" ;;
    404) : > "$WORK/current.nt" ;;   # graph doesn't exist yet — an honest empty, first harvest creates it
    *)   echo "service-harvest-load: Fuseki gave $HTTP at $FUSEKI — refusing to treat as empty" >&2; exit 2 ;;
  esac
fi

if diff -q "$WORK/generated.nt" "$WORK/current.nt" >/dev/null; then
  echo "service-harvest-load: 0 changes — graph already matches harvest, nothing written"
  exit 0
fi

DELTA="$(diff "$WORK/current.nt" "$WORK/generated.nt" | grep -c '^[<>]' || true)"
if [ "$DRY" -eq 1 ]; then
  echo "service-harvest-load: write needed — $DELTA triple line(s) differ (dry-run, nothing written)"
  exit 0
fi

# Replace the graph wholesale — PUT is idempotent by construction; the diff
# above exists so an unchanged world writes nothing at all.
curl -sf "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" --max-time 30 -X PUT -H 'Content-Type: text/turtle' \
    --data-binary "@$GENERATED" "$FUSEKI/$DATASET/data?graph=$GRAPH" \
  || { echo "service-harvest-load: PUT failed" >&2; exit 2; }
echo "service-harvest-load: wrote graph $GRAPH ($DELTA triple line(s) changed)"
