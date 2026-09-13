#!/bin/bash
# athena-validate — #3846. The post-write conformance sweep over the LIVE graph.
#
# The write door (athena-model) gates a single write: is THIS change conformant?
# It cannot see what's already there. athena-validate sweeps the whole instance
# graph for OLD / BAD data the door never gets to refuse:
#
#   1. RETIRED PREDICATES in use — edges the model retired (inParent, inProduct,
#      hostedBy, belongsTo, back-pointers). Present = stale data from before a
#      model change; the door 409s new ones but old ones linger.
#   2. DANGLING EDGES — an edge whose object IRI is not a subject anywhere: a
#      reference to a node that was deleted/renamed out from under it.
#   3. UNTYPED INSTANCES — a chorus: subject with predicates but no rdf:type:
#      data that exists but belongs to no class the model knows.
#
# Exit 0 = clean. Exit 1 = old/bad data found (report lists each). Read-only.
set -uo pipefail
FUSEKI="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
G="urn:chorus:instances"
NS="https://jeffbridwell.com/chorus#"
Q() { curl -sf --max-time 20 -H "Accept: application/sparql-results+json" --data-urlencode "query=$1" "$FUSEKI" 2>/dev/null; }
count() { echo "$1" | python3 -c 'import sys,json; print(len(json.load(sys.stdin)["results"]["bindings"]))' 2>/dev/null || echo "?"; }
rows()  { echo "$1" | python3 -c 'import sys,json;[print("    "+" ".join(v["value"].split("#")[-1] for v in b.values())) for b in json.load(sys.stdin)["results"]["bindings"][:8]]' 2>/dev/null; }

# #4166 — the sweep's answer has to reach somebody. Best-effort: a failing
# emit must never turn a read-only audit into a failure.
emit_spine() {
  local ev="$1"; shift
  local log="${CHORUS_HOME:-/Users/jeffbridwell/CascadeProjects/chorus}/platform/scripts/chorus-log"
  [ -f "$log" ] || return 0
  bash "$log" "$ev" "${DEPLOY_ROLE:-system}" "$@" >/dev/null 2>&1 || true
}

BAD=0
echo "=== athena-validate — conformance sweep over GRAPH <$G> ==="

# #4166 — REACHABILITY FIRST. Every check below treats a failed query as zero
# rows, so an unreachable store used to walk the whole sweep and print PROVEN
# CLEAN. A dead sweep reading as a healthy graph is the one outcome that makes
# this script worse than not having it. Ask once, up front, and refuse to
# report a number we did not measure.
if ! Q "ASK { }" >/dev/null 2>&1; then
  echo
  echo "UNMEASURED — the store at $FUSEKI did not answer. This is NOT zero issues;"
  echo "nothing was swept. Fix the store, then re-run."
  emit_spine "graph.validate.unmeasured" "endpoint=$FUSEKI"
  exit 2
fi

# 1. retired predicates still in use. This list is the model's OWN retired set
# (athena-product-design.html: 409 retired-predicate) — NOT a guess. inStream /
# inValueStream are CURRENT (steps use them), so they are deliberately absent:
# a check that flags valid predicates as bad is the #3850 hollow-check trap.
RETIRED="inParent inProduct hostedBy belongsTo"
echo "1) retired predicates in use:"
for p in $RETIRED; do
  r=$(Q "PREFIX c: <$NS> SELECT ?s WHERE { GRAPH <$G> { ?s c:$p ?o } } LIMIT 20")
  n=$(count "$r")
  if [ "$n" != "0" ] && [ "$n" != "?" ]; then BAD=$((BAD+n)); echo "  ⚠️  c:$p — $n subject(s)"; rows "$r"; fi
done
[ "$BAD" = "0" ] && echo "  ✅ none"

