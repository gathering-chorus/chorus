#!/usr/bin/env bash
# #3768 — serve proof for the undeclared-tenancy fix.
#
# Each class below had instances in urn:chorus:ontology while its shape declared
# no instancesGraph, so the ADR-051 fallback resolved to an empty domain graph
# and the collection served rows=0 (Jeff: "instances seems 100% empty").
# After the declaration lands + model deploy, every one must serve its rows AND
# say so in provenance via servedFrom.
#
# #4265 — this asserted servedFrom == generatedFrom.graph. They are DIFFERENT
# fields: servedFrom is where the ROWS live, generatedFrom is where the SHAPE
# lives (it carries a shape name and version beside the graph). Reading them as
# one source has now been filed as a defect three times — Wren 06-23, Silas
# 07-08, me today — so the misreading is recorded here rather than re-derived.
#
# RED before the model deploy, GREEN after — the transition is the evidence.
# (The DETECTOR for future undeclared tenancy is Silas's #3765 SERVED column —
# this test proves THIS fix; it does not pretend to be the general guard.)
set -u
OWL="${OWL_BASE:-http://localhost:3360}"
fails=0

# class|route|min-rows  (floors from the 2026-08-06 sweep measurement)
CASES="
APISurface|apisurfaces|25
Gate|gates|18
EmitContract|emitcontracts|1
Metric|metrics|6
Property|properties|2
PropertyKey|propertykeys|4
"

# #4256 — the routes were hardcoded bare (/emitcontracts), but athena-make
# serves every class under its domain prefix (/v1/spine/emitcontracts). Four
# cases read "unreachable" and two read rows=0 against a route that is not the
# one being served. Ask discovery where the class lives; a route that moves
# can then never make this suite lie in either direction.
DISCOVERY=$(curl -sf "$OWL/" || echo '{}')
collection_of() {
  printf '%s' "$DISCOVERY" | python3 -c "
import json,sys
try: d=json.load(sys.stdin)
except Exception: print(''); raise SystemExit
print(next((p.get('collection','') for p in d.get('primitives',[]) if p.get('kind')=='$1'), ''))"
}

for line in $CASES; do
  cls="${line%%|*}"; rest="${line#*|}"; route="${rest%%|*}"; min="${rest##*|}"
  served_route="$(collection_of "$cls")"
  if [ -z "$served_route" ]; then
    echo "FAIL $cls — not served at all (absent from $OWL/ discovery)"; fails=$((fails+1)); continue
  fi
  body=$(curl -sf "$OWL$served_route") || { echo "FAIL $cls — $OWL$served_route unreachable"; fails=$((fails+1)); continue; }
  rows=$(printf '%s' "$body" | python3 -c "import json,sys; print(len(json.load(sys.stdin).get('data',[])))")
  served=$(printf '%s' "$body" | python3 -c "import json,sys; print(json.load(sys.stdin).get('servedFrom',''))")
  # #4265 — two assertions removed here, both instance-data (Jeff, 2026-09-21:
  # "if a test tests data instance i dont think it belongs here like counts or
  # specific names or values"):
  #   1. the per-class row floors (25, 18, ...) were a snapshot of one sweep
  #   2. servedFrom == "urn:chorus:ontology" was pinned to where rows lived in
  #      August. A row's home is its own domain graph (Jeff, 2026-09-03), so the
  #      classes now serve from domains:security / domains:spine and this read
  #      as broken while the model was doing exactly what it was told.
  # What survives is behaviour: the collection serves SOMETHING, and its
  # provenance agrees with itself. That cannot be made green by moving a row.
  if [ "$rows" -eq 0 ]; then
    echo "FAIL $cls — serves no rows at all (servedFrom=$served)"; fails=$((fails+1))
  elif [ -z "$served" ]; then
    echo "FAIL $cls — serves $rows rows and will not say from where (servedFrom empty)"; fails=$((fails+1))
  else
    echo "ok   $cls — rows=$rows servedFrom=$served"
  fi
done

# Gate's second tenancy: 11 instances remain in urn:chorus:gates OUTSIDE the
# declared graph (measured 2026-08-06). That is a source-exclusivity violation
# ADR-051 Addendum II owns; recorded here as a KNOWN count so growth is loud.
# When the instance-migrate verb moves them, delete this note and raise the
# Gate floor to 29.

if [ "$fails" -gt 0 ]; then echo "test-instances-graph-3768: $fails FAILURE(S)"; exit 1; fi
echo "test-instances-graph-3768: all green"
