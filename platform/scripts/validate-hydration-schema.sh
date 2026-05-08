#!/usr/bin/env bash
# validate-hydration-schema.sh — #2827 §A: refuse any predicate of a chorus:Hydratable
# class that omits chorus:writeOwner, or that names a writeOwner outside the
# allowed Writer set.
#
# The chorus:writeOwner contract is the single mechanism preventing two
# writers from competing on the same predicate (crawler vs enrichment-writer).
# A predicate without writeOwner has unowned writes — every crawl run risks
# clobbering an analysis pass's data, or vice versa. The validator runs
# pre-commit and in CI.
#
# Schema-load step parses the chorus.ttl, finds every owl:ObjectProperty /
# owl:DatatypeProperty whose rdfs:domain is chorus:Hydratable or a subclass
# thereof, and asserts each one has exactly one chorus:writeOwner triple
# pointing at a chorus:Writer instance from the allow-list.
#
# Refusals:
#   missing-writeOwner  — predicate has no chorus:writeOwner triple
#   invalid-writeOwner  — value isn't in {chorus:crawler, chorus:enrichment}
#   ttl-not-found       — chorus.ttl missing at expected path
#
# Exit codes:
#   0   all hydratable predicates declare a valid writeOwner
#   1   one or more violations (printed to stderr)
#   2   usage / missing-input

set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
TTL="${1:-$CHORUS_ROOT/roles/silas/ontology/chorus.ttl}"

ALLOWED_WRITERS=("chorus:crawler" "chorus:enrichment")

if [ ! -f "$TTL" ]; then
  echo "validate-hydration-schema: ttl-not-found at $TTL" >&2
  exit 2
fi

# 1. Build the set of Hydratable subclasses by transitive subClassOf.
# chorus:Hydratable itself is the root; anything with rdfs:subClassOf
# pointing at it (or at one of its subclasses) inherits.
hydratable_classes() {
  awk '
    BEGIN { roots["chorus:Hydratable"] = 1 }
    # Pass 1: collect all (subclass, parent) pairs
    /rdfs:subClassOf/ {
      # Find current class declaration: previous line that starts with "chorus:" + " a"
      # Simpler: track the most recent class declaration above any rdfs:subClassOf.
    }
  ' "$TTL"
  # Practical version using grep+awk — robust enough for our flat schema.
  python3 - "$TTL" <<'PYEOF'
import sys, re
ttl = open(sys.argv[1]).read()
# Find blocks: "chorus:X a owl:Class ; ... rdfs:subClassOf chorus:Y ; ..."
# Greedy-match per-class block separated by terminal "."
classes = {}
for m in re.finditer(r'(chorus:\w+)\s+a\s+owl:Class\s*;([^.]+)\.', ttl, re.DOTALL):
    cls, body = m.group(1), m.group(2)
    parents = re.findall(r'rdfs:subClassOf\s+(chorus:\w+)', body)
    classes[cls] = parents
# Transitive: roots = {Hydratable}; expand
roots = {"chorus:Hydratable"}
changed = True
while changed:
    changed = False
    for cls, parents in classes.items():
        if cls in roots:
            continue
        if any(p in roots for p in parents):
            roots.add(cls)
            changed = True
for cls in sorted(roots):
    print(cls)
PYEOF
}

HYDRATABLE_CLASSES=$(hydratable_classes)

# 2. For each predicate (ObjectProperty / DatatypeProperty), parse its
# rdfs:domain and chorus:writeOwner from its declaration block.
violations=0
violation_log=$(mktemp)
trap 'rm -f "$violation_log"' EXIT

set +e
python3 - "$TTL" "$violation_log" <<PYEOF
import sys, re
ttl = open(sys.argv[1]).read()
log = open(sys.argv[2], "w")
hydratable = set("""$HYDRATABLE_CLASSES""".split())
allowed_writers = {"chorus:crawler", "chorus:enrichment"}

# Per-predicate block. Match owl:DatatypeProperty | owl:ObjectProperty
# declarations terminated by ".".
viol = 0
for m in re.finditer(r'(chorus:\w+)\s+a\s+owl:(?:Object|Datatype)Property\s*;([^.]+)\.', ttl, re.DOTALL):
    pred, body = m.group(1), m.group(2)
    domains = re.findall(r'rdfs:domain\s+(chorus:\w+)', body)
    if not any(d in hydratable for d in domains):
        continue  # not a hydratable predicate
    write_owners = re.findall(r'chorus:writeOwner\s+(chorus:\w+)', body)
    if not write_owners:
        log.write(f"missing-writeOwner: {pred} (domain={','.join(domains)})\n")
        viol += 1
        continue
    bad = [w for w in write_owners if w not in allowed_writers]
    if bad:
        log.write(f"invalid-writeOwner: {pred} owner={bad[0]} (allowed: {sorted(allowed_writers)})\n")
        viol += 1
        continue
    if len(write_owners) > 1:
        log.write(f"ambiguous-writeOwner: {pred} declares {len(write_owners)} owners\n")
        viol += 1

log.close()
sys.exit(viol)
PYEOF
rc=$?
set -e

if [ "$rc" -ne 0 ]; then
  echo "validate-hydration-schema: $rc violation(s) in $TTL" >&2
  cat "$violation_log" >&2
  exit 1
fi

echo "validate-hydration-schema: PASS — all hydratable predicates declare writeOwner ($TTL)"
exit 0
