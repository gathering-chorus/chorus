#!/usr/bin/env bash
# @test-type: unit — reads the instance TTLs; no store
# @domain: products
#
# #4390 — every product's atStep names a declared ValueStreamStep. gathering
# pointed at chorus:Operating (an EventCategory) and every canonical model land
# failed at the seed step with 422 unknown-target.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
check() { # products-file steps-file
  python3 - "$1" "$2" <<'PY'
import re,sys
prod=open(sys.argv[1]).read(); steps=open(sys.argv[2]).read()
declared=set(re.findall(r'^chorus:(value-stream-step-[a-z0-9-]+)\s+a chorus:ValueStreamStep', steps, re.M))
bad=[(m.group(1)) for m in re.finditer(r'chorus:atStep chorus:([A-Za-z0-9-]+)', prod) if m.group(1) not in declared]
for b in bad: print(f"not a ValueStreamStep: chorus:{b}")
print(f"{len(re.findall(r'chorus:atStep chorus:', prod))} product steps, {len(bad)} not declared")
sys.exit(1 if bad else 0)
PY
}
pass=0; fail=0
if out=$(check "$ROOT/designing/data/product-instances.ttl" "$ROOT/designing/data/value-stream-step-instances.ttl"); then echo "PASS every product's atStep is a declared step ($out)"; pass=$((pass+1)); else echo "FAIL $out"; fail=$((fail+1)); fi
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
printf 'chorus:x a chorus:Product ;\n    chorus:atStep chorus:Operating .\n' > "$T/p.ttl"
if check "$T/p.ttl" "$ROOT/designing/data/value-stream-step-instances.ttl" >/dev/null; then echo "FAIL NEGATIVE: an event category passed as a step"; fail=$((fail+1)); else echo "PASS NEGATIVE: a product at chorus:Operating is red"; pass=$((pass+1)); fi
# a product's design doc must be a Document row the API can name (chorus:document-…), declared in document-instances.ttl
docs() { python3 - "$1" "$ROOT/designing/data/document-instances.ttl" <<'PY'
import re,sys
prod=open(sys.argv[1]).read(); docs=open(sys.argv[2]).read()
declared=set(re.findall(r'^chorus:(document-[a-z0-9-]+)\s+a chorus:Document', docs, re.M))
refs=re.findall(r'chorus:hasDesignDoc\s+(<[^>]+>|chorus:[A-Za-z0-9-]+)', prod)
bad=[r for r in refs if not (r.startswith('chorus:') and r[7:] in declared)]
for b in bad: print(f"design doc not a declared Document row: {b}")
print(f"{len(refs)} design docs, {len(bad)} not declared"); sys.exit(1 if bad else 0)
PY
}
if out=$(docs "$ROOT/designing/data/product-instances.ttl"); then echo "PASS every product's design doc is a declared Document ($out)"; pass=$((pass+1)); else echo "FAIL $out"; fail=$((fail+1)); fi
printf 'chorus:x a chorus:Product ;\n    chorus:hasDesignDoc <https://jeffbridwell.com/chorus#doc/docs%%2FX.md> .\n' > "$T/d.ttl"
if docs "$T/d.ttl" >/dev/null; then echo "FAIL NEGATIVE: a legacy doc IRI passed"; fail=$((fail+1)); else echo "PASS NEGATIVE: a legacy doc/ IRI is red"; pass=$((pass+1)); fi
# DocumentShape requires hasDomain (seed 422 on 09-28 07:23 without it): every Document row carries one
nodom() { python3 - "$1" <<'PY'
import re,sys
t=open(sys.argv[1]).read()
blocks=re.findall(r'^chorus:(document-[a-z0-9-]+)\s+a chorus:Document\s*;(.*?)\s\.\s*$', t, re.M|re.S)
bad=[n for n,b in blocks if 'chorus:hasDomain' not in b]
for n in bad: print(f"Document without hasDomain: {n}")
print(f"{len(blocks)} Document rows, {len(bad)} without hasDomain"); sys.exit(1 if bad else 0)
PY
}
if out=$(nodom "$ROOT/designing/data/document-instances.ttl"); then echo "PASS every Document row names its domain ($out)"; pass=$((pass+1)); else echo "FAIL $out"; fail=$((fail+1)); fi
printf 'chorus:document-x a chorus:Document ;\n    chorus:docTitle """X""" .\n' > "$T/n.ttl"
if nodom "$T/n.ttl" >/dev/null; then echo "FAIL NEGATIVE: a Document without hasDomain passed"; fail=$((fail+1)); else echo "PASS NEGATIVE: a Document without hasDomain is red"; pass=$((pass+1)); fi
echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
