#!/usr/bin/env bash
# @domain: domains
# @test-type: integration — reads the store this code's model was deployed to (see pick_store); reports UNMEASURED, never red or green, when it is down or another card owns staging.
# @card: #4576
# @owner: wren
#
# #4576 — Jeff 2026-10-17: "we are not clearly separating out our top level chorus and loom
# and security classes". Rule: every domain that defines vocabulary belongs to exactly one
# Product (Product hasDomain), and every class has exactly one claiming domain. A domain with
# 0 or 2 products, or a class claimed twice, is red. CodeFile rows also use hasDomain (a
# different meaning), so only subjects typed chorus:Product count.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
STAMP_Q='query=SELECT ?c WHERE { GRAPH <urn:chorus:ontology> { <urn:chorus:model-deploy> <urn:chorus:vocab#deployedFromCommit> ?c } }'

# #4576 (Silas 14:38) — which store holds this code's model, from the deploy's own stamp:
# canonical/nightly (HEAD in origin/main) reads prod; a card werk reads staging only when
# staging's stamp is in its own history, else another card owns staging: UNMEASURED.
pick_store() {
  [ -n "${CHORUS_FUSEKI:-}" ] && { echo "$CHORUS_FUSEKI"; return; }
  if git -C "$ROOT" merge-base --is-ancestor HEAD origin/main 2>/dev/null; then
    echo "http://localhost:3030/pods/query"; return
  fi
  local stamp
  stamp=$(curl -sf -m 15 -H 'Accept: text/csv' --data-urlencode "$STAMP_Q" http://localhost:3030/staging/query 2>/dev/null | sed -n 2p | tr -d '"\r')
  if [ -n "$stamp" ] && git -C "$ROOT" merge-base --is-ancestor "$stamp" HEAD 2>/dev/null; then
    echo "http://localhost:3030/staging/query"; return
  fi
  echo ""
}

check() { # claims.csv pairs.csv shaped.csv ephemeral.csv retired.txt -> prints violations, exit 1 on any
  python3 - "$1" "$2" "${3:-}" "${4:-}" "${5:-}" <<'PY'
import csv, json, sys, collections
# claims.csv: dom,cls — every Domain row's definesVocabulary (the same source the class atlas walks)
tail = lambda iri: iri.rsplit("#", 1)[-1].rsplit("/", 1)[-1]
claim_rows = [(tail(r["dom"]), tail(r["cls"])) for r in csv.DictReader(open(sys.argv[1]))]
domains = sorted({d for d, _ in claim_rows})
owners = collections.defaultdict(set)
for row in csv.DictReader(open(sys.argv[2])):
    owners[row["d"].rsplit("#", 1)[-1]].add(row["prod"].rsplit("#", 1)[-1])
claims = collections.defaultdict(list)
for d, c in claim_rows:
    if d not in claims[c]:
        claims[c].append(d)
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
# a class the retirement ledger retires (File, SourceFile: Jeff 2026-09-18) is leaving, not homeless
retired = set()
if len(sys.argv) > 5 and sys.argv[5]:
    retired = {l.strip() for l in open(sys.argv[5]) if l.strip()}
skipped = set()
if len(sys.argv) > 3 and sys.argv[3]:
    for row in csv.DictReader(open(sys.argv[3])):
        if is_ephemeral(row.get("g", "")):
            continue
        c = row["cls"].rsplit("#", 1)[-1].rsplit("/", 1)[-1]
        if c not in claims and c in retired:
            skipped.add(c)
        if c not in claims and c not in retired:
            bad.append(f"class {c}: has a shape, claimed by no domain")
if skipped:
    print("  retired, pending removal: " + ", ".join(sorted(skipped)))
for b in bad:
    print("  " + b)
print(f"domains {len(domains)}, classes {len(claims)}, violations {len(bad)}")
sys.exit(1 if bad else 0)
PY
}

if [ "${1:-}" = "--self-test" ]; then
  T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
  printf 'dom,cls\nhttps://x#a,https://x#X\nhttps://x#b,https://x#Y\n' > "$T/atlas.json"
  printf 'prod,d\nchorus,a\nloom,b\n' > "$T/ok.csv"
  check "$T/atlas.json" "$T/ok.csv" >/dev/null || { echo "FAIL: one product per domain reported red"; exit 1; }
  # NEGATIVE PROOF: an orphan domain, a doubly-owned domain, and a doubly-claimed class are each red
  printf 'prod,d\nchorus,a\n' > "$T/orphan.csv"
  check "$T/atlas.json" "$T/orphan.csv" >/dev/null && { echo "FAIL NEGATIVE: an orphan domain passed"; exit 1; }
  printf 'prod,d\nchorus,a\nloom,b\nathena,b\n' > "$T/double.csv"
  check "$T/atlas.json" "$T/double.csv" >/dev/null && { echo "FAIL NEGATIVE: a domain with two products passed"; exit 1; }
  printf 'dom,cls\nhttps://x#a,https://x#X\nhttps://x#b,https://x#X\n' > "$T/twice.json"
  check "$T/twice.json" "$T/ok.csv" >/dev/null && { echo "FAIL NEGATIVE: a class claimed twice passed"; exit 1; }
  printf 'cls\nhttps://x#X\nhttps://x#Z\n' > "$T/shaped.csv"
  check "$T/atlas.json" "$T/ok.csv" "$T/shaped.csv" >/dev/null && { echo "FAIL NEGATIVE: a shaped class no domain claims passed"; exit 1; }
  printf 'cls,g\nhttps://x#Z,urn:chorus:verbs-sandbox\n' > "$T/sandbox.csv"
  printf 'iri\nurn:chorus:verbs-sandbox\n' > "$T/eph.csv"
  check "$T/atlas.json" "$T/ok.csv" "$T/sandbox.csv" "$T/eph.csv" >/dev/null || { echo "FAIL: a shape left in a test-ephemeral graph counted as a gap"; exit 1; }
  check "$T/atlas.json" "$T/ok.csv" "$T/sandbox.csv" /dev/null >/dev/null && { echo "FAIL NEGATIVE: the same shape outside an ephemeral graph passed"; exit 1; }
  printf 'Z\n' > "$T/retired.txt"
  out=$(check "$T/atlas.json" "$T/ok.csv" "$T/shaped.csv" /dev/null "$T/retired.txt") || { echo "FAIL: a class the ledger retires counted as homeless"; exit 1; }
  echo "$out" | grep -q "retired, pending removal: Z" || { echo "FAIL: a skipped retired class was not named"; exit 1; }
  printf 'Q\n' > "$T/other.txt"
  check "$T/atlas.json" "$T/ok.csv" "$T/shaped.csv" /dev/null "$T/other.txt" >/dev/null && { echo "FAIL NEGATIVE: an unclaimed class not on the ledger passed"; exit 1; }
  echo "PASS self-test: orphan, two products, a twice-claimed class and an unclaimed shaped class are each red; test-ephemeral graphs and ledger-retired classes are skipped"; exit 0
fi

FUSEKI=$(pick_store)
[ -z "$FUSEKI" ] && { echo "UNMEASURED: staging holds another card's model (its deploy stamp is not in this werk's history)"; exit 0; }
echo "store: $FUSEKI"
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
python3 -c 'import json,sys
for l in open(sys.argv[1]):
    l=l.strip()
    if l:
        r=json.loads(l)
        if r.get("retire_class"): print(r["retire_class"].rsplit("#",1)[-1])' "$ROOT/designing/schemas/model-retirements.jsonl" > "$T/retired.txt" 2>/dev/null || : > "$T/retired.txt"
curl -sf -m 30 -H 'Accept: text/csv' --data-urlencode 'query=PREFIX c:<https://jeffbridwell.com/chorus#> SELECT DISTINCT ?dom ?cls WHERE { GRAPH ?g { ?dom a c:Domain ; c:definesVocabulary ?cls } }' "$FUSEKI" > "$T/atlas.json" 2>/dev/null \
  || { echo "UNMEASURED: store not reachable at $FUSEKI"; exit 0; }
curl -sf -m 30 -H 'Accept: text/csv' --data-urlencode 'query=PREFIX c:<https://jeffbridwell.com/chorus#> SELECT ?prod ?d WHERE { GRAPH ?g { ?prod a c:Product ; c:hasDomain ?d } }' "$FUSEKI" > "$T/pairs.csv" 2>/dev/null \
  || { echo "UNMEASURED: store not reachable at $FUSEKI"; exit 0; }
curl -sf -m 30 -H 'Accept: text/csv' --data-urlencode 'query=PREFIX sh:<http://www.w3.org/ns/shacl#> SELECT DISTINCT ?cls ?g WHERE { GRAPH ?g { ?s sh:targetClass ?cls } }' "$FUSEKI" > "$T/shaped.csv" 2>/dev/null \
  || { echo "UNMEASURED: store not reachable at $FUSEKI"; exit 0; }
curl -sf -m 30 -H 'Accept: text/csv' --data-urlencode 'query=PREFIX c:<https://jeffbridwell.com/chorus#> SELECT ?iri WHERE { GRAPH ?x { ?n c:graphStatus "test-ephemeral" ; c:graphIri ?iri } }' "$FUSEKI" > "$T/ephemeral.csv" 2>/dev/null \
  || { echo "UNMEASURED: store not reachable at $FUSEKI"; exit 0; }
check "$T/atlas.json" "$T/pairs.csv" "$T/shaped.csv" "$T/ephemeral.csv" "$T/retired.txt"
