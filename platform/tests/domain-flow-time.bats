#!/usr/bin/env bats
# @test-type: integration — drives platform/scripts/domain-flow against a stub model door and a fixture tree; checks the generated pipeline. No live services.
# @domain: time — the platform domain this suite guards (#4885)
#
# #4885 — Jeff 2026-11-03 18:39 "maybe u dogfood on time first", yes 11-04 03:40.
# The domain flow (inventory → bar → wire → retire) is a dagu pipeline generated
# from its rows, and it reads the domain from the graph (Wren 11-04 03:53), so
# messages can reuse it. AC1 and AC5 here; AC2–AC4 follow Jeff's answer on the bar.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  FLOW="$ROOT/platform/scripts/domain-flow"
  F="$ROOT/platform/services/chorus-make/tests/fixtures"
  T="$BATS_TEST_TMPDIR"
  unset CHORUS_TRACE_ID
  # a small tree with one of each kind of time use, and one file with none
  mkdir -p "$T/tree/scripts" "$T/tree/src" "$T/door"
  printf '#!/bin/bash\nts=$(TZ=America/New_York date "+%%Y-%%m-%%d %%H:%%M")\necho "$ts up" >> log\n' > "$T/tree/scripts/lossy.sh"
  printf 'export const f = (d: Date) => d.toLocaleString("sv-SE", { timeZone: "America/New_York" });\n' > "$T/tree/src/render.ts"
  printf 'let ts = Utc::now().with_timezone(&boston_offset_pub()).to_rfc3339();\n' > "$T/tree/src/stamp.rs"
  printf 'echo nothing about time here\n' > "$T/tree/scripts/plain.sh"
  # the time Domain row as the door serves it, with the #4885 patterns on it
  cat > "$T/door/time.json" <<'EOF'
{"kind":"Domain","id":"chorus:time","data":{"label":"time",
 "pathPattern":["wall-clock","clock_sync","boston_offset","boston\\(","America/New_York"],
 "lossyPattern":"TZ=America/New_York date( |$|['\"])"},
 "links":{"ownedBy":"chorus:principal-silas"}}
EOF
  cat > "$T/door.py" <<'EOF'
import http.server, os, sys, time
D = sys.argv[1]
class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        name = self.path.split("?")[0].rstrip("/").split("/")[-1]
        p = os.path.join(D, name + ".json")
        if self.path.startswith("/v1/domains/domains/") and os.path.exists(p):
            self.send_response(200); self.end_headers(); self.wfile.write(open(p, "rb").read())
        elif self.path.startswith("/loki/api/v1/query") and os.path.exists(os.path.join(D, "loki.json")):
            open(os.path.join(D, "loki-asked"), "w").write(self.path)
            self.send_response(200); self.end_headers(); self.wfile.write(open(os.path.join(D, "loki.json"), "rb").read())
        else:
            self.send_response(404); self.end_headers()
s = http.server.HTTPServer(("127.0.0.1", 0), H)
open(os.path.join(D, "port"), "w").write(str(s.server_address[1]))
s.timeout, end = 1, time.time() + 30
while time.time() < end:
    s.handle_request()
EOF
  python3 "$T/door.py" "$T/door" >/dev/null 2>&1 3>&- &
  for _ in $(seq 1 50); do [ -s "$T/door/port" ] && break; perl -e 'select(undef,undef,undef,0.1)'; done
  export OWL_URL="http://127.0.0.1:$(cat "$T/door/port")" DOMAIN_FLOW_ROOT="$T/tree"
  export DOMAIN_FLOW_LOKI="$OWL_URL"
}

@test "AC1: the inventory carries the domain's 24h Loki traffic, labelled domain-level" {
  printf '{"status":"success","data":{"resultType":"vector","result":[{"metric":{},"value":[0,"536"]}]}}' > "$T/door/loki.json"
  run "$FLOW" inventory DOMAIN=time
  out_has "24h traffic (domain-level"
  out_has ": 536"
  # the query is the row's own patterns, over 24h
  grep -qF "24h" "$T/door/loki-asked" || return 1
  grep -qF "clock_sync" <(python3 -c 'import sys,urllib.parse;print(urllib.parse.unquote_plus(open(sys.argv[1]).read()))' "$T/door/loki-asked") || return 1
}

@test "AC1 NEGATIVE PROOF: a Loki that does not answer reads not measured, never 0" {
  run "$FLOW" inventory DOMAIN=time
  out_has "24h traffic (domain-level"
  out_has "not measured"
  ! out_has ": 0" || return 1
}

out_has() { printf '%s' "$output" | grep -qF -- "$1"; }

