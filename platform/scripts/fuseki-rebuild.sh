#!/usr/bin/env bash
# fuseki-rebuild.sh — rebuild the TDB2 store one proven step at a time.
#
# WHY THIS EXISTS. We have recovered this store from the same corruption twice
# (2026-03-29, and again today) and automated it zero times. Both runs were done
# by hand from a markdown table; the knowledge left with the session. Jeff,
# 2026-09-14: "make the rebuild a script this time - i used to run 1 step at a
# time - comment each step and run until i proved all steps worked in sequence".
#
# THE FAILURE IT RECOVERS FROM. Jena cannot read part of the node table:
#   TDBException: NodeTableTRDF/Read  <- TProtocolException: Unrecognized type 0
# Compaction dies on it (so the store never shrinks — 432 GB today) and so does
# tdb2.tdbdump, which stopped after 796 quads. SPARQL queries still work, because
# they resolve nodes by id rather than scanning the table. So the data comes out
# through the query door or not at all.
#
# HOW TO USE IT. One step at a time, in order. Every step is idempotent, prints
# what it did, and writes a receipt; the next step REFUSES if the previous
# receipt is missing. Nothing destructive happens before step 6, and step 6
# moves the old store aside rather than deleting it.
#
#   fuseki-rebuild.sh inventory      1. what is in the store, per graph
#   fuseki-rebuild.sh export         2. SPARQL each graph out to N-Quads
#   fuseki-rebuild.sh verify-export  3. every graph's file has the count step 1 saw
#   fuseki-rebuild.sh quadify       3b. put the graph back on each triple; re-home the catch-all
#   fuseki-rebuild.sh load           4. load the files into a NEW store
#   fuseki-rebuild.sh verify-load    5. the new store has the same counts
#   fuseki-rebuild.sh swap           6. stop fuseki, swap stores, start  (DESTRUCTIVE)
#   fuseki-rebuild.sh verify-live    7. live counts match, and compact reclaims
#   fuseki-rebuild.sh status         where am I
set -uo pipefail

WORK="${FUSEKI_REBUILD_WORK:-$HOME/.chorus/fuseki-rebuild}"
STORE="${FUSEKI_STORE:-$HOME/.gathering/data/fuseki-pods}"
NEW_STORE="${FUSEKI_NEW_STORE:-${STORE}-rebuilt}"
QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
DATASET="${FUSEKI_DATASET:-pods}"
RECEIPTS="$WORK/receipts"
EXPORTS="$WORK/exports"
mkdir -p "$RECEIPTS" "$EXPORTS"

log()   { echo "$(date '+%H:%M:%S') [rebuild] $*"; }
die()   { echo "$(date '+%H:%M:%S') [rebuild] REFUSED: $*" >&2; exit 2; }
receipt_done() { [ -f "$RECEIPTS/$1.done" ]; }
mark_done()    { date -u +%FT%TZ > "$RECEIPTS/$1.done"; log "step '$1' complete — receipt written"; }
need()  { receipt_done "$1" || die "step '$1' has not completed. Run: $0 $1"; }

# Graph names are URIs; the file that holds one is named by its sha so the
# filesystem never has to care about slashes or colons.
gfile() { printf '%s' "$1" | shasum -a 256 | cut -c1-40; }

auth() { source "$(dirname "${BASH_SOURCE[0]}")/fuseki-auth.sh" >/dev/null 2>&1 || true; }

