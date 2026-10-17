#!/usr/bin/env bash
# @domain: domains
# @test-type: fitness — reads the model's TTL source; no live services
# @card: #4576
# @owner: wren
#
# #4576 — every class carries the core attributes (name, description, created/changed at and by;
# Jeff 2026-09-16 and 2026-10-17). Silas 10-17: explicit, not implied, so each class shape must say
# `sh:node chorus:CoreShape`. A shape without it is red, and so is a missing CoreShape.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

check() { # dir whose */ontology/*.ttl (or *.ttl for fixtures) are the model -> violations, exit 1 on any
  local files; files=$(ls "$1"/*/ontology/*.ttl 2>/dev/null; ls "$1"/*.ttl 2>/dev/null)
  # parse with riot, so the check reads the graph, not the text layout
  riot --output=ntriples $files 2>/dev/null | RETIREMENTS="${2:-}" python3 -c '
import sys, collections
SH = "<http://www.w3.org/ns/shacl#"
core = "<https://jeffbridwell.com/chorus#CoreShape>"
# Jeff 2026-10-17 08:24, on TestResult: "i thought we were gonna deprecate logging in the graph
# this one class always causes non functional issues". Its ~1.1M rows are log data, not model; no stamps.
EXEMPT = {"<https://jeffbridwell.com/chorus#TestResult>"}
# classes the retirement ledger retires (File, SourceFile: Jeff 2026-09-18) are leaving; named, not silent
import json, os
_ledger = os.environ.get("RETIREMENTS", "")
RETIRED = set()
if _ledger and os.path.exists(_ledger):
    for _l in open(_ledger):
        _l = _l.strip()
        if _l and json.loads(_l).get("retire_class"):
            RETIRED.add("<" + json.loads(_l)["retire_class"] + ">")
target, node, nodeshapes = {}, collections.defaultdict(set), set()
for line in sys.stdin:
    parts = line.split(" ", 2)
    if len(parts) < 3: continue
    s, p, o = parts[0], parts[1], parts[2].rstrip(" .\n")
    if p == SH + "targetClass>": target[s] = o
    elif p == SH + "node>": node[s].add(o)
    elif o == SH + "NodeShape>": nodeshapes.add(s)
bad = [f"shape for {c} has no sh:node chorus:CoreShape" for s, c in sorted(target.items()) if core not in node[s] and c not in EXEMPT and c not in RETIRED]
skipped = sorted(c.rsplit("#", 1)[-1].rstrip(">") for s, c in target.items() if core not in node[s] and c in RETIRED)
if skipped: print("  retired, pending removal: " + ", ".join(skipped))
if core not in nodeshapes: bad.append("chorus:CoreShape is not defined")
for b in bad: print("  " + b)
print(f"class shapes {len(target)}, violations {len(bad)}")
sys.exit(1 if bad else 0)
'
}
if [ "${1:-}" = "--self-test" ]; then
  T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
  mkdir -p "$T/ok" "$T/missing" "$T/nocore"; P='@prefix chorus: <https://jeffbridwell.com/chorus#> . @prefix sh: <http://www.w3.org/ns/shacl#> .'
  printf "$P\n"'chorus:CoreShape a sh:NodeShape .\nchorus:AShape a sh:NodeShape ; sh:targetClass chorus:A ; sh:node chorus:CoreShape ;\n  sh:property [ sh:path chorus:x ] .\n' > "$T/ok/a.ttl"
  check "$T/ok" >/dev/null || { echo "FAIL: a shape with the node line reported red"; exit 1; }
  # NEGATIVE PROOF: a shape without the line, and a model with no CoreShape, are each red
  printf "$P\n"'chorus:CoreShape a sh:NodeShape .\nchorus:BShape a sh:NodeShape ; sh:targetClass chorus:B ;\n  sh:property [ sh:path chorus:x ] .\n' > "$T/missing/b.ttl"
  check "$T/missing" >/dev/null && { echo "FAIL NEGATIVE: a shape without sh:node CoreShape passed"; exit 1; }
  printf "$P\n"'chorus:AShape a sh:NodeShape ; sh:targetClass chorus:A ; sh:node chorus:CoreShape .\n' > "$T/nocore/a.ttl"
  check "$T/nocore" >/dev/null && { echo "FAIL NEGATIVE: a model with no CoreShape passed"; exit 1; }
  echo "PASS self-test: a shape without the core node, and a model without CoreShape, are each red"; exit 0
fi
check "$ROOT/roles" "$ROOT/designing/schemas/model-retirements.jsonl"
