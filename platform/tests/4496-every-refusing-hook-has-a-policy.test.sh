#!/usr/bin/env bash
# @domain: policies
# #4496 — every hook that refuses something names the policy it enforces.
# Jeff 2026-10-10: "i believe you might be able to declare a policy for most of our
# gates (aka hooks)". A policy names its hook through chorus:enforcedBy, pointing at
# the CodeFile row the crawler keeps for platform/services/chorus-hooks/src/hooks/<m>.rs.
# Hooks that only warn or do plumbing are listed below with a reason, so a new hook
# lands in one list or the other — never in neither.
#
# Fails when:
#   1. a hook module is neither enforced by a policy nor listed as not-a-policy;
#   2. a module listed as not-a-policy refuses in its own source;
#   3. an enforcedBy points at a hook file that does not exist;
#   4. a Policy row has no owner.
# --self-test runs each failure against a fixture and must see it fail (#3734).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

# Not a policy: they warn and let the call through, or they are plumbing.
NOT_A_POLICY='
bedroom_nfs_guard	warns: heavy NFS work should run on Bedroom, still allowed
icd_pre_read	warns: advisory since #3677, prints only
input_classifier	warns: labels Jeff'"'"'s input as statement, question or command
memory_gate	warns: its warning is dropped in dispatch
nifi_discipline	warns: NiFi cards should use native processors
nudge_blast_radius	warns: shows the interrupt cost of a nudge
pair_enforcement	warns: tells the partner to load /pair
pair_gate	warns: advisory since #3677, always allows
quality_gate	warns: advisory since #3677, logs only
sparql_guard	warns: raw SPARQL should use /pods/query and a GRAPH clause
buzz_reply	plumbing: publishes the reply to Buzz
card_approval_responder	plumbing: replays or cancels cards on approve or deny
clock_sync	plumbing: writes the wall-clock time
context_inject	plumbing: adds Chorus, memory and git context to the prompt
e2e_responder	plumbing: acknowledges e2e test nudges
handoff_logger	plumbing: logs handoff writes
icd_write_gate	plumbing: validates and reloads an ICD file after it is written
inject_force	plumbing: records whether the reply used the injected context
interaction_pattern	plumbing: classifies Jeff'"'"'s prompt into a pattern
jdi_detector	plumbing: logs "just do it"
nudge_poll	plumbing: finds unread nudges for the injected context
observer	plumbing: writes the rolling activity digest
reply_delivery	plumbing: records the reply hash for delivery matching
tool_telemetry	plumbing: logs every tool call
'

check() { # $1 = hooks dir, $2 = policy ttl
python3 - "$1" "$2" "$NOT_A_POLICY" <<'PY'
import os, re, sys
hooks, ttl, ledger = sys.argv[1], sys.argv[2], sys.argv[3]
def cf(mod):
    rel = f"platform/services/chorus-hooks/src/hooks/{mod}.rs"
    out, ld = "", False
    for c in rel:
        if c.isascii() and c.isalnum(): out += c.lower(); ld = False
        elif not ld: out += "-"; ld = True
    h = 0xcbf29ce484222325
    for b in rel.encode(): h ^= b; h = (h * 0x100000001b3) & 0xffffffffffffffff
    return "code-file-file-%s-%08x" % (out.strip("-"), h & 0xffffffff)
mods = sorted(f[:-3] for f in os.listdir(hooks) if f.endswith(".rs") and f != "mod.rs" and not f.endswith("_test.rs"))
by_iri = {cf(m): m for m in mods}
text = open(ttl).read()
enforced = set()
bad = []
for block in re.findall(r"chorus:enforcedBy\s+([^;.]+)", text):
    for iri in re.findall(r"chorus:(code-file-[a-z0-9-]+)", block):
        if iri in by_iri: enforced.add(by_iri[iri])
        else: bad.append(f"enforcedBy points at {iri}, which is no hook file")
skip = {l.split("\t")[0] for l in ledger.strip().splitlines()}
for m in mods:
    if m in enforced and m in skip: bad.append(f"{m} is enforced by a policy and also listed as not-a-policy")
    if m not in enforced and m not in skip: bad.append(f"{m} has no policy and is not listed as not-a-policy")
    if m in skip and re.search(r"permission_deny_json|block_with_stderr|exit_code:\s*2", open(f"{hooks}/{m}.rs").read()):
        bad.append(f"{m} refuses in its own source but is listed as not-a-policy")
policies = set(re.findall(r"^chorus:(policy-[a-z0-9-]+) a chorus:Policy", text, re.M))
owned = set(re.findall(r"^chorus:(policy-[a-z0-9-]+)\b[^.]*?chorus:ownedBy", text, re.M | re.S))
for p in sorted(policies - owned): bad.append(f"{p} has no owner")
for b in bad: print("FAIL", b)
print(f"{len(mods)} hooks: {len(enforced)} enforce a policy, {len(skip & set(mods))} are not policies; {len(policies)} policies")
sys.exit(1 if bad else 0)
PY
}