# 2. dangling edges — object is a chorus: IRI that is never a subject
echo "2) dangling edges (object node does not exist):"
# exclude rdf:type edges: their object is a CLASS, which lives in the ontology
# graph, not the instance graph — so "missing here" is expected, not dangling.
# Only edges to expected-instance nodes that don't exist are real dangling.
RDFTYPE="http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
# display shows 8 examples; the COUNT is uncapped — a capped count reads as
# "covered everything" when it hasn't (the no-silent-caps rule, caught live by
# Jeff 2026-08-14 when "40 issues" was really 20+20 display limits over 310).
DANGLE=$(Q "PREFIX c: <$NS> SELECT ?s ?p ?o WHERE { GRAPH <$G> { ?s ?p ?o . FILTER(?p != <$RDFTYPE>) FILTER(isIRI(?o) && STRSTARTS(STR(?o),\"$NS\")) FILTER NOT EXISTS { ?o ?anyp ?anyo } } } LIMIT 20")
NDALL=$(Q "PREFIX c: <$NS> SELECT (COUNT(*) AS ?n) WHERE { GRAPH <$G> { ?s ?p ?o . FILTER(?p != <$RDFTYPE>) FILTER(isIRI(?o) && STRSTARTS(STR(?o),\"$NS\")) FILTER NOT EXISTS { ?o ?anyp ?anyo } } }")
nd=$(echo "$NDALL" | python3 -c 'import sys,json; print(json.load(sys.stdin)["results"]["bindings"][0]["n"]["value"])' 2>/dev/null || echo "?")
if [ "$nd" != "0" ] && [ "$nd" != "?" ]; then BAD=$((BAD+nd)); echo "  ⚠️  $nd dangling edge(s)"; rows "$DANGLE"; else echo "  ✅ none"; fi

# 3. untyped instances — a chorus: subject with data but no rdf:type
echo "3) untyped instances (data with no class):"
UNTYPED=$(Q "PREFIX c: <$NS> SELECT ?s WHERE { GRAPH <$G> { ?s ?p ?o . FILTER(STRSTARTS(STR(?s),\"$NS\")) FILTER NOT EXISTS { ?s a ?t } } } GROUP BY ?s LIMIT 20")
NUALL=$(Q "PREFIX c: <$NS> SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH <$G> { ?s ?p ?o . FILTER(STRSTARTS(STR(?s),\"$NS\")) FILTER NOT EXISTS { ?s a ?t } } }")
nu=$(echo "$NUALL" | python3 -c 'import sys,json; print(json.load(sys.stdin)["results"]["bindings"][0]["n"]["value"])' 2>/dev/null || echo "?")
if [ "$nu" != "0" ] && [ "$nu" != "?" ]; then BAD=$((BAD+nu)); echo "  ⚠️  $nu untyped subject(s)"; rows "$UNTYPED"; else echo "  ✅ none"; fi

# 4. #3846/ADR-058 — the GOVERNANCE CHECK registry: ADRs/decisions/practices as
# checkable data (chorus:GovernanceCheck in urn:chorus:ontology). Each check's
# checkQuery returns one row per violation; the violation cites its law
# (boundTo). Registry rules: a check missing provenRedOn must NOT gate (a
# check never seen red is not a check, #3734) — reported, skipped, counted
# loud. Zero registered checks is itself a WARN: the registry is the guard's
# target, and a deleted target must never pass vacuously.
echo "4) governance checks (ADR-058 registry):"
CHECKS=$(Q "PREFIX c: <$NS> SELECT ?chk ?q ?sev ?law WHERE { GRAPH <urn:chorus:ontology> { ?chk a c:GovernanceCheck ; c:checkQuery ?q ; c:checkSeverity ?sev ; c:boundTo ?law . OPTIONAL { ?chk c:provenRedOn ?red } BIND(BOUND(?red) AS ?proven) FILTER(?proven) } }")
nchk=$(count "$CHECKS")
UNPROVEN=$(Q "PREFIX c: <$NS> SELECT ?chk WHERE { GRAPH <urn:chorus:ontology> { ?chk a c:GovernanceCheck . FILTER NOT EXISTS { ?chk c:provenRedOn ?d } } }")
nup=$(count "$UNPROVEN")
[ "$nup" != "0" ] && [ "$nup" != "?" ] && echo "  ⚠️  $nup check(s) missing provenRedOn — NOT run (never seen red = not a check)" && rows "$UNPROVEN"
if [ "$nchk" = "0" ] || [ "$nchk" = "?" ]; then
  echo "  ⚠️  0 provable governance checks registered — registry empty or unreachable (vacuous pass refused; not counting as clean)"
