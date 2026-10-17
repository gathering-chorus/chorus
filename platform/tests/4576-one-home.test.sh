#!/usr/bin/env bash
# @domain: domains
# @test-type: integration — reads the live store and the class atlas; reports UNMEASURED, never red or green, when either is down.
# @card: #4576
# @owner: wren
#
# #4576 — Jeff 2026-10-17: "we are not clearly separating out our top level chorus and loom
# and security classes". Rule: every domain that defines vocabulary belongs to exactly one
# Product (Product hasDomain), and every class has exactly one claiming domain. A domain with
# 0 or 2 products, or a class claimed twice, is red. CodeFile rows also use hasDomain (a
# different meaning), so only subjects typed chorus:Product count.
set -u
FUSEKI="${CHORUS_FUSEKI:-http://localhost:3030/pods/query}"
ATLAS="${CLASS_ATLAS_URL:-http://localhost:3340/api/athena/class-atlas}"

check() { # atlas.json pairs.csv shaped.csv ephemeral.csv -> prints violations, exit 1 on any
  python3 - "$1" "$2" "${3:-}" "${4:-}" <<'PY'
import csv, json, sys, collections
atlas = json.load(open(sys.argv[1]))
domains = [d["name"] for d in atlas.get("domains", [])]
owners = collections.defaultdict(set)
for row in csv.DictReader(open(sys.argv[2])):
    owners[row["d"].rsplit("#", 1)[-1]].add(row["prod"].rsplit("#", 1)[-1])
claims = collections.defaultdict(list)
for d in atlas.get("domains", []):
    for c in d.get("classes", []):
        claims[c["name"]].append(d["name"])
bad = []
for d in domains:
    n = len(owners.get(d, ()))
    if n != 1:
        bad.append(f"domain {d}: {n} products {sorted(owners.get(d, ()))}")
for c, ds in sorted(claims.items()):
    if len(ds) != 1:
        bad.append(f"class {c}: claimed by {len(ds)} domains {ds}")
# a class with a shape that no domain claims has no home either
# graphs declared test-ephemeral (NamedGraph graphStatus) are a test's leftovers, not a home
ephemeral = []
if len(sys.argv) > 4 and sys.argv[4]:
    ephemeral = [r["iri"] for r in csv.DictReader(open(sys.argv[4]))]
def is_ephemeral(g):
    return any(g == e or (e.endswith("*") and g.startswith(e[:-1])) for e in ephemeral)
if len(sys.argv) > 3 and sys.argv[3]:
    for row in csv.DictReader(open(sys.argv[3])):
        if is_ephemeral(row.get("g", "")):
            continue
        c = row["cls"].rsplit("#", 1)[-1].rsplit("/", 1)[-1]
        if c not in claims:
            bad.append(f"class {c}: has a shape, claimed by no domain")
for b in bad:
    print("  " + b)
print(f"domains {len(domains)}, classes {len(claims)}, violations {len(bad)}")
sys.exit(1 if bad else 0)
PY
}

if [ "${1:-}" = "--self-test" ]; then
  T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
  echo '{"domains":[{"name":"a","classes":[{"name":"X"}]},{"name":"b","classes":[{"name":"Y"}]}]}' > "$T/atlas.json"
  printf 'prod,d\nchorus,a\nloom,b\n' > "$T/ok.csv"
  check "$T/atlas.json" "$T/ok.csv" >/dev/null || { echo "FAIL: one product per domain reported red"; exit 1; }
  # NEGATIVE PROOF: an orphan domain, a doubly-owned domain, and a doubly-claimed class are each red
  printf 'prod,d\nchorus,a\n' > "$T/orphan.csv"
  check "$T/atlas.json" "$T/orphan.csv" >/dev/null && { echo "FAIL NEGATIVE: an orphan domain passed"; exit 1; }
  printf 'prod,d\nchorus,a\nloom,b\nathena,b\n' > "$T/double.csv"
  check "$T/atlas.json" "$T/double.csv" >/dev/null && { echo "FAIL NEGATIVE: a domain with two products passed"; exit 1; }
  echo '{"domains":[{"name":"a","classes":[{"name":"X"}]},{"name":"b","classes":[{"name":"X"}]}]}' > "$T/twice.json"
  check "$T/twice.json" "$T/ok.csv" >/dev/null && { echo "FAIL NEGATIVE: a class claimed twice passed"; exit 1; }
  printf 'cls\nhttps://x#X\nhttps://x#Z\n' > "$T/shaped.csv"
  check "$T/atlas.json" "$T/ok.csv" "$T/shaped.csv" >/dev/null && { echo "FAIL NEGATIVE: a shaped class no domain claims passed"; exit 1; }
  printf 'cls,g\nhttps://x#Z,urn:chorus:verbs-sandbox\n' > "$T/sandbox.csv"
  printf 'iri\nurn:chorus:verbs-sandbox\n' > "$T/eph.csv"
  check "$T/atlas.json" "$T/ok.csv" "$T/sandbox.csv" "$T/eph.csv" >/dev/null || { echo "FAIL: a shape left in a test-ephemeral graph counted as a gap"; exit 1; }
  check "$T/atlas.json" "$T/ok.csv" "$T/sandbox.csv" /dev/null >/dev/null && { echo "FAIL NEGATIVE: the same shape outside an ephemeral graph passed"; exit 1; }
  echo "PASS self-test: orphan, two products, a twice-claimed class and an unclaimed shaped class are each red; test-ephemeral graphs are skipped"; exit 0
fi

T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
curl -sf -m 30 "$ATLAS" > "$T/atlas.json" 2>/dev/null || { echo "UNMEASURED: class atlas not reachable at $ATLAS"; exit 0; }
curl -sf -m 30 -H 'Accept: text/csv' --data-urlencode 'query=PREFIX c:<https://jeffbridwell.com/chorus#> SELECT ?prod ?d WHERE { GRAPH ?g { ?prod a c:Product ; c:hasDomain ?d } }' "$FUSEKI" > "$T/pairs.csv" 2>/dev/null \
  || { echo "UNMEASURED: store not reachable at $FUSEKI"; exit 0; }
curl -sf -m 30 -H 'Accept: text/csv' --data-urlencode 'query=PREFIX sh:<http://www.w3.org/ns/shacl#> SELECT DISTINCT ?cls ?g WHERE { GRAPH ?g { ?s sh:targetClass ?cls } }' "$FUSEKI" > "$T/shaped.csv" 2>/dev/null \
  || { echo "UNMEASURED: store not reachable at $FUSEKI"; exit 0; }
curl -sf -m 30 -H 'Accept: text/csv' --data-urlencode 'query=PREFIX c:<https://jeffbridwell.com/chorus#> SELECT ?iri WHERE { GRAPH ?x { ?n c:graphStatus "test-ephemeral" ; c:graphIri ?iri } }' "$FUSEKI" > "$T/ephemeral.csv" 2>/dev/null \
  || { echo "UNMEASURED: store not reachable at $FUSEKI"; exit 0; }
check "$T/atlas.json" "$T/pairs.csv" "$T/shaped.csv" "$T/ephemeral.csv"
