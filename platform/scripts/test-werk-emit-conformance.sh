#!/usr/bin/env bash
# #3513 — werk emit conformance: every event the werk pipeline EMITS must be
# registered in designing/schemas/spine-events.json (the canonical vocabulary).
# Fails loud on any phantom (emitted-but-unregistered) event so the spine can't
# drift out of vocabulary again. Pairs with #3143 (codegen-from-schema) for the
# enforce-by-construction half; this is the source-side guard.
#
# Naming law (#3513, 3 namespaces): verb events <verb>.<phase> (the 7 verbs),
# card-lifecycle card.*, declared sequencer namespaces (werk.* run + demo.* ceremony).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SCHEMA="$ROOT/designing/schemas/spine-events.json"

python3 - "$ROOT" "$SCHEMA" <<'PY'
import json, re, sys, glob, os
root, schema_path = sys.argv[1], sys.argv[2]

# 1. EMITTED events: the first dotted-lowercase string literal in a jsonl()/emit_spine()
#    call (Rust verb crates) + "event":"X" / ev="X" (werk.yml orchestrator).
EVENT = r'"([a-z][a-z0-9_]*(?:\.[a-z0-9_]+)+)"'
emit_call = re.compile(r'(?:jsonl|emit_spine)\([^;]*?' + EVENT, re.S)
yml_event = re.compile(r'(?:"event"\s*:\s*|ev=)' + EVENT)

emits = set()
for f in glob.glob(os.path.join(root, "platform/services/werk-*/src/*.rs")):
    src = open(f).read()
    emits |= set(emit_call.findall(src))
wy = os.path.join(root, ".github/workflows/werk.yml")
if os.path.exists(wy):
    emits |= set(yml_event.findall(open(wy).read()))

# 2. REGISTERED events
registered = set(json.load(open(schema_path)).get("events", {}).keys())

# 3. phantoms
phantom = sorted(e for e in emits if e not in registered)
print(f"werk emits found: {len(emits)} | registered: {len(registered)} | phantom: {len(phantom)}")
for e in phantom:
    print(f"  PHANTOM (unregistered): {e}")
if phantom:
    print(f"\nFAIL (#3513): {len(phantom)} werk emit(s) not in spine-events.json — register them (Silas's schema lane).")
    sys.exit(1)
print("OK: every werk emit is registered in spine-events.json.")
PY