# --- AC1: inventory -------------------------------------------------------

@test "AC1: inventory lists every use the domain row's patterns find, each with its file and owner" {
  run "$FLOW" inventory DOMAIN=time
  [ "$status" -eq 0 ]
  out_has "scripts/lossy.sh"
  out_has "src/render.ts"
  out_has "src/stamp.rs"
  printf '%s\n' "$output" | grep -F "src/render.ts" | grep -qF "silas"
  out_has "paths: 3"
}

@test "AC1 NEGATIVE PROOF: a file with no time is never listed, and a missed use changes the count" {
  run "$FLOW" inventory DOMAIN=time
  ! out_has "scripts/plain.sh" || return 1
  printf 'stamp=$(TZ=America/New_York date)\n' > "$T/tree/scripts/another.sh"
  run "$FLOW" inventory DOMAIN=time
  out_has "scripts/another.sh"
  out_has "paths: 4"
}

@test "AC1: an offset-less Boston stamp is marked lossy; an offset stamp is not" {
  run "$FLOW" inventory DOMAIN=time
  printf '%s\n' "$output" | grep -F "scripts/lossy.sh" | grep -qF "lossy"
  ! { printf '%s\n' "$output" | grep -F "src/stamp.rs" | grep -qF "lossy"; }
}

@test "AC1 NEGATIVE PROOF: the patterns come from the graph; change the row and the inventory changes" {
  python3 - "$T/door/time.json" <<'EOF'
import json, sys
p = sys.argv[1]; d = json.load(open(p)); d["data"]["pathPattern"] = ["boston_offset"]; json.dump(d, open(p, "w"))
EOF
  run "$FLOW" inventory DOMAIN=time
  out_has "src/stamp.rs"
  ! out_has "scripts/lossy.sh" || return 1
  out_has "paths: 1"
}

@test "AC1 NEGATIVE PROOF: a domain the graph does not have is refused and nothing is listed" {
  run "$FLOW" inventory DOMAIN=no-such-domain
  [ "$status" -ne 0 ]
  out_has "no domain no-such-domain"
  ! out_has "paths:" || return 1
}

# --- AC5: the flow runs as the domain-flow pipeline ------------------------

@test "AC5: domain-flow.yaml is exactly what chorus-make writes from its rows" {
  command -v chorus-make >/dev/null || skip "chorus-make not installed"
  run chorus-make check-straight domain-flow "$F/domain-flow-rows.tsv" "$F/domain-flow-pipe.tsv" \
    cicd,cicd-test,nightly,athena,pair,demo,card,domain-flow "$ROOT/platform/pipelines/domain-flow.yaml"
  [ "$status" -eq 0 ]
}

@test "AC5: the steps run inventory, then the bar (a human step for the approver), then wire, then retire" {
  Y="$ROOT/platform/pipelines/domain-flow.yaml"
  [ "$(grep -E '^  - name:' "$Y" | sed 's/.*skill-domain-flow-//' | paste -sd' ' -)" = "inventory bar wire retire" ]
  grep -A3 'name: skill-domain-flow-bar' "$Y" | grep -qF 'depends: [skill-domain-flow-inventory]'
  grep -qF 'action: human.task' "$Y"
  grep -qF '(${APPROVER} answers)' "$Y"
  grep -A3 'name: skill-domain-flow-wire' "$Y" | grep -qF 'depends: [skill-domain-flow-bar]'
}

@test "AC5 NEGATIVE PROOF: a hand edit that gives the bar to the driver is drift" {
  command -v chorus-make >/dev/null || skip "chorus-make not installed"
  sed 's/(${APPROVER} answers)/(${ROLE} answers)/' "$ROOT/platform/pipelines/domain-flow.yaml" > "$T/df.yaml"
  ! cmp -s "$ROOT/platform/pipelines/domain-flow.yaml" "$T/df.yaml" || return 1
  run chorus-make check-straight domain-flow "$F/domain-flow-rows.tsv" "$F/domain-flow-pipe.tsv" \
    cicd,cicd-test,nightly,athena,pair,demo,card,domain-flow "$T/df.yaml"
  [ "$status" -ne 0 ]
}

@test "AC5 NEGATIVE PROOF: wire is refused unless Jeff agreed to the bar" {
  run "$FLOW" wire DOMAIN=time BY=silas DECISION=agreed
  [ "$status" -eq 3 ]
  out_has "no bar recorded for time"
  run "$FLOW" wire DOMAIN=time BY=jeff DECISION=rejected
  [ "$status" -eq 3 ]
  run "$FLOW" retire DOMAIN=time
  [ "$status" -eq 3 ]
}
