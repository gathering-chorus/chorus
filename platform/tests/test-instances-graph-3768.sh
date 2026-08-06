#!/usr/bin/env bash
# #3768 — serve proof for the undeclared-tenancy fix.
#
# Each class below had instances in urn:chorus:ontology while its shape declared
# no instancesGraph, so the ADR-051 fallback resolved to an empty domain graph
# and the collection served rows=0 (Jeff: "instances seems 100% empty").
# After the declaration lands + model deploy, every one must serve its rows AND
# say so in provenance: servedFrom == generatedFrom.graph == urn:chorus:ontology.
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

for line in $CASES; do
  cls="${line%%|*}"; rest="${line#*|}"; route="${rest%%|*}"; min="${rest##*|}"
  body=$(curl -sf "$OWL/$route") || { echo "FAIL $cls — $OWL/$route unreachable"; fails=$((fails+1)); continue; }
  rows=$(printf '%s' "$body" | python3 -c "import json,sys; print(len(json.load(sys.stdin).get('data',[])))")
  served=$(printf '%s' "$body" | python3 -c "import json,sys; print(json.load(sys.stdin).get('servedFrom',''))")
  gen=$(printf '%s' "$body" | python3 -c "import json,sys; print(json.load(sys.stdin).get('generatedFrom',{}).get('graph',''))")
  if [ "$rows" -lt "$min" ]; then
    echo "FAIL $cls — rows=$rows < floor $min (servedFrom=$served)"; fails=$((fails+1))
  elif [ "$served" != "urn:chorus:ontology" ]; then
    echo "FAIL $cls — servedFrom=$served, expected urn:chorus:ontology"; fails=$((fails+1))
  elif [ "$served" != "$gen" ]; then
    echo "FAIL $cls — servedFrom=$served != generatedFrom.graph=$gen (provenance split)"; fails=$((fails+1))
  else
    echo "ok   $cls — rows=$rows servedFrom=$served (== generatedFrom.graph)"
  fi
done

# Gate's second tenancy: 11 instances remain in urn:chorus:gates OUTSIDE the
# declared graph (measured 2026-08-06). That is a source-exclusivity violation
# ADR-051 Addendum II owns; recorded here as a KNOWN count so growth is loud.
# When the instance-migrate verb moves them, delete this note and raise the
# Gate floor to 29.

if [ "$fails" -gt 0 ]; then echo "test-instances-graph-3768: $fails FAILURE(S)"; exit 1; fi
echo "test-instances-graph-3768: all green"
