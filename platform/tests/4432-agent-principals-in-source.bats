#!/usr/bin/env bats
# @test-type: unit — reads three model source files, no store, no network
# @domain: identity — the principals and the roles they hold (#4432)
#
# #4432 — Abby Normal's role and principal existed only in the live store
# (minted 2026-09-17), so a reseed would have dropped her. These cases hold
# the source to the rule that every agent principal can be rebuilt from it:
# its role is declared, typed AgentRole, names both word caps (the property
# rows themselves live in the store, not the seed — #4432), and the
# principal names the Mac account it runs as.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  PRINCIPALS="${PRINCIPALS_TTL:-$ROOT/roles/silas/ontology/identity-principals-3613.ttl}"
  ROLES="${ROLES_TTL:-$ROOT/roles/wren/ontology/role-instances-3838.ttl}"
}

check() {
  python3 - "$PRINCIPALS" "$ROLES" <<'PY'
import re, sys
P, R = (open(f).read() for f in sys.argv[1:3])
def blocks(t):
    t = re.sub(r'(?m)^\s*#.*$', '', t)
    return {m.group(1): m.group(2) for m in re.finditer(r'(?ms)^chorus:([\w-]+) a ([^.]*?(?:"[^"]*"[^.]*?)*)\.\s*$', t)}
pb, rb = blocks(P), blocks(R)
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
        # New rows follow the ADR-040 mint table (property-…); the three
        # original roles' prop-… names are grandfathered by the seed's iri-guard.
        names = [f"property-{role}-{cap}-word-cap", f"prop-{role}-{cap}-word-cap"]
        prop = next((n for n in names if f"chorus:{role} chorus:hasProperty chorus:{n} ." in R), None)
        if prop is None:
            bad.append(f"{role}: no hasProperty {names[0]}"); continue

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

wears_four() {
  for h in product-manager solutions-architect engineering-lead operations-lead; do
    python3 - "$1" "$h" <<'PY' || { echo "missing hat-$h"; return 1; }
import re, sys
t = open(sys.argv[1]).read()
b = re.search(r'(?ms)^chorus:principal-abby-normal a .*?canSignIn "false" \.', t).group(0)
sys.exit(0 if f"chorus:hat-{sys.argv[2]}" in b and "chorus:wearsHat" in b else 1)
PY
  done
}

@test "abby-normal wears the four default hats (Jeff 2026-09-17)" {
  wears_four "$PRINCIPALS"
}

@test "NEGATIVE PROOF: a principal missing one hat fails the hat check" {
  sed 's/chorus:hat-engineering-lead, //' "$PRINCIPALS" > "$BATS_TEST_TMPDIR/p.ttl"
  run wears_four "$BATS_TEST_TMPDIR/p.ttl"
  [ "$status" -ne 0 ]
  printf "%s" "$output" | grep -qF "missing hat-engineering-lead"
}

check_abby() {
  python3 - "$1" <<'PY'
import os, subprocess, re, sys

target_dir = sys.argv[1]
allowed_radius = {
    "chorus:clearing",
    "chorus:cards",
    "chorus:messages",
}

if os.path.exists(os.path.join(target_dir, ".git")):
    try:
        out = subprocess.check_output(
            ["git", "grep", "-nE", r"(ownedBy|appointee|appointedPrincipal|appointedRole)[^.;]*(role|principal)-abby-normal", "--", "*.ttl"],
            cwd=target_dir
        ).decode("utf-8")
    except subprocess.CalledProcessError:
        out = ""
    lines = [l for l in out.splitlines() if l.strip()]
    files_with_lines = []
    for l in lines:
        parts = l.split(":", 2)
        files_with_lines.append((os.path.join(target_dir, parts[0]), int(parts[1])))
else:
    files_with_lines = []
    for r, ds, fs in os.walk(target_dir):
        for f in fs:
            if f.endswith(".ttl"):
                p = os.path.join(r, f)
                with open(p, "r", errors="ignore") as fp:
                    for idx, line in enumerate(fp, 1):
                        if re.search(r'(ownedBy|appointee|appointedPrincipal|appointedRole)[^.;]*(role|principal)-abby-normal', line):
                            files_with_lines.append((p, idx))

if not files_with_lines:
    print("abby-normal owns nothing (expected Clearing / product-radius)")
    sys.exit(1)

bad = []
for p, lno in files_with_lines:
    with open(p, "r", errors="ignore") as fp:
        flines = fp.readlines()
    subj = None
    for i in range(lno - 1, -1, -1):
        l = flines[i].strip()
        if i < lno - 1 and l.endswith("."):
            break
        m = re.match(r'^(chorus:[a-zA-Z0-9_-]+)\s+a\s+', l)
        if m:
            subj = m.group(1); break
        m2 = re.match(r'^(chorus:[a-zA-Z0-9_-]+)\s+chorus:(ownedBy|appointee|appointedPrincipal|appointedRole)', l)
        if m2:
            subj = m2.group(1); break
    if subj not in allowed_radius:
        bad.append(f"outside Clearing product-radius: {subj} at {p}:{lno}")

if bad:
    print("\n".join(bad))
    sys.exit(1)

print(f"ok ({len(files_with_lines)} references in Clearing product-radius)")
sys.exit(0)
PY
}

@test "abby-normal owns Clearing and its product-radius, nothing outside (Jeff 2026-10-21 18:16)" {
  run check_abby "$ROOT"
  echo "$output"
  [ "$status" -eq 0 ]
  if curl -s --max-time 1 http://localhost:3030/pods/query >/dev/null 2>&1; then
    run curl -s --data-urlencode 'query=PREFIX c: <https://jeffbridwell.com/chorus#> ASK { GRAPH <urn:chorus:domains:products> { c:clearing c:ownedBy c:principal-abby-normal } }' http://localhost:3030/pods/query
    echo "prod live check: $output"
    printf '%s' "$output" | grep -qF '"boolean" : true'
  fi
}

@test "NEGATIVE PROOF: an ownedBy outside Clearing radius is caught" {
  mkdir -p "$BATS_TEST_TMPDIR/repo"
  printf 'chorus:x a chorus:Domain ;\n    chorus:ownedBy chorus:principal-abby-normal .\n' > "$BATS_TEST_TMPDIR/repo/x.ttl"
  run check_abby "$BATS_TEST_TMPDIR/repo"
  echo "$output"
  [ "$status" -ne 0 ]
  printf "%s" "$output" | grep -qF "outside Clearing product-radius: chorus:x"
}
