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
  ROLES_DOOR="${ROLES_DOOR:-$ROOT/platform/tests/fixtures/roles-domain.json}"
}

check() {
  python3 - "$PRINCIPALS" "$ROLES_DOOR" <<'PY'
import json, os, re, sys, urllib.request

P = open(sys.argv[1]).read()
roles_source = sys.argv[2]

if os.path.exists(roles_source):
    with open(roles_source) as fp:
        roles_doc = json.load(fp)
else:
    req = urllib.request.Request(roles_source, headers={"Accept": "application/json"})
    with urllib.request.urlopen(req, timeout=5) as resp:
        roles_doc = json.load(resp)

roles = {r.get("name"): r for r in roles_doc.get("data", [])}

def blocks(t):
    t = re.sub(r'(?m)^\s*#.*$', '', t)
    return {m.group(1): m.group(2) for m in re.finditer(r'(?ms)^chorus:([\w-]+) a ([^.]*?(?:"[^"]*"[^.]*?)*)\.\s*$', t)}

pb = blocks(P)
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
    role_key = re.sub(r'^role-', '', role)
    if role_key not in roles and role not in roles:
        bad.append(f"{name}: holds {role}, which has no row in the roles domain"); continue
    r = roles.get(role_key) or roles.get(role)
    if r.get("roleKind") != "agent":
        bad.append(f"{role}: roleKind is not agent in the roles domain")

print("\n".join(bad) if bad else "ok")
sys.exit(1 if bad else 0)
PY
}

@test "every agent principal in source holds a role in the roles domain and a host account" {
  run check
  echo "$output"
  [ "$status" -eq 0 ]
}

@test "abby-normal is one of them" {
  grep -q '^chorus:principal-abby-normal a chorus:Principal' "$PRINCIPALS"
  grep -q 'holdsRole chorus:role-abby-normal' "$PRINCIPALS"
  run check
  [ "$status" -eq 0 ]
}

# #4691 — Borg Queen was minted live 2026-10-22 06:30 (create) and never in
# source, Abby's #4432 gap again: her live row had no hostAccount, holdsRole or
# agentRuntime, and a session cannot write a Principal (403 deploy-only).
@test "borg-queen is one of them, on agy as her own Mac account (#4691)" {
  b=$(python3 -c 'import re,sys; m=re.search(r"(?ms)^chorus:principal-borg-queen a .*?canSignIn \"false\" \.", open(sys.argv[1]).read()); print(m.group(0) if m else "")' "$PRINCIPALS")
  [ -n "$b" ] || { echo "no principal-borg-queen block"; return 1; }
  for want in 'holdsRole chorus:role-borg-queen' 'hostAccount "chorus-borg-queen"' 'agentRuntime chorus:runtime-agy' 'principalKind "agent"' 'webId "https://id.lightlifeurbangardens.com/borg-queen/profile/card#me"'; do
    [[ "$b" == *"$want"* ]] || { echo "missing: $want"; return 1; }
  done
  run check
  echo "$output"
  [ "$status" -eq 0 ]
}

@test "NEGATIVE PROOF: an agent principal with no roles-domain row fails" {
  python3 -c '
import json, sys, urllib.request
src = sys.argv[1]
if __import__("os").path.exists(src):
    data = json.load(open(src))
else:
    data = json.load(urllib.request.urlopen(src, timeout=5))
data["data"] = [r for r in data.get("data", []) if r.get("name") != "abby-normal"]
with open("'"$BATS_TEST_TMPDIR/roles-missing-abby.json"'", "w") as fp:
    json.dump(data, fp)
' "$ROLES_DOOR"
  ROLES_DOOR="$BATS_TEST_TMPDIR/roles-missing-abby.json" run check
  echo "$output"
  [ "$status" -eq 1 ]
  printf "%s" "$output" | grep -qF "holds role-abby-normal, which has no row in the roles domain"
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
allowed_radius = {"chorus:clearing"}
p_file = os.path.join(target_dir, "designing/data/product-instances.ttl")
if os.path.exists(p_file):
    with open(p_file) as fp:
        p_text = fp.read()
    m_clr = re.search(r'(?ms)^chorus:clearing a chorus:Product\s*;\s*(.*?)\.\s*$', p_text)
    if m_clr:
        m_dom = re.search(r'chorus:hasDomain\s+([^;]+);', m_clr.group(1))
        if m_dom:
            allowed_radius.update(d.strip() for d in m_dom.group(1).split(",") if d.strip())

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
}

@test "NEGATIVE PROOF: an ownedBy outside Clearing radius is caught" {
  mkdir -p "$BATS_TEST_TMPDIR/repo"
  printf 'chorus:x a chorus:Domain ;\n    chorus:ownedBy chorus:principal-abby-normal .\n' > "$BATS_TEST_TMPDIR/repo/x.ttl"
  run check_abby "$BATS_TEST_TMPDIR/repo"
  echo "$output"
  [ "$status" -ne 0 ]
  printf "%s" "$output" | grep -qF "outside Clearing product-radius: chorus:x"
}
