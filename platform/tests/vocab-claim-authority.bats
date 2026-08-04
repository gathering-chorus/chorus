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
  curl -s -m 5 "http://localhost:3360/__nope__" | python3 -c "
import json,sys
routes=json.load(sys.stdin)['served']
# plural route -> class name: strip slash, singularize the known irregulars
irr={'properties':'Property','propertykeys':'PropertyKey','keyregistryentries':'KeyRegistryEntry','chunkmemberships':'ChunkMembership','testsuiteruns':'TestSuiteRun','valuestreams':'ValueStream','valuestreamsteps':'ValueStreamStep','securityprobes':'SecurityProbe','authboundaries':'AuthBoundary','apisurfaces':'APISurface','emitcontracts':'EmitContract','testresults':'TestResult'}
out=[]
for r in routes:
    r=r.strip('/')
    if r in irr: out.append(irr[r]); continue
    s=r[:-1] if r.endswith('s') else r
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
