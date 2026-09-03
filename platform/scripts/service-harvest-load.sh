#!/usr/bin/env bash
# #3870 — service harvester, load phase: put generated instance TTL into the
# dedicated harvested-state graph, ONLY when it differs from what the graph
# already holds. Second run over the same world writes 0 — the live half of
# "idempotent or it isn't a harvest, it's an append" (wren).
#
# Graph name: urn:chorus:domains:services — Silas's OWL-DBA ruling (ADR-051
# domain-named spine, 2026-08-14 16:06). Kept OUT of MODEL_SET staging.
#
# #4089 — the harvester owns its CLASSES, never the graph. It used to PUT-replace
# the whole graph, which wiped every co-tenant each cycle (proven 2026-08-27 —
# the reason Service rows fled to urn:chorus:instances). Jeff, 2026-09-03: a
# hydrator wiping a graph is the defect, not a reason to move rows. Now the write
# is ONE SPARQL UPDATE: delete every triple whose subject is typed as a class the
# harvest emits (rdf:type objects of the generated file), then insert the
# harvest. A hand-authored row of any other class (Commitment, Service, …)
# survives a cycle untouched. `--print-update` prints that update text so the
# proof can apply it to a fixture with jena `update` and watch the co-tenant
# survive — and watch the old wholesale form lose it.
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

GENERATED="" CURRENT="" DRY=0 SERVED="" PRINT_UPDATE=0
while [ $# -gt 0 ]; do
  case "$1" in
    --generated)       GENERATED="$2"; shift 2 ;;
    --current)         CURRENT="$2"; shift 2 ;;
    --dry-run)         DRY=1; shift ;;
    --print-update)    PRINT_UPDATE=1; shift ;;   # #4089 — emit the class-scoped update and stop
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

# #4089 — the classes this harvest OWNS: every rdf:type object in the generated
# file. Only subjects of these types are replaced; nothing else in the graph is
# touched. An empty class set is a refusal, never "delete nothing, insert all".
# DECLARED, not inferred (Wren, 2026-09-03 10:30): if the generator ever emitted
# chorus:Service by mistake, an inferred set would delete the authored Service
# rows silently. The harvester owns exactly these classes; a generated type
# outside the list is a refusal, never a wider delete.
HARVEST_OWNED_CLASSES="${HARVEST_OWNED_CLASSES:-ServiceInstance ScheduledJob MappingStaleness}"
OWNED=""; for c in $HARVEST_OWNED_CLASSES; do OWNED="$OWNED<https://jeffbridwell.com/chorus#$c> "; done
EMITTED="$(grep -E ' <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <' "$WORK/generated.nt" | awk '{print $3}' | sort -u || true)"
[ -n "$EMITTED" ] || { echo "service-harvest-load: generated TTL declares no rdf:type — refusing to write a harvest that owns no class" >&2; exit 2; }
for ty in $EMITTED; do
  case " $OWNED" in *" $ty "*) ;; *)
    echo "service-harvest-load: REFUSE — generated TTL types a subject as $ty, which this harvester does not own (owned: $HARVEST_OWNED_CLASSES)" >&2; exit 2 ;;
  esac
done
owned_filter() { # -> "IN (<c1>, <c2>)"
  printf 'IN (%s)' "$(printf '%s\n' $OWNED | paste -sd, - | sed 's/,/, /g')"
}
# The update: delete the owned-class slice, insert the harvest. One request,
# so a reader never sees the graph half-written.
update_text() {
  printf 'DELETE { GRAPH <%s> { ?s ?p ?o } }\nWHERE  { GRAPH <%s> { ?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ?t ; ?p ?o . FILTER(?t %s) } } ;\nINSERT DATA { GRAPH <%s> {\n' "$GRAPH" "$GRAPH" "$(owned_filter)" "$GRAPH"
  cat "$WORK/generated.nt"
  printf '} }\n'
}
# The slice of the CURRENT graph this harvest owns — the only part it compares
# against. A co-tenant row is not a difference.
owned_slice() { # file(sorted nt) -> stdout: triples whose subject is typed in OWNED
  python3 - "$1" $OWNED <<'PY'
import sys
path, owned = sys.argv[1], set(sys.argv[2:])
lines = open(path).read().splitlines()
typed = set()
for l in lines:
    p = l.split(' ', 3)
    if len(p) >= 3 and p[1] == '<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>' and p[2] in owned:
        typed.add(p[0])
for l in lines:
    if l.split(' ', 1)[0] in typed:
        print(l)
PY
}
if [ "$PRINT_UPDATE" -eq 1 ]; then update_text; exit 0; fi

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
  canon "$CURRENT" > "$WORK/current.all" 2>/dev/null || : > "$WORK/current.all"
else
  HTTP="$(curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" --max-time 15 -H 'Accept: application/n-triples' \
      -o "$WORK/current.raw" -w '%{http_code}' "$FUSEKI/$DATASET/data?graph=$GRAPH" || echo 000)"
  case "$HTTP" in
    200) LC_ALL=C sort "$WORK/current.raw" > "$WORK/current.all" ;;
    404) : > "$WORK/current.all" ;;   # graph doesn't exist yet — an honest empty, first harvest creates it
    *)   echo "service-harvest-load: Fuseki gave $HTTP at $FUSEKI — refusing to treat as empty" >&2; exit 2 ;;
  esac
fi
# #4089 — compare only the owned slice; co-tenants are not ours to diff
owned_slice "$WORK/current.all" | LC_ALL=C sort > "$WORK/current.nt"

if diff -q "$WORK/generated.nt" "$WORK/current.nt" >/dev/null; then
  echo "service-harvest-load: 0 changes — graph already matches harvest, nothing written"
  exit 0
fi

DELTA="$(diff "$WORK/current.nt" "$WORK/generated.nt" | grep -c '^[<>]' || true)"
if [ "$DRY" -eq 1 ]; then
  echo "service-harvest-load: write needed — $DELTA triple line(s) differ (dry-run, nothing written)"
  exit 0
fi

# #4089 — replace the OWNED CLASSES, not the graph: one SPARQL UPDATE
# (delete the owned slice, insert the harvest). Idempotent on the slice; the
# diff above exists so an unchanged world writes nothing at all.
update_text > "$WORK/update.ru"
curl -sf "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" --max-time 30 -X POST -H 'Content-Type: application/sparql-update' \
    --data-binary "@$WORK/update.ru" "$FUSEKI/$DATASET/update" \
  || { echo "service-harvest-load: UPDATE failed" >&2; exit 2; }
echo "service-harvest-load: wrote graph $GRAPH ($DELTA triple line(s) changed; classes: $OWNED)"
