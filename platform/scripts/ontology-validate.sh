#!/usr/bin/env bash
# ontology-validate.sh — Validate chorus.ttl against version contract
# Card #1356 — domain versioning
#
# Usage: ontology-validate.sh [chorus.ttl] [version-contract.json]
# Exit 0 = PASS, Exit 1 = FAIL (violations listed)

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

TTL="${1:-$REPO_ROOT/roles/silas/ontology/chorus.ttl}"
CONTRACT="${2:-$REPO_ROOT/roles/silas/ontology/version-contract.json}"

if [ ! -f "$TTL" ]; then
  echo "FAIL: chorus.ttl not found at $TTL" >&2
  exit 1
fi
if [ ! -f "$CONTRACT" ]; then
  echo "FAIL: version-contract.json not found at $CONTRACT" >&2
  exit 1
fi

VIOLATIONS=0
PASS_COUNT=0

# Trace hop — convergence call stack (#2103, ADR-024)
# #3658 — trace is envelope-secured (#3619): mint a scoped token when the realm
# env is present, else send bare (fail-open, fire-and-forget as always).
CHORUS_ROOT_FOR_TOKEN="${CHORUS_ROOT:-$HOME/CascadeProjects/chorus}"
# #4004 — DO NOT cache the token in a variable. It lives 600s, and minting it
# once per run meant every call after ten minutes carried an expired token: the
# pain board shows 4,153 of 4,997 failures in 14 days — 83% of all recorded pain
# — as authn-missing on this exact surface, a steady ~5,000/day since 2026-07-29,
# and because the POST is fire-and-forget nothing errored: the traces were simply
# never recorded. chorus-identity-token ALREADY caches on disk and re-mints only
# near expiry, so calling it per call is one cheap local read, not a mint storm.
trace_auth() { "$CHORUS_ROOT_FOR_TOKEN/platform/scripts/chorus-identity-token" chorus-sdk 2>/dev/null || true; }
TRACE_AUTH=""
if [ -f "$HOME/.chorus/secrets/chorus-realm.env" ]; then
  # #3689 — ES256 identity; scope (urn:chorus:ops) is a chorus:hasScope grant
  # on principal-chorus-sdk in the model, not a claim.
    :  # token is fetched per call by trace_auth (see below)
fi
TRACE_ID="convergence-validate-$(date +%s)"
_TA="$(trace_auth)"
curl -s -X POST http://localhost:3340/api/chorus/trace \
  -H 'Content-Type: application/json' \
  ${_TA:+-H "Authorization: Bearer ${_TA}"} \
  -d "{\"correlationId\":\"$TRACE_ID\",\"hop\":1,\"callStack\":\"convergence\",\"source\":{\"domain\":\"chorus\",\"service\":\"git-queue\"},\"destination\":{\"domain\":\"chorus\",\"service\":\"ontology-validate\"}}" \
  --max-time 3 > /dev/null 2>&1 &

# Parse contract and validate against TTL
python3 - "$TTL" "$CONTRACT" <<'PYEOF'
import json, sys, re

ttl_path = sys.argv[1]
contract_path = sys.argv[2]

with open(ttl_path) as f:
    ttl = f.read()

with open(contract_path) as f:
    contract = json.load(f)

violations = []
passes = 0

classes = contract.get("classes", {})

