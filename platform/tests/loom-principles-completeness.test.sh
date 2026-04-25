#!/usr/bin/env bats
# #2451 — loom-principles SubDomain completeness contract.
#
# Asserts that the populated subdomain meets all 3 lifecycle gates
# (create / wip / done) so it can serve as the reference shape for
# every other domain to follow (first domain to hit 7/7 MUST-haves).

API_BASE="${API_BASE:-http://localhost:3340}"
SUBDOMAIN="loom-principles"

@test "loom-principles completeness API responds" {
  CODE=$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 "${API_BASE}/api/athena/subdomains/${SUBDOMAIN}/completeness")
  [ "$CODE" = "200" ]
}

@test "create lifecycle stage passes" {
  PASS=$(curl -s --max-time 5 "${API_BASE}/api/athena/subdomains/${SUBDOMAIN}/completeness" \
    | python3 -c "import json,sys; print(json.load(sys.stdin)['data']['lifecycle']['create']['pass'])")
  [ "$PASS" = "True" ]
}

@test "wip lifecycle stage passes (actors populated)" {
  PASS=$(curl -s --max-time 5 "${API_BASE}/api/athena/subdomains/${SUBDOMAIN}/completeness" \
    | python3 -c "import json,sys; print(json.load(sys.stdin)['data']['lifecycle']['wip']['pass'])")
  [ "$PASS" = "True" ]
}

@test "done lifecycle stage passes (scenarios + contract populated)" {
  PASS=$(curl -s --max-time 5 "${API_BASE}/api/athena/subdomains/${SUBDOMAIN}/completeness" \
    | python3 -c "import json,sys; print(json.load(sys.stdin)['data']['lifecycle']['done']['pass'])")
  [ "$PASS" = "True" ]
}

@test "subdomain has at least 3 actors" {
  COUNT=$(curl -s --max-time 5 "${API_BASE}/api/athena/subdomains/${SUBDOMAIN}/actors" \
    | python3 -c "import json,sys; d=json.load(sys.stdin); print(len(d.get('data',d).get('actors',[])))")
  [ "$COUNT" -ge 3 ]
}

@test "subdomain has at least 3 scenarios" {
  COUNT=$(curl -s --max-time 5 "${API_BASE}/api/athena/subdomains/${SUBDOMAIN}/scenarios" \
    | python3 -c "import json,sys; d=json.load(sys.stdin); print(len(d.get('data',d).get('scenarios',[])))")
  [ "$COUNT" -ge 3 ]
}

@test "subdomain has a contract section" {
  COUNT=$(curl -s --max-time 5 "${API_BASE}/api/athena/subdomains/${SUBDOMAIN}/contract" \
    | python3 -c "import json,sys; d=json.load(sys.stdin); print(len(d.get('data',{}).get('endpoints',[])))")
  [ "$COUNT" -ge 1 ]
}