sparql() {  # sparql <query> -> JSON on stdout
  auth
  curl -s --max-time "${FUSEKI_REBUILD_TIMEOUT:-600}" ${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"} \
    -G "$QUERY" --data-urlencode "query=$1" -H "Accept: application/sparql-results+json"
}

# ── 1. inventory ───────────────────────────────────────────────────────────
# Read-only. The count per graph is the number every later step is checked
# against, so it is captured ONCE and never recomputed from the thing we are
# trying to verify.
#
# COUNT(*) on purpose, and it is the CHEAP number: rows, duplicates included.
# It is not what a CONSTRUCT emits — a CONSTRUCT emits a SET — so step 3 has to
# reconcile the two. Counting DISTINCT here instead looks tidier and is not
# affordable: over 39.7M triples the store-wide DISTINCT ran past ten minutes
# and timed out, measured 13:03-13:13. Step 3 pays that cost only for the
# handful of graphs that actually differ.
cmd_inventory() {
  log "reading every graph and its triple count (this takes a few minutes)"
  sparql 'SELECT ?g (COUNT(*) AS ?n) WHERE { GRAPH ?g { ?s ?p ?o } } GROUP BY ?g ORDER BY DESC(?n)' \
  | python3 -c '
import sys, json
d = json.load(sys.stdin)["results"]["bindings"]
if not d:
    sys.exit("inventory returned no graphs - is Fuseki up?")
for b in d:
    print(b["n"]["value"] + "\t" + b["g"]["value"])
' > "$WORK/inventory.tsv" || die "inventory query failed"
  local graphs total
  graphs=$(wc -l < "$WORK/inventory.tsv" | tr -d ' ')
  total=$(awk -F'\t' '{s+=$1} END {print s}' "$WORK/inventory.tsv")
  [ "$graphs" -gt 0 ] || die "inventory is empty"
  log "inventory: $graphs graphs, $total triples -> $WORK/inventory.tsv"
  mark_done inventory
}

# ── 2. export ──────────────────────────────────────────────────────────────
# One CONSTRUCT per graph, straight to a file. Per-graph (not one big dump)
# for three reasons: a single bad graph cannot poison the whole export, each
# file is independently checkable in step 3, and a resumed run skips what it
# already has.
cmd_export() {
  need inventory
  local n=0 skipped=0
  while IFS=$'\t' read -r count graph; do
    local f="$EXPORTS/$(gfile "$graph").nq"
    if [ -s "$f" ]; then skipped=$((skipped+1)); continue; fi
    auth
    curl -s --max-time "${FUSEKI_REBUILD_TIMEOUT:-600}" ${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"} \
      -G "$QUERY" --data-urlencode "query=CONSTRUCT { ?s ?p ?o } WHERE { GRAPH <$graph> { ?s ?p ?o } }" \
      -H "Accept: application/n-triples" > "$f.part" || { rm -f "$f.part"; die "export failed for $graph"; }
    mv "$f.part" "$f"
    echo -e "$graph\t$f\t$count" >> "$WORK/exported.tsv"
    n=$((n+1))
    [ $((n % 200)) -eq 0 ] && log "exported $n graphs..."
  done < "$WORK/inventory.tsv"
  log "export: $n written, $skipped already present -> $EXPORTS"
  mark_done export
}

# ── 3. verify-export ───────────────────────────────────────────────────────
# Count lines in each file against the store, and re-ask the store NOW rather
# than trusting step 1's number.
#
# Why: the inventory is a photograph, the export happens later, and the roles
# keep writing in between. The first real run refused with three graphs "short"
# — tests -9, instances -35, seeds -12, 56 triples out of 39.7 million — every
# one of them a graph we write to constantly, and every one a row that arrived
# AFTER the count was taken. That is drift, not loss, and a check that cannot
# tell them apart would either block a good rebuild or wave through a bad one.
#
# So: a file may hold FEWER triples than the graph does now (someone added rows
# while we worked) but never MORE than the graph held when it was written, and
# never fewer than the graph holds after re-asking. Concretely — re-count the
# graph at verify time; the file must have at least the smaller of the two
# counts, and any shortfall must be explainable by rows added since. A file
# shorter than the live count by more than the inventory-to-now growth is a
# REAL short and still refuses.
cmd_verify_export() {
  need export
  local bad=0 checked=0 drifted=0
  while IFS=$'\t' read -r count graph; do
    local f="$EXPORTS/$(gfile "$graph").nq"
    [ -f "$f" ] || { echo "MISSING $graph"; bad=$((bad+1)); continue; }
    local got; got=$(grep -c . "$f")
    checked=$((checked+1))
    [ "$got" = "$count" ] && continue

    # Differs from the photograph. Do NOT ask the store for a DISTINCT count:
    # SELECT DISTINCT ?s ?p ?o forces Jena to read node VALUES out of the node
    # table, which is the corrupt structure this whole script exists to escape.
    # Measured 13:41:16 — Fuseki request 2362, HTTP 500 in 103 ms:
    #   NodeTableTRDF.readNodeFromTable <- TProtocolUtil.skip <- TProtocol...
    # COUNT(*) never touches the node table (it counts index entries), which is
    # why the cheap number works and the tidy one cannot.
    #
    # That 500 is also the proof this check needs. A CONSTRUCT that cannot read
    # a triple does not silently omit it — it fails the whole request, loudly,
    # the way request 2362 did. So a file that came back 200 holds everything
    # Jena can read from that graph, and the only legal shortfall against the
    # row count is byte-identical duplicates, which a CONSTRUCT dedupes because
    # it emits a set. Re-export the graph and demand an explicit 200.
    local recode refile
    refile="$f.recheck"
    auth
    recode=$(curl -s -o "$refile" -w '%{http_code}' --max-time "${FUSEKI_REBUILD_TIMEOUT:-600}" \
      ${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"} -G "$QUERY" \
      --data-urlencode "query=CONSTRUCT { ?s ?p ?o } WHERE { GRAPH <$graph> { ?s ?p ?o } }" \
      -H "Accept: application/n-triples")
    local again; again=$(grep -c . "$refile" 2>/dev/null || echo 0)
    rm -f "$refile"

    if [ "$recode" != "200" ]; then
      echo "UNREADABLE $graph — re-export returned HTTP $recode (the node table refused it)"
      bad=$((bad+1))
    elif [ "$got" -le "$count" ] && [ "$again" -ge "$got" ]; then
      # Read clean, and a second independent read is no smaller than the file.
      echo "DUPES   $graph rows=$count file=$got reread=$again (duplicate rows, not loss)"
      drifted=$((drifted+1))
    elif [ "$got" -gt "$count" ]; then
      echo "DRIFT   $graph rows=$count file=$got reread=$again (rows added during export)"
      drifted=$((drifted+1))
    else
      echo "SHORT   $graph rows=$count file=$got reread=$again — the file lost rows"
      bad=$((bad+1))
    fi
  done < "$WORK/inventory.tsv"
  [ "$bad" -eq 0 ] || die "$bad graph(s) did not export completely — do NOT proceed"
  # The proven number for step 5. NOT the inventory total: the inventory counts
  # stored rows and the export files hold distinct triples, so the store we are
  # about to build is expected to be SMALLER than the inventory by exactly the
  # duplicate count. Step 5 must check what step 3 proved, not the photograph.
  cat "$EXPORTS"/*.nq | grep -c . > "$WORK/expected-triples.txt"
  log "verify-export: $checked graphs, $drifted reconciled (duplicates or drift), 0 short"
  log "expected triples in the rebuilt store: $(cat "$WORK/expected-triples.txt")"
  mark_done verify-export
}

# ── 3b. quadify ────────────────────────────────────────────────────────────
# The export asked for application/n-triples, so every file is TRIPLES with no
# graph on them. tdbloader would have put all 6,242 graphs into ONE default
# graph — the rebuild would have "succeeded" and silently flattened the store.
# So each file is rewritten as N-QUADS with its own graph URI appended.
#
# The same pass re-homes urn:chorus:instances, the catch-all Jeff has called
# wrong since 2026-06-22. A row's home is its own domain graph. The model says
# where each class lives (definesVocabulary in urn:chorus:ontology); a class
# with no such edge is v1 and is DROPPED, not carried forward. This is the one
# moment the data is flat text instead of a database, so it is the cheap moment
# to do it. Nothing is guessed — the map comes from the store, and the report
# names every class and count on both sides.
CATCHALL="${FUSEKI_CATCHALL_GRAPH:-urn:chorus:instances}"
QUADS="$WORK/quads"

cmd_quadify() {
  need verify-export
  mkdir -p "$QUADS"

  # The class -> domain map, straight from the ontology. Cheap: the ontology
  # graph is small and this touches no corrupt region.
  log "reading the class -> domain map from urn:chorus:ontology"
  sparql 'PREFIX c: <https://jeffbridwell.com/chorus#> SELECT ?d ?cls WHERE { GRAPH <urn:chorus:ontology> { ?d c:definesVocabulary ?cls } }' \
  | python3 -c '
import sys, json
b = json.load(sys.stdin)["results"]["bindings"]
for x in b:
    d = x["d"]["value"].split("#")[-1]
    if d.endswith("-domain"): d = d[:-7]
    print(x["cls"]["value"] + "\t" + d)
' > "$WORK/class-domain.tsv" || die "could not read the class -> domain map"
  local mapped; mapped=$(wc -l < "$WORK/class-domain.tsv" | tr -d ' ')
  [ "$mapped" -gt 0 ] || die "the class -> domain map is empty — refusing to re-home anything"
  log "class -> domain map: $mapped classes"

  local n=0
  while IFS=$'\t' read -r count graph; do
    local src="$EXPORTS/$(gfile "$graph").nq"
    [ -f "$src" ] || die "missing export for $graph"
    if [ "$graph" = "$CATCHALL" ]; then continue; fi   # handled below
    # Append the graph as the fourth term. Every export line already ends
    # " ." so the suffix replaces that terminator.
    sed "s| \.\$| <$graph> .|" "$src" > "$QUADS/$(gfile "$graph").nq"
    n=$((n+1))
    [ $((n % 500)) -eq 0 ] && log "quadified $n graphs..."
  done < "$WORK/inventory.tsv"
  log "quadified $n graphs into $QUADS"

  # The catch-all, split by class.
  local cf="$EXPORTS/$(gfile "$CATCHALL").nq"
  if [ -f "$cf" ]; then
    FR_SRC="$cf" FR_MAP="$WORK/class-domain.tsv" FR_OUT="$QUADS/instances-refiled.nq" \
    FR_DROP="$WORK/instances-dropped.nt" FR_REPORT="$WORK/refile-report.txt" \
    python3 "$(dirname "${BASH_SOURCE[0]}")/fuseki-refile-instances.py" \
      || die "re-homing $CATCHALL failed"
    cat "$WORK/refile-report.txt"
  else
    log "no $CATCHALL export present — nothing to re-home"
  fi
  mark_done quadify
}

# ── 4. load ────────────────────────────────────────────────────────────────
# Into a NEW directory. The old store is untouched and stays servable, so a
# failure here costs nothing but time.
cmd_load() {
  need quadify
  [ -e "$NEW_STORE" ] && die "$NEW_STORE already exists — move it aside first"
  mkdir -p "$NEW_STORE"
  log "loading $(ls "$QUADS"/*.nq 2>/dev/null | wc -l | tr -d ' ') QUAD files into $NEW_STORE"
  tdb2.tdbloader --loc="$NEW_STORE" "$QUADS"/*.nq || die "tdbloader failed"
  mark_done load
}

# ── 5. verify-load ─────────────────────────────────────────────────────────
# The new store must answer with the same totals BEFORE anything is swapped.
cmd_verify_load() {
  need load
  local want got
  want=$(cat "$WORK/expected-triples.txt")
  # GRAPH ?g, not a bare pattern. Every triple in this store lives in a NAMED
  # graph, and a bare { ?s ?p ?o } reads the DEFAULT graph only — which is
  # empty by construction. The first run of this check reported "rebuilt=0,
  # expected 39,687,538" against a store that had just loaded all 39.7M
  # correctly. A count that cannot see the data it is counting is worse than
  # no count: it refuses a good rebuild and would wave through a bad one.
  got=$(tdb2.tdbquery --loc="$NEW_STORE" 'SELECT (COUNT(*) AS ?n) WHERE { GRAPH ?g { ?s ?p ?o } }' 2>/dev/null \
        | grep -oE '[0-9]+' | tail -1)
  [ -n "$got" ] || die "could not count the rebuilt store"
  log "verify-load: rebuilt=$got  expected=$want"
  [ "$got" = "$want" ] || die "rebuilt store has $got triples, expected $want"
  mark_done verify-load
}

# ── 6. swap ────────────────────────────────────────────────────────────────
# The only destructive step, and it MOVES rather than deletes. Fuseki is stopped
# by label, not by killing a pid.
cmd_swap() {
  need verify-load
  local stamp; stamp=$(date +%Y%m%d-%H%M%S)
  log "stopping fuseki"
  launchctl bootout "gui/$(id -u)/com.gathering.fuseki" 2>/dev/null || true
  sleep 5
  pgrep -f "fuseki-server.jar" >/dev/null && die "fuseki still running — refusing to swap a live store"
  mv "$STORE" "${STORE}.corrupt-${stamp}" || die "could not move the old store aside"
  mv "$NEW_STORE" "$STORE"  || die "could not move the rebuilt store into place"
  log "old store kept at ${STORE}.corrupt-${stamp} — delete it only after step 7 passes"
  launchctl bootstrap "gui/$(id -u)" "$HOME/Library/LaunchAgents/com.gathering.fuseki.plist" 2>/dev/null || true
  sleep 10
  mark_done swap
}

# ── 7. verify-live ─────────────────────────────────────────────────────────
# Two proofs: the live store answers with the expected total, and compaction —
# the thing that has been failing since 2026-08-29 — now actually reclaims.
cmd_verify_live() {
  need swap
  local want got before after
  want=$(cat "$WORK/expected-triples.txt")
  # GRAPH ?g here too, for a second reason on top of step 5's. This dataset
  # serves a UNION default graph, so a bare { ?s ?p ?o } returns the union
  # DEDUPLICATED across graphs — it read 39,674,078 against a store holding
  # exactly 39,687,538, because 13,460 triples legitimately appear in more
  # than one graph. Two different checks, same defect: counting a projection
  # of the store and calling it the store.
  got=$(sparql 'SELECT (COUNT(*) AS ?n) WHERE { GRAPH ?g { ?s ?p ?o } }' \
        | python3 -c 'import sys,json;print(json.load(sys.stdin)["results"]["bindings"][0]["n"]["value"])' 2>/dev/null)
  log "verify-live: live=$got  expected=$want"
  [ "$got" = "$want" ] || die "live store has $got triples, expected $want"
  before=$(du -sk "$STORE" | awk '{print $1}')
  log "running compact to prove it reclaims (store ${before}K)"
  bash "$(dirname "${BASH_SOURCE[0]}")/../../building/products/convergence/fuseki-maintenance.sh" compact || true
  after=$(du -sk "$STORE" | awk '{print $1}')
  log "compact: ${before}K -> ${after}K"
  [ "$after" -lt "$before" ] || die "compact still reclaims nothing — the rebuild did not fix the node table"
  mark_done verify-live
  log "REBUILD PROVEN. Old store may now be deleted."
}

cmd_status() {
  for s in inventory export verify-export quadify load verify-load swap verify-live; do
    if receipt_done "$s"; then echo "  done     $s   ($(cat "$RECEIPTS/$s.done"))"; else echo "  pending  $s"; fi
  done
}

case "${1:-status}" in
  inventory)     cmd_inventory ;;
  export)        cmd_export ;;
  verify-export) cmd_verify_export ;;
  quadify)       cmd_quadify ;;
  load)          cmd_load ;;
  verify-load)   cmd_verify_load ;;
  swap)          cmd_swap ;;
  verify-live)   cmd_verify_live ;;
  status)        cmd_status ;;
  *) echo "usage: $0 {inventory|export|verify-export|quadify|load|verify-load|swap|verify-live|status}" >&2; exit 1 ;;
esac