for class_name, class_def in classes.items():
    required = class_def.get("required", False)
    # Extract local name for regex matching
    local = class_name.split(":")[-1]
    prefix = class_name.split(":")[0]

    # Check 1: required class has at least one instance
    # Look for "someUri a <class_name>" or "someUri a owl:Class" for class definitions
    # Instance pattern: something "a <class_name>"
    instance_pattern = rf'\b[\w-]+:[\w-]+\s+a\s+{re.escape(class_name)}\b'
    instances = re.findall(instance_pattern, ttl)

    # Also check for subClassOf references (class exists as a definition)
    class_def_pattern = rf'{re.escape(class_name)}\s+a\s+owl:Class'
    class_defined = bool(re.search(class_def_pattern, ttl))

    if required and not class_defined and not instances:
        violations.append(f"MISSING CLASS: {class_name} — required but not defined in TTL")
        continue

    if required and not instances:
        # Abstract classes (base classes with subclass instances) skip instance check
        if class_def.get("abstract", False):
            passes += 1
            continue
        super_class = class_def.get("superClass")
        if not super_class:
            violations.append(f"NO INSTANCES: {class_name} — required class has no instances")
            continue

    if not class_defined and not instances:
        continue  # Not required, not present — skip property checks

    passes += 1

    # Check 2+3: required properties exist with correct cardinality
    properties = class_def.get("properties", {})
    for prop_name, prop_spec in properties.items():
        prop_required = prop_spec.get("required", False)
        cardinality = prop_spec.get("cardinality", "0..*")

        if not prop_required:
            continue

        # Find all instances of this class
        # Pattern: <instance> a <class> ; ... <property> ... .
        # Simplified: check if property appears in blocks that reference this class
        prop_local = prop_name.split(":")[-1]
        prop_pattern = rf'{re.escape(prop_name)}'

        # Count instances that have this property
        # Find instance declarations
        inst_names = [m.split()[0] for m in instances]

        if not inst_names and class_name == "chorus:Domain":
            # Base class — check SubProduct and SubDomain instances instead
            continue

        missing_prop_instances = []
        lines = ttl.split('\n')
        for inst in inst_names:
            # Line-based block extraction: find the line with "inst a Class",
            # collect all lines until a line ending with " ."
            block_lines = []
            in_block = False
            for line in lines:
                if not in_block:
                    if inst in line and class_name in line and ' a ' in line:
                        in_block = True
                        block_lines.append(line)
                else:
                    block_lines.append(line)
                    stripped = line.rstrip()
                    if stripped.endswith(' .') or stripped == '.':
                        break
            block = '\n'.join(block_lines)
            if not block:
                continue

            has_prop = bool(re.search(prop_pattern, block))

            if not has_prop:
                missing_prop_instances.append(inst)

            # Cardinality check for "1" — exactly one occurrence
            if cardinality == "1" and has_prop:
                count = len(re.findall(prop_pattern, block))
                if count > 1:
                    violations.append(f"CARDINALITY: {inst} has {count}x {prop_name} (expected 1)")

        if missing_prop_instances:
            # Deduplicate
            missing_prop_instances = list(set(missing_prop_instances))
            if len(missing_prop_instances) <= 3:
                msg = f"MISSING PROPERTY: {prop_name} missing on {', '.join(missing_prop_instances)}"
            else:
                msg = f"MISSING PROPERTY: {prop_name} missing on {len(missing_prop_instances)} instances of {class_name}"

            # Find affected consumers (#1356 AC5)
            consumers = contract.get("consumers", [])
            affected = [c for c in consumers if prop_name in c.get("depends_on", [])]
            if affected:
                names = [c["name"] + " (" + c.get("owner", "?") + ")" for c in affected]
                msg += "\n      Affects: " + ", ".join(names)

            violations.append(msg)
        else:
            passes += 1

# Output
if violations:
    print(f"FAIL: {len(violations)} violation(s), {passes} passed\n")
    for v in violations:
        print(f"  ✗ {v}")
    sys.exit(1)
else:
    version = contract.get('version', '?')
    print(f"PASS: {passes} checks passed, 0 violations")
    print(f"Contract version: {version}")
    # Output version for spine event detection
    print(f"__VERSION__:{version}")
    sys.exit(0)
PYEOF

RESULT=$?
if [ $RESULT -eq 0 ]; then
  _TA="$(trace_auth)"
  curl -s -X POST http://localhost:3340/api/chorus/trace \
    -H 'Content-Type: application/json' \
    ${_TA:+-H "Authorization: Bearer ${_TA}"} \
    -d "{\"correlationId\":\"$TRACE_ID\",\"hop\":2,\"callStack\":\"convergence\",\"source\":{\"domain\":\"chorus\",\"service\":\"ontology-validate\"},\"destination\":{\"domain\":\"chorus\",\"service\":\"validate-pass\"}}" \
    --max-time 3 > /dev/null 2>&1 &
else
  _TA="$(trace_auth)"
  curl -s -X POST http://localhost:3340/api/chorus/trace \
    -H 'Content-Type: application/json' \
    ${_TA:+-H "Authorization: Bearer ${_TA}"} \
    -d "{\"correlationId\":\"$TRACE_ID\",\"hop\":2,\"callStack\":\"convergence\",\"source\":{\"domain\":\"chorus\",\"service\":\"ontology-validate\"},\"error\":{\"classification\":\"validation\",\"message\":\"ontology validation failed\",\"retryable\":false}}" \
    --max-time 3 > /dev/null 2>&1 &
fi
exit $RESULT
