#!/usr/bin/env bats
# @test-type: smoke — live: main accepts only the lander App's jeff-go
# @domain: security
# @needs: machine — GitHub, after the land, the lander install and Jeff turning protection on
# @card: #4710 · owner: wren
# What Jeff sees: only his Face ID go lands a card. main requires the `jeff-go` check from the
# lander's GitHub App (app id 5719867) and admins are held to it, so a jeff-go status posted by
# any other identity (github-deploy can still post statuses, Jeff 18:01) does not satisfy main.
APP_ID="${LANDER_APP_ID:-5719867}"
REPO="${CHORUS_REPO:-gathering-chorus/chorus}"

# prints why a branches/main answer ($1, JSON) lets a jeff-go from someone else through, if it does
problems() {
  python3 - "$1" "$APP_ID" <<'PY'
import json, sys
b = json.loads(sys.argv[1]); app = int(sys.argv[2])
p = b.get("protection") or {}
if not b.get("protected"): print("main is not protected")
checks = (p.get("required_status_checks") or {}).get("checks") or []
mine = [c for c in checks if c.get("context") == "jeff-go"]
if not mine: print("main does not require jeff-go")
for c in mine:
    if c.get("app_id") != app: print(f"jeff-go is satisfied by app {c.get('app_id')}, not the lander's {app}")
PY
}

@test "AC2: main requires jeff-go from the lander's App, and no one else's satisfies it" {
  command -v gh >/dev/null || { echo "unmeasured: no gh"; return 1; }
  run gh api "repos/$REPO/branches/main"
  [ "$status" -eq 0 ] || { echo "unmeasured: $output"; return 1; }
  out="$(problems "$output")"
  [ -z "$out" ] || { echo "$out"; return 1; }
}

@test "AC2 negative proof: jeff-go accepted from any app (app_id null) is caught" {
  out="$(problems '{"protected":true,"protection":{"required_status_checks":{"checks":[{"context":"jeff-go","app_id":null}]}}}')"
  [[ "$out" == *"satisfied by app None, not the lander's"* ]] || { echo "$out"; return 1; }
}

@test "AC2 negative proof: today's main (no jeff-go required) is caught" {
  out="$(problems '{"protected":false,"protection":{}}')"
  [[ "$out" == *"main is not protected"* && "$out" == *"does not require jeff-go"* ]] || { echo "$out"; return 1; }
}
