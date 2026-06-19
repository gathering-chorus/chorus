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

# 4. failureClass conformance (#3513 Part B): every jsonl() emit of a *.failed or
#    *.refused event must carry the DORA change-vs-tooling discriminator —
#    failureClass — via the shared fail_extra() helper or a literal "failureClass"
#    key. The jsonl witness is what the read side (#3497) walks; a naked failure
#    fails loud here, by construction. (Scoped to jsonl, the durable witness;
#    spine is best-effort. The ONE classifier lives at services/shared/failure_class.rs.)
FAIL_EVENT = re.compile(r'"([a-z][a-z0-9_]*(?:\.[a-z0-9_]+)*\.(?:failed|refused))"')
JSONL_STMT = re.compile(r'jsonl\((.*?)\);', re.S)
naked = []
for f in glob.glob(os.path.join(root, "platform/services/werk-*/src/*.rs")):
    verb = os.path.basename(os.path.dirname(os.path.dirname(f)))
    src = open(f).read()
    for body in JSONL_STMT.findall(src):
        evs = FAIL_EVENT.findall(body)
        if not evs:
            continue
        if "failureClass" not in body and "fail_extra" not in body:
            naked.append((verb, evs[0]))

print(f"\nfailure jsonl emits checked for failureClass | naked: {len(naked)}")
for verb, ev in naked:
    print(f"  NAKED (no failureClass): {verb} -> {ev}")
if naked:
    print(f"\nFAIL (#3513 Part B): {len(naked)} failure emit(s) ship without failureClass — wrap the extra in fail_extra(reason).")
    sys.exit(1)
print("OK: every *.failed/*.refused jsonl emit carries failureClass.")

# 5. CLOSED VOCABULARY (#3513, Wren's review): failureClass is a CLOSED enum —
#    {change, tooling} — not free-text. Presence (check 4) is not enough: a hand-
#    written class string would drift into the per-surface mess. Two guarantees:
#    (a) the ONE classifier's codomain is exactly {change, tooling}; (b) no verb
#    source hard-codes a failureClass literal outside that set. Together: the value
#    is closed by construction AND the guard validates it, so the read side (#3497)
#    can treat failureClass as a 2-valued enum.
ALLOWED = {"change", "tooling"}
violations = []

# (a) classifier codomain — every string literal the match arms return.
clsf = os.path.join(root, "platform/services/shared/failure_class.rs")
csrc = open(clsf).read()
fn = re.search(r'pub fn failure_class\(reason: &str\) -> &.static str \{(.*?)\n\}', csrc, re.S)
returned = set(re.findall(r'=>\s*"([a-z]+)"', fn.group(1))) if fn else set()
for v in sorted(returned - ALLOWED):
    violations.append(f"classifier returns non-enum value: {v!r}")
if not returned:
    violations.append("could not parse failure_class() codomain")

# (b) no hard-coded failureClass literal outside the enum anywhere in the verbs.
LIT = re.compile(r'\\"failureClass\\":\\"([a-z]+)\\"')
for f in glob.glob(os.path.join(root, "platform/services/werk-*/src/*.rs")):
    for val in LIT.findall(open(f).read()):
        if val not in ALLOWED:
            violations.append(f"{os.path.basename(os.path.dirname(os.path.dirname(f)))}: hard-coded failureClass {val!r}")

print(f"\nfailureClass closed-vocabulary check | classifier codomain={sorted(returned)} | violations: {len(violations)}")
for v in violations:
    print(f"  OUT-OF-ENUM: {v}")
if violations:
    print(f"\nFAIL (#3513): failureClass is not a closed {{change,tooling}} enum — fix the classifier/emit, don't widen the vocabulary silently.")
    sys.exit(1)
print("OK: failureClass is a closed {change, tooling} enum, validated by construction.")
PY