else
  echo "$CHECKS" | python3 -c '
import sys, json, urllib.request, urllib.parse
data = json.load(sys.stdin)["results"]["bindings"]
fuseki = "'"$FUSEKI"'"
bad = 0
for b in data:
    chk = b["chk"]["value"].split("#")[-1]
    law = b["law"]["value"].split("#")[-1]
    sev = b["sev"]["value"]
    q = b["q"]["value"]
    req = urllib.request.Request(fuseki, data=urllib.parse.urlencode({"query": q}).encode(),
        headers={"Accept": "application/sparql-results+json"})
    try:
        rows = json.load(urllib.request.urlopen(req, timeout=30))["results"]["bindings"]
    except Exception as e:
        print(f"  ⚠️  {chk}: query FAILED ({e}) — counted as violation, never skipped"); bad += 1; continue
    if rows:
        print(f"  ⚠️  {chk} [{sev}] — {len(rows)} violation(s) of {law}:")
        for r in rows[:8]:
            print("      " + " ".join(v["value"].split("#")[-1] for v in r.values()))
        if sev == "block": bad += len(rows)
    else:
        print(f"  ✅ {chk} — 0 violations ({law})")
print(f"GOVBAD={bad}")
' | tee /tmp/gov-check-out.$$
  GOVBAD=$(grep -o "GOVBAD=[0-9]*" /tmp/gov-check-out.$$ | cut -d= -f2)
  # #4166 — keep the one-home subject names so section 5 can locate them.
  ONE_HOME_SUBJECTS=$(awk '/gc-one-home-per-subject/{f=1;next} /^  (✅|⚠️)/{f=0} f&&/^      /{print $1}' /tmp/gov-check-out.$$ | tr '\n' ' ')
  rm -f /tmp/gov-check-out.$$
  BAD=$((BAD+${GOVBAD:-0}))
fi

# 5. #4166 — name the graphs, not just the subject. "pulse is in 2 graphs" is a
# number; "pulse is in urn:chorus:instances and urn:chorus:domains:pulse" is
# something a person can go and fix. The governance check above reports WHICH
# subjects break one-home-per-subject (ADR-051); this says WHERE they live.
if [ "${GOVBAD:-0}" != "0" ]; then
  echo "5) where the one-home violators actually live:"
  for subj in $ONE_HOME_SUBJECTS; do
    r=$(Q "PREFIX c: <$NS> SELECT DISTINCT ?g WHERE { GRAPH ?g { c:$subj ?p ?o } }")
    gs=$(echo "$r" | python3 -c 'import sys,json;print(", ".join(b["g"]["value"] for b in json.load(sys.stdin)["results"]["bindings"]))' 2>/dev/null)
    [ -n "$gs" ] && echo "  $subj → $gs"
  done
fi

echo
if [ "$BAD" = "0" ]; then
  echo "PROVEN CLEAN — no old/bad data in the instance graph."
  emit_spine "graph.validate.completed" "issues=0" "verdict=clean"
  exit 0
else
  echo "OLD/BAD DATA FOUND — $BAD issue(s). The write door can't reach these; this sweep is how they surface."
  emit_spine "graph.validate.completed" "issues=$BAD" "verdict=dirty"
  # #4166 — reach a person. A count on the stdout of a launchd job nobody opens
  # is the same as not running: the sweep already existed and went unread for
  # weeks. Best-effort; a nudge failure never changes the audit's verdict.
  if [ "${ATHENA_VALIDATE_NUDGE:-1}" = "1" ]; then
    curl -sf --max-time 10 -X POST "${CHORUS_API:-http://localhost:3340}/api/chorus/nudge" \
      -H 'Content-Type: application/json' \
      -d "{\"to\":\"silas\",\"from\":\"system\",\"message\":\"athena-validate: $BAD issue(s) in the graph — see ~/Library/Logs/Chorus/athena-validate.log\"}" \
      >/dev/null 2>&1 || true
  fi
  exit 1
fi
