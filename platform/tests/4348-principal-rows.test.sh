#!/usr/bin/env bash
# @test-type: unit — reads the principal TTL and the Principal shape; no store, no network
# @domain: identity
#
# #4348 — Principal rows complete (Login ER, Jeff 2026-09-27 "is login er done").
# Every principal that runs on the Library says which host account it runs as,
# and principalKind has one word per kind: person / agent / service (Jeff's
# Ruling 2, 2026-09-21: "human" means person, "worker" means service).
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ROWS="${PRINCIPAL_ROWS:-$ROOT/roles/silas/ontology/identity-principals-3613.ttl}"
SHAPE="$ROOT/roles/silas/ontology/security-model-3618.ttl"
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
pass=0; fail=0
ok() { echo "PASS $1"; pass=$((pass+1)); }; bad() { echo "FAIL $1"; fail=$((fail+1)); }
command -v riot >/dev/null || { echo "FAIL: riot not on PATH, nothing checked"; exit 1; }
# principals that never run on this machine carry no host account, by name
REMOTE="marknakib"
check_rows() { # $1 ttl → prints problems
  riot --output=nt "$1" 2>/dev/null | python3 -c '
import sys,re
C="https://jeffbridwell.com/chorus#"; rows={}
for l in sys.stdin:
    m=re.match(r"<([^>]+)> <([^>]+)> (.+) \.$",l.strip())
    if not m: continue
    s,p,o=m.groups()
    if not s.startswith(C+"principal-"): continue
    r=rows.setdefault(s[len(C)+10:],{})
    if p==C+"principalKind": r["kind"]=o.strip("\"")
    if p==C+"hostAccount": r["host"]=o.strip("\"")
remote=set(sys.argv[1].split())
for n,r in sorted(rows.items()):
    k=r.get("kind"); h=r.get("host")
    if k not in ("person","agent","service"): print(f"{n}: principalKind {k!r} is not person/agent/service")
    if n in remote and h: print(f"{n}: never runs here but has hostAccount {h}")
    if n not in remote and not r.get("host"): print(f"{n}: runs here but has no hostAccount")
print(f"#rows {len(rows)}")
' "$REMOTE"
}
out=$(check_rows "$ROWS"); probs=$(printf '%s\n' "$out" | grep -v '^#rows' || true); n=$(printf '%s\n' "$out" | sed -n 's/^#rows //p')
[ "${n:-0}" -ge 10 ] && ok "read $n principal rows (not an empty set)" || bad "read ${n:-0} principal rows"
[ "${n:-0}" -ge 1 ] && [ -z "$probs" ] && ok "every principal: one of person/agent/service, and a host account unless it never runs here" || { bad "principal rows:"; echo "$probs" | sed 's/^/    /'; }
# NEGATIVE PROOF: a "human" row and a local row with no host account are both caught
printf '@prefix c: <https://jeffbridwell.com/chorus#> .\nc:principal-deb c:principalKind "human" ; c:hostAccount "jeffbridwell" .\nc:principal-x c:principalKind "agent" .\n' > "$T/bad.ttl"
b=$(check_rows "$T/bad.ttl")
printf '%s' "$b" | grep -q 'deb: principalKind .human.' && printf '%s' "$b" | grep -q 'x: runs here but has no hostAccount' && ok "negative proof: \"human\" and a missing host account are both caught" || bad "negative proof missed: $b"
# the shape refuses anything but the three words (so a new "human" is refused at write)
pat=$(grep -A3 'PrincipalShape-principalKind a sh:PropertyShape' "$SHAPE" | sed -n 's/.*sh:pattern "\([^"]*\)".*/\1/p')
[ -n "$pat" ] || { bad "PrincipalShape-principalKind has no sh:pattern"; pat='.*'; }
res=$(python3 -c 'import re,sys;p=sys.argv[1];print(" ".join(w for w in ["person","agent","service","human","worker"] if re.search(p,w)))' "$pat")
[ "$res" = "person agent service" ] && ok "the shape accepts person/agent/service and refuses human and worker" || bad "shape pattern $pat accepts: $res"
echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
