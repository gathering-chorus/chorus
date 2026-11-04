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
 "lossyPattern":"TZ=America/New_York date[^)]*%H:%M"},
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
    def do_POST(self):
        n = int(self.headers.get("Content-Length", 0))
        open(os.path.join(D, "posted"), "w").write(self.path + "\n" + self.rfile.read(n).decode() + "\n" + self.headers.get("Authorization", ""))
        self.send_response(201); self.end_headers()
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

@test "AC5: the steps run inventory, ask, then the bar (a human step), then wire, then retire" {
  Y="$ROOT/platform/pipelines/domain-flow.yaml"
  [ "$(grep -E '^  - name:' "$Y" | sed 's/.*skill-domain-flow-//' | paste -sd' ' -)" = "inventory ask bar wire retire" ] || return 1
  grep -A3 'name: skill-domain-flow-bar' "$Y" | grep -qF 'depends: [skill-domain-flow-ask]' || return 1
  grep -qF 'action: human.task' "$Y" || return 1
  grep -A3 'name: skill-domain-flow-wire' "$Y" | grep -qF 'depends: [skill-domain-flow-bar]' || return 1
  # wire is handed the run, never a typed answerer
  grep -F 'domain-flow wire' "$Y" | grep -qF 'RUN=${DAG_RUN_ID}' || return 1
  ! grep -F 'domain-flow wire' "$Y" | grep -qF 'outputs.by' || return 1
}

@test "AC5 NEGATIVE PROOF: a hand edit that gives the bar to the driver is drift" {
  command -v chorus-make >/dev/null || skip "chorus-make not installed"
  sed 's/(${APPROVER} answers)/(${ROLE} answers)/' "$ROOT/platform/pipelines/domain-flow.yaml" > "$T/df.yaml"
  ! cmp -s "$ROOT/platform/pipelines/domain-flow.yaml" "$T/df.yaml" || return 1
  run chorus-make check-straight domain-flow "$F/domain-flow-rows.tsv" "$F/domain-flow-pipe.tsv" \
    cicd,cicd-test,nightly,athena,pair,demo,card,domain-flow "$T/df.yaml"
  [ "$status" -ne 0 ]
}

# a chorus-go that answers check-step from a file: present = Jeff's passkey signed that run's bar
stub_go() {
  cat > "$T/chorus-go" <<EOF
#!/bin/bash
echo "\$@" >> "$T/go-asked"
[ "\$1" = check-step ] && [ -f "$T/signed-\$3-\$4" ] && { echo signed; exit 0; }
echo "not signed by Jeff's passkey" >&2; exit 1
EOF
  chmod +x "$T/chorus-go"; export DOMAIN_FLOW_GO_BIN="$T/chorus-go"
}

@test "AC5: wire runs on Jeff's passkey-signed bar for this run" {
  stub_go; touch "$T/signed-run1-skill_domain_flow_bar"
  run "$FLOW" wire DOMAIN=time RUN=run1
  out_has "not built yet"
  grep -qF "check-step domain-flow run1 skill_domain_flow_bar" "$T/go-asked" || return 1
}

@test "AC5 NEGATIVE PROOF: a typed by=jeff with no signature is refused; so is another run's signature" {
  stub_go
  run "$FLOW" wire DOMAIN=time RUN=run1 DECISION=agreed BY=jeff
  [ "$status" -eq 3 ]
  out_has "no bar signed by Jeff for time"
  touch "$T/signed-run0-skill_domain_flow_bar"
  run "$FLOW" retire DOMAIN=time RUN=run1
  [ "$status" -eq 3 ]
  run "$FLOW" wire DOMAIN=time
  [ "$status" -eq 3 ]
}

@test "AC5 NEGATIVE PROOF: an answer typed into the step decides nothing; only check-step does" {
  stub_go
  run "$FLOW" wire DOMAIN=time RUN=run1 DECISION=agreed
  [ "$status" -eq 3 ]
  touch "$T/signed-run1-skill_domain_flow_bar"
  run "$FLOW" wire DOMAIN=time RUN=run1 DECISION=rejected
  out_has "not built yet"
}

@test "AC5: ask posts one step row naming this card, run and the bar step, as the role" {
  printf '#!/bin/bash\necho tok-$1\n' > "$T/mint"; chmod +x "$T/mint"
  CHORUS_IDENTITY_TOKEN_BIN="$T/mint" ROLE=silas run "$FLOW" ask DOMAIN=time CARD=4885 RUN=run1
  [ "$status" -eq 0 ]
  out_has "on Jeff's approvals list"
  python3 - "$T/door/posted" <<'EOF' || return 1
import json, sys
path, body, auth = open(sys.argv[1]).read().split("\n", 2)
assert path == "/decisions/approvals", path
assert auth.strip() == "Bearer tok-silas", auth
b = json.loads(body)
assert b["approvalKind"] == "step" and b["name"] == "step-4885-run1", b
assert json.loads(b["approvalAction"]) == {"card": 4885, "dag": "domain-flow", "run": "run1", "step": "skill_domain_flow_bar"}, b
EOF
}

@test "AC5 NEGATIVE PROOF: ask with no identity posts nothing" {
  printf '#!/bin/bash\nexit 1\n' > "$T/mint"; chmod +x "$T/mint"
  CHORUS_IDENTITY_TOKEN_BIN="$T/mint" ROLE=silas run "$FLOW" ask DOMAIN=time CARD=4885 RUN=run1
  [ "$status" -ne 0 ]
  out_has "no identity"
  [ ! -e "$T/door/posted" ] || return 1
}

# --- AC3 (wire, part 1): the shell writes and shows time through one function ----

