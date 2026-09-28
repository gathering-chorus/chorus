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
echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