if [ "${1:-}" = "--self-test" ]; then
  T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
  ok=0
  cp -R "$ROOT/platform/services/chorus-hooks/src/hooks" "$T/h1"; cp "$ROOT/designing/data/policy-instances.ttl" "$T/p.ttl"
  check "$T/h1" "$T/p.ttl" >/dev/null || { echo "self-test: the real tree must pass first"; exit 1; }
  touch "$T/h1/brand_new_gate.rs"
  if { check "$T/h1" "$T/p.ttl" || true; } | grep -q "brand_new_gate has no policy"; then ok=$((ok+1)); else echo "MISSED: a new hook with no policy"; fi
  rm "$T/h1/brand_new_gate.rs"; echo 'fn x() { permission_deny_json("no") }' >> "$T/h1/observer.rs"
  if { check "$T/h1" "$T/p.ttl" || true; } | grep -q "observer refuses"; then ok=$((ok+1)); else echo "MISSED: a plumbing hook that refuses"; fi
  cp -R "$ROOT/platform/services/chorus-hooks/src/hooks" "$T/h2"; rm "$T/h2/word_cap.rs"
  if { check "$T/h2" "$T/p.ttl" || true; } | grep -q "which is no hook file"; then ok=$((ok+1)); else echo "MISSED: a policy naming a deleted hook"; fi
  printf '\nchorus:policy-unowned a chorus:Policy ;\n    rdfs:label "x" .\n' >> "$T/p.ttl"
  if { check "$T/h1" "$T/p.ttl" || true; } | grep -q "policy-unowned has no owner"; then ok=$((ok+1)); else echo "MISSED: a policy with no owner"; fi
  [ "$ok" = 4 ] && { echo "NEGATIVE PROOF OK (4/4)"; exit 0; } || exit 1
fi

check "$ROOT/platform/services/chorus-hooks/src/hooks" "$ROOT/designing/data/policy-instances.ttl"

# The shape in roles/wren/ontology/policies-4077.ttl is what mounts the door route;
# enforcedBy must stay a link to a CodeFile row, or the edges above point at nothing.
grep -q 'sh:path chorus:enforcedBy ; sh:nodeKind sh:IRI ; sh:class chorus:CodeFile' "$ROOT/roles/wren/ontology/policies-4077.ttl" \
  || { echo "FAIL PolicyShape no longer links enforcedBy to a CodeFile row"; exit 1; }

# platform/scripts/4496-policies-apply.sh must refuse when the door has no policy
# route, not read every row as new (a 404 on an unknown route is not a missing row).
# A stand-in door that answers 404 to every path, as the real one does before the land.
D=$(mktemp -d); PORT=$((20000 + RANDOM % 20000))
python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$D" >/dev/null 2>&1 & SRV=$!; disown "$SRV"
trap 'kill "$SRV" 2>/dev/null; rm -rf "$D"' EXIT
for _ in 1 2 3 4 5 6 7 8 9 10; do curl -s -o /dev/null "http://127.0.0.1:$PORT/" && break; sleep 0.2; done
out=$(ATHENA_MAKE_URL="http://127.0.0.1:$PORT" "$ROOT/platform/scripts/4496-policies-apply.sh" 2>&1) && rc=0 || rc=$?
if [ "$rc" = 0 ] || ! printf '%s' "$out" | grep -q "does not serve /v1/policies/policies"; then
  echo "FAIL the apply script ran against a door with no policy route"; exit 1
fi
echo "PolicyShape links CodeFile; the apply script refuses a door with no policy route"