@test "AC3: boston_time carries its zone in every form" {
  . "$ROOT/platform/scripts/lib/boston-time.sh"
  [[ "$(boston_time stamp)" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}[-+][0-9]{4}$ ]] || return 1
  [[ "$(boston_time show)" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}\ [0-9]{2}:[0-9]{2}\ Boston$ ]] || return 1
  [[ "$(boston_time clock)" =~ ^[0-9]{2}:[0-9]{2}:[0-9]{2}\ Boston$ ]] || return 1
  run boston_time weekday
  [ "$status" -eq 2 ]
}

@test "AC3: no script in the tree writes Boston wall time without its zone" {
  run env DOMAIN_FLOW_ROOT="$ROOT" "$FLOW" inventory DOMAIN=time
  [ "$status" -eq 0 ]
  # code only: the role instructions that tell an agent to run date by hand are the agent-stamp
  # path (AC3 part 2, the injected Now line), not a script
  lossy="$(printf '%s\n' "$output" | awk -F'\t' '$3 == "lossy" && $1 ~ /^(platform|proving|directing)\// && $1 !~ /\.(md|html|json)$/ { print $1 }')"
  [ -z "$lossy" ] || { echo "lossy: $lossy"; return 1; }
  out_has "paths:"
}

@test "AC3 NEGATIVE PROOF: a script that goes back to its own offset-less date is caught" {
  mkdir -p "$T/real"; cp -R "$ROOT/platform/scripts" "$T/real/"
  printf 'echo "$(TZ=America/New_York date '"'"'+%%Y-%%m-%%d %%H:%%M:%%S'"'"') up" >> log\n' > "$T/real/scripts/regressed.sh"
  run env DOMAIN_FLOW_ROOT="$T/real" "$FLOW" inventory DOMAIN=time
  printf '%s\n' "$output" | grep -F "scripts/regressed.sh" | grep -qF lossy || return 1
}

# --- AC4 (retire): one renderer per runtime ----------------------------------

# TS/JS source that formats Boston time itself instead of calling chorus-sdk/time/boston.
# The browser is a runtime too: pages load the same file as a script (globalThis.chorusBoston).
own_ts_renderers() {
  grep -rnE "timeZone: *['\"]America/New_York['\"]|toLocale[A-Za-z]*String\([^)]*America/New_York" \
    --include='*.ts' --include='*.js' --include='*.html' --exclude-dir=node_modules --exclude-dir=dist "$@" 2>/dev/null \
    | grep -vE '\.test\.|/tests/|chorus-sdk/time/' || true
}

@test "AC4: no TypeScript service or browser page formats Boston time itself; all use the one renderer" {
  out="$(own_ts_renderers "$ROOT/platform" "$ROOT/directing")"
  [ -z "$out" ] || { echo "$out"; return 1; }
}

@test "AC4 NEGATIVE PROOF: a service that formats Boston time itself is caught" {
  mkdir -p "$T/svc/src"
  printf "export const t = (d: Date) => d.toLocaleTimeString('en-US', { timeZone: 'America/New_York' });\n" > "$T/svc/src/own.ts"
  out="$(own_ts_renderers "$T/svc")"
  [[ "$out" == *"own.ts"* ]] || return 1
}

@test "AC4: the Rust offset rule lives once, in shared/boston.rs" {
  out="$(grep -rlE 'fn nth_sun(day)?\(' --include='*.rs' "$ROOT/platform/services" | grep -v '/target/' | sed "s|$ROOT/||")"
  [ "$out" = "platform/services/shared/boston.rs" ] || { echo "$out"; return 1; }
}

# The 21 scripts moved onto boston_time (9d2280085), named so a change to any of them runs this.
MOVED="platform/scripts/alert-delivery-test.sh platform/scripts/app-state.sh platform/scripts/authn-coverage.sh
platform/scripts/authz-coverage.sh platform/scripts/bedroom-heartbeat.sh platform/scripts/cards-orphan-reaper.sh
platform/scripts/chat.sh platform/scripts/chorus-ops.sh platform/scripts/daily-review-ops.sh
platform/scripts/daily-review-quality.sh platform/scripts/daily-review-summary.sh platform/scripts/daily-signal-scan.sh
platform/scripts/generate-standards-surface.sh platform/scripts/nudge-health-check.sh platform/scripts/perf-baseline-chorus.sh
platform/scripts/security-probes-run.sh platform/scripts/standards-surface-cron.sh platform/scripts/tmp-reaper.sh
proving/fitness/run-fitness.sh proving/fitness/write-snapshot.sh proving/scripts/alert-runner.sh"

@test "AC3: each moved script parses and loads boston_time from its own folder" {
  for f in $MOVED; do
    bash -n "$ROOT/$f" || { echo "does not parse: $f"; return 1; }
    grep -qF 'boston-time.sh' "$ROOT/$f" || { echo "no boston_time: $f"; return 1; }
    # the source line resolves from the script's own folder (no CHORUS_ROOT needed)
    lib="$(dirname "$ROOT/$f")/$(sed -n 's|^\. "$(dirname "${BASH_SOURCE\[0\]}")/\([^"]*boston-time\.sh\)".*|\1|p' "$ROOT/$f" | head -1)"
    [ -f "$lib" ] || { echo "source path does not resolve: $f -> $lib"; return 1; }
  done
}

@test "AC3: werk-init no longer asks wall-clock to write the clock file" {
  ! grep -qE 'wall-clock" --write' "$ROOT/platform/scripts/werk-init.sh" || return 1
  grep -qF '"$SCRIPT_DIR/wall-clock"' "$ROOT/platform/scripts/werk-init.sh" || return 1
}
