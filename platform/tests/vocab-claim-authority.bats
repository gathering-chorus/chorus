#!/usr/bin/env bats
# @test-type: integration — queries the LIVE Fuseki store + owl-api; RUN_INTEGRATION-gated (TEST.md two-mode contract)
load test_helper
# vocab-claim-authority.bats — #3727: one placement authority per served class.
#
# Wren's #3718 placement gate derives where an instance belongs from the class's
# chorus:definesVocabulary claim. A SERVED class claimed by TWO domains makes
# that derivation ambiguous — the exact defect the 2026-08-04 audit found live:
# chorus:events (a live-only relic, no MODEL_SET source) duplicated chorus:spine's
# claims on EmitContract/EventCategory/Vertebra, and EmitContract is served.
#
# Negative proof (#3734): this suite was written BEFORE the relic retirement and
# observed RED on the live duplicates; the retirement turned it green. It stays
# as the regression lock — any future double-claim on a served class goes red.

setup() {
  [ "${RUN_INTEGRATION:-}" = "true" ] || skip "integration (live Fuseki/owl-api) — RUN_INTEGRATION=true to run"
  source "$CHORUS_ROOT/platform/scripts/fuseki-auth.sh"
  [ -n "${FUSEKI_ADMIN_PASSWORD:-}" ] || skip "no Fuseki credential in this environment"
}

# Served classes: owl-api's own route table (the error envelope lists them —
# the STORE/route truth, never a source file; DECLARED⊃CLAIMED⊃SERVED).
served_classes() {
  # #4065 — SERVED_ROUTES_JSON is the fixture seam (#3528): a proof feeds the
  # route list directly instead of asking the live generator.
  { if [ -n "${SERVED_ROUTES_JSON:-}" ]; then printf '%s' "$SERVED_ROUTES_JSON"; else curl -s -m 5 "http://localhost:3360/__nope__"; fi; } | python3 -c "
import json,sys
routes=json.load(sys.stdin)['served']
# plural route -> class name: strip slash, singularize the known irregulars
irr={'properties':'Property','propertykeys':'PropertyKey','keyregistryentries':'KeyRegistryEntry','chunkmemberships':'ChunkMembership','testsuiteruns':'TestSuiteRun','valuestreams':'ValueStream','valuestreamsteps':'ValueStreamStep','securityprobes':'SecurityProbe','authboundaries':'AuthBoundary','apisurfaces':'APISurface','emitcontracts':'EmitContract','testresults':'TestResult'}
out=[]
for r in routes:
    r=r.strip('/')
    if r in irr: out.append(irr[r]); continue
    # #4065 — English plurals: memories -> Memory, not 'Memorie' (that misread
    # made a claimed class report as served-but-unclaimed every night).
    if r.endswith('ies'): s=r[:-3]+'y'
    elif r.endswith('s'): s=r[:-1]
    else: s=r
    out.append(s.capitalize())
print('\n'.join(out))"
}

@test "no SERVED class has more than one claiming domain (placement authority is unambiguous)" {
  claims=$(curl -s -u "$FUSEKI_ADMIN_USER:$FUSEKI_ADMIN_PASSWORD" "http://localhost:3030/pods/sparql" \
    --data-urlencode 'query=SELECT ?domain ?class WHERE { GRAPH ?g { ?domain ?p ?class . FILTER(STRENDS(STR(?p),"definesVocabulary")) } }' \
    -H "Accept: application/sparql-results+json")
  dupes=$(SERVED="$(served_classes)" python3 -c "
import json,sys,os,collections
claims=json.loads('''$claims''')['results']['bindings']
served={s.strip().lower() for s in os.environ['SERVED'].split()}
short=lambda v: v.rsplit('/',1)[-1].rsplit('#',1)[-1]
by=collections.defaultdict(set)
for r in claims: by[short(r['class']['value'])].add(short(r['domain']['value']))
for cls,doms in sorted(by.items()):
    if len(doms)>1 and cls.lower() in served:
        print(f'{cls}: {sorted(doms)}')")
  echo "double-claimed served classes: ${dupes:-none}"
  [ -z "$dupes" ]
}

@test "no served class is unclaimed (served ⊆ claimed — structural, but verify the store)" {
  claims=$(curl -s -u "$FUSEKI_ADMIN_USER:$FUSEKI_ADMIN_PASSWORD" "http://localhost:3030/pods/sparql" \
    --data-urlencode 'query=SELECT DISTINCT ?class WHERE { GRAPH ?g { ?domain ?p ?class . FILTER(STRENDS(STR(?p),"definesVocabulary")) } }' \
    -H "Accept: application/sparql-results+json")
  missing=$(SERVED="$(served_classes)" python3 -c "
import json,sys,os
claimed={r['class']['value'].rsplit('/',1)[-1].rsplit('#',1)[-1].lower() for r in json.loads('''$claims''')['results']['bindings']}
for s in os.environ['SERVED'].split():
    if s.strip().lower() not in claimed: print(s)")
  echo "served-but-unclaimed: ${missing:-none}"
  [ -z "$missing" ]
}

# #4065 — the singularizer is a check that gates a verdict, so it ships with
# proofs (#3734). Positive: the -ies plural that made a claimed class read as
# unclaimed every night now names the class the store claims. Negative: a
# served route with no claim still FAILS — the fix did not widen into
# "everything is claimed".
@test "singularizer: /memories is Memory (positive control) and irregulars hold" {
  out=$(SERVED_ROUTES_JSON='{"served":["/memories","/properties","/products","/authboundaries"]}' served_classes)
  [[ "$out" == *"Memory"* ]] || { echo "expected Memory, got: $out" >&2; return 1; }
  [[ "$out" != *"Memorie"* ]] || { echo "still produces Memorie: $out" >&2; return 1; }
  [[ "$out" == *"Property"* && "$out" == *"Product"* && "$out" == *"AuthBoundary"* ]] || { echo "$out" >&2; return 1; }
}

@test "NEGATIVE PROOF: a served route nothing claims is still reported unclaimed" {
  claims=$(curl -s -u "$FUSEKI_ADMIN_USER:$FUSEKI_ADMIN_PASSWORD" "http://localhost:3030/pods/sparql" \
    --data-urlencode 'query=SELECT DISTINCT ?class WHERE { GRAPH ?g { ?domain ?p ?class . FILTER(STRENDS(STR(?p),"definesVocabulary")) } }' \
    -H "Accept: application/sparql-results+json")
  missing=$(SERVED="$(SERVED_ROUTES_JSON='{"served":["/memories","/nosuchwidgets"]}' served_classes)" python3 -c "
import json,sys,os
claimed={r['class']['value'].rsplit('/',1)[-1].rsplit('#',1)[-1].lower() for r in json.loads('''$claims''')['results']['bindings']}
for s in os.environ['SERVED'].split():
    if s.strip().lower() not in claimed: print(s)")
  [[ "$missing" == *"Nosuchwidget"* ]] || { echo "unclaimed route was not reported: '$missing'" >&2; return 1; }
  [[ "$missing" != *"Memory"* ]] || { echo "Memory reported unclaimed: '$missing'" >&2; return 1; }
}
