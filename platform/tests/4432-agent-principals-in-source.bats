#!/usr/bin/env bats
# @test-type: unit — reads three model source files, no store, no network
# @domain: identity — the principals and the roles they hold (#4432)
#
# #4432 — Abby Normal's role and principal existed only in the live store
# (minted 2026-09-17), so a reseed would have dropped her. These cases hold
# the source to the rule that every agent principal can be rebuilt from it:
# its role is declared, typed AgentRole, carries both word caps, and the
# principal names the Mac account it runs as.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  PRINCIPALS="${PRINCIPALS_TTL:-$ROOT/roles/silas/ontology/identity-principals-3613.ttl}"
  ROLES="${ROLES_TTL:-$ROOT/roles/wren/ontology/role-instances-3838.ttl}"
  PROPS="${PROPS_TTL:-$ROOT/designing/data/property-instances.ttl}"
}

check() {
  python3 - "$PRINCIPALS" "$ROLES" "$PROPS" <<'PY'
import re, sys
P, R, X = (open(f).read() for f in sys.argv[1:4])
def blocks(t):
    t = re.sub(r'(?m)^\s*#.*$', '', t)
    return {m.group(1): m.group(2) for m in re.finditer(r'(?ms)^chorus:([\w-]+) a ([^.]*?(?:"[^"]*"[^.]*?)*)\.\s*$', t)}
pb, rb, xb = blocks(P), blocks(R), blocks(X)
bad = []
for name, body in pb.items():
    if 'principalKind "agent"' not in body:
        continue
    m = re.search(r'holdsRole chorus:([\w-]+)', body)
    if not m:
        bad.append(f"{name}: agent principal holds no role"); continue
    role = m.group(1)
    if 'hostAccount' not in body:
        bad.append(f"{name}: no hostAccount")
    if role not in rb:
        bad.append(f"{name}: holds {role}, which is not declared in role-instances"); continue
    if 'chorus:AgentRole' not in rb[role]:
        bad.append(f"{role}: not typed chorus:AgentRole")
    for cap in ('response', 'nudge'):
        prop = f"prop-{role}-{cap}-word-cap"
        if f"chorus:{role} chorus:hasProperty chorus:{prop} ." not in R:
            bad.append(f"{role}: no hasProperty {prop}")
        if prop not in xb:
            bad.append(f"{prop}: not declared in property-instances")
print("\n".join(bad) if bad else "ok")
sys.exit(1 if bad else 0)
PY
}

@test "every agent principal in source holds a declared AgentRole with both word caps and a host account" {
  run check
  echo "$output"
  [ "$status" -eq 0 ]
}

@test "abby-normal is one of them" {
  grep -q '^chorus:principal-abby-normal a chorus:Principal' "$PRINCIPALS"
  grep -q 'holdsRole chorus:role-abby-normal' "$PRINCIPALS"
  grep -q '^chorus:role-abby-normal a chorus:Role, chorus:AgentRole' "$ROLES"
}

@test "NEGATIVE PROOF: a principal whose role block is missing fails the check" {
  grep -v -e '^chorus:role-abby-normal a' "$ROLES" | sed '/^chorus:role-abby-normal a/,/\.$/d' > "$BATS_TEST_TMPDIR/roles.ttl"
  python3 - "$ROLES" "$BATS_TEST_TMPDIR/roles.ttl" <<'PY'
import re, sys
t = open(sys.argv[1]).read()
t = re.sub(r'(?ms)^chorus:role-abby-normal a .*?rolePriority 4 \.\n', '', t)
open(sys.argv[2], 'w').write(t)
PY
  ROLES="$BATS_TEST_TMPDIR/roles.ttl" run check
  echo "$output"
  [ "$status" -eq 1 ]
  printf "%s" "$output" | grep -qF "holds role-abby-normal, which is not declared"
}

@test "NEGATIVE PROOF: an agent principal with no hostAccount fails the check" {
  grep -v 'hostAccount "chorus-abby-normal"' "$PRINCIPALS" > "$BATS_TEST_TMPDIR/principals.ttl"
  PRINCIPALS="$BATS_TEST_TMPDIR/principals.ttl" run check
  echo "$output"
  [ "$status" -eq 1 ]
  printf "%s" "$output" | grep -qF "principal-abby-normal: no hostAccount"
}
