#!/usr/bin/env bats
# @test-type: unit — the repo's Loki config, Loki's own -verify-config, and today's live /config as a fixture
# @domain: logs
# @card: #4983 · owner: silas
# Checks platform/config/loki/loki.yaml, platform/scripts/land-4983-rows and
# platform/scripts/launchagents-canonical/com.gathering.loki.plist.
#
# #4983 AC2: the limits change in the one Loki config file in the repo, each value with its reason
# beside it; the running Loki reports them at /config after the land. The land (land-4983-rows)
# reloads Loki on this file and refuses, keeping the old files, unless /config shows every value.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  CONF="$ROOT/platform/config/loki/loki.yaml"
  PLIST="$ROOT/platform/scripts/launchagents-canonical/com.gathering.loki.plist"
  # the binary the agent runs, read from the plist (never retyped)
  LOKI_BIN="${LOKI_BIN:-$(plutil -extract ProgramArguments.0 raw -o - "$PLIST")}"
}

# land-4983-rows' missing(), on a /config text on stdin
missing() {
  python3 -c '
import importlib.machinery, importlib.util, sys
l = importlib.machinery.SourceFileLoader("l4983", sys.argv[1])
m = importlib.util.module_from_spec(importlib.util.spec_from_loader("l4983", l)); l.exec_module(m)
left = m.missing(sys.stdin.read())
print("\n".join(left)); sys.exit(1 if left else 0)' "$ROOT/platform/scripts/land-4983-rows"
}

@test "each #4983 value sits in the repo file with its reason above it" {
  for key in cache_results parallelise_shardable_queries split_queries_by_interval max_outstanding_requests_per_tenant; do
    python3 - "$CONF" "$key" <<'EOF' || return 1
import sys
lines = open(sys.argv[1]).read().splitlines()
i = next((n for n, l in enumerate(lines) if l.strip().startswith(sys.argv[2] + ":")), None)
if i is None or not any("#4983" in lines[j] for j in range(max(0, i - 4), i)):
    print(f"{sys.argv[2]}: no value, or no #4983 reason in the lines above it"); sys.exit(1)
EOF
  done
}

@test "Loki accepts the file and would run with all four values" {
  [ -x "$LOKI_BIN" ] || { echo "no loki binary at $LOKI_BIN"; return 1; }
  run "$LOKI_BIN" -config.file="$CONF" -verify-config -print-config-stderr
  [ "$status" -eq 0 ] || { echo "$output" | tail -5; return 1; }
  printf '%s\n' "$output" | missing || return 1
}

# NEGATIVE PROOF: today's live /config (before the land) is refused, naming each old value
@test "the land refuses Loki's config of 2026-11-15, naming every value it lacks" {
  run missing < "$ROOT/platform/tests/fixtures/4983/config-before.yaml"
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  [[ "$output" == *"max_outstanding_requests_per_tenant: want 1024, /config has 100"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"cache_results: want true, /config has false"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"parallelise_shardable_queries: want false, /config has true"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"split_queries_by_interval: want 2h"* ]] || { echo "$output"; return 1; }
}

@test "the canonical plist starts Loki on the repo file; a dry run changes nothing" {
  arg="$(plutil -extract ProgramArguments.1 raw -o - "$PLIST")"
  [[ "$arg" == -config.file=*/chorus/platform/config/loki/loki.yaml ]] || { echo "plist config: $arg"; return 1; }
  if grep -q "shared-observability/config/loki" "$PLIST"; then echo "plist still names the old file"; return 1; fi
  run python3 "$ROOT/platform/scripts/land-4983-rows"
  [ "$status" -eq 0 ] && [[ "$output" == plan:* ]] || { echo "$output"; return 1; }
}

new_config() { printf 'max_outstanding_requests_per_tenant: 1024\nsplit_queries_by_interval: 2h\ncache_results: true\nparallelise_shardable_queries: false\n' > "$BATS_TEST_TMPDIR/config"; }

# the land against a fake launchctl (records its calls) and a fake Loki serving $T/config
fake_land() {
  T="$BATS_TEST_TMPDIR"
  mkdir -p "$T/agents" "$T/obs/config/loki"
  printf 'OLD PLIST\n' > "$T/agents/com.gathering.loki.plist"
  printf 'live\n' > "$T/obs/config/loki/loki-native.yaml"; printf 'dead\n' > "$T/obs/config/loki/loki-config.yaml"
  git -C "$T/obs" init -q && git -C "$T/obs" add -A && git -C "$T/obs" -c user.name=t -c user.email=t@t commit -qm init
  printf '#!/bin/sh\necho "$*" >> "%s/launchctl.log"\n[ "$1" = print ] && exit 113\nexit 0\n' "$T" > "$T/launchctl"; chmod +x "$T/launchctl"
  PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
  python3 - "$PORT" "$T/config" <<'PY' &
import http.server, sys
port, conf = int(sys.argv[1]), sys.argv[2]
import os, time
mode = open(os.path.join(os.path.dirname(conf), "mode")).read().strip() if os.path.exists(os.path.join(os.path.dirname(conf), "mode")) else "ok"
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path.startswith("/loki/api/v1/query"):
            if mode == "refuse":
                self.send_response(429); self.end_headers(); self.wfile.write(b"too many outstanding requests"); return
            if mode == "slow":
                time.sleep(1.5)
            body = b'{"status":"success","data":{"result":[]}}' if mode == "empty" else b'{"status":"success","data":{"result":[{"stream":{},"values":[["1","x"]]}]}}'
            self.send_response(200); self.end_headers(); self.wfile.write(body); return
        body = b"ready" if self.path == "/ready" else open(conf, "rb").read()
        self.send_response(200); self.end_headers(); self.wfile.write(body)
    def log_message(self, *a): pass
http.server.ThreadingHTTPServer(("127.0.0.1", port), H).serve_forever()
PY
  SRV=$!
  for _ in $(seq 1 20); do curl -sf -o /dev/null "http://127.0.0.1:$PORT/ready" && break; sleep 0.2; done
  CHORUS_HOME="$ROOT" LAND_AS_UID="${LAND_AS_UID:-$(id -u)}" OBS_REPO="$T/obs" LAUNCH_AGENTS_DIR="$T/agents" LAUNCHCTL="$T/launchctl" \
    LOKI_URL="http://127.0.0.1:$PORT" LOKI_READY_WAIT=3 LOKI_SLOWEST_MAX="${LOKI_SLOWEST_MAX:-60}" GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t \
    GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t run python3 "$ROOT/platform/scripts/land-4983-rows" --apply
  kill "$SRV" 2>/dev/null; wait "$SRV" 2>/dev/null || true
}

@test "on the new limits the land keeps the new plist and removes both old files" {
  new_config
  fake_land
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  cmp -s "$BATS_TEST_TMPDIR/agents/com.gathering.loki.plist" "$ROOT/platform/scripts/launchagents-canonical/com.gathering.loki.plist" || { echo "new plist not installed"; return 1; }
  [ ! -e "$BATS_TEST_TMPDIR/obs/config/loki/loki-native.yaml" ] && [ ! -e "$BATS_TEST_TMPDIR/obs/config/loki/loki-config.yaml" ] || { echo "old files left"; return 1; }
  [ -z "$(git -C "$BATS_TEST_TMPDIR/obs" status --porcelain)" ] || { git -C "$BATS_TEST_TMPDIR/obs" status; return 1; }
}

# NEGATIVE PROOF: Loki comes up on the old limits: the old plist goes back and is reloaded, old files stay
@test "a Loki not on the new limits gets its old plist back, reloaded, and keeps its old files" {
  cp "$ROOT/platform/tests/fixtures/4983/config-before.yaml" "$BATS_TEST_TMPDIR/config"
  fake_land
  [ "$status" -eq 1 ] && [[ "$output" == *"old plist restored, Loki ready"* ]] || { echo "$output"; return 1; }
  [ "$(cat "$BATS_TEST_TMPDIR/agents/com.gathering.loki.plist")" = "OLD PLIST" ] || { echo "old plist not restored"; return 1; }
  [ "$(grep -c '^bootstrap' "$BATS_TEST_TMPDIR/launchctl.log")" -eq 2 ] || { cat "$BATS_TEST_TMPDIR/launchctl.log"; return 1; }
  [ -e "$BATS_TEST_TMPDIR/obs/config/loki/loki-native.yaml" ] && [ -e "$BATS_TEST_TMPDIR/obs/config/loki/loki-config.yaml" ] || { echo "old files removed"; return 1; }
}

# NEGATIVE PROOF: a land running as an account that does not own the agent changes nothing
@test "a land whose account does not own the Loki agent refuses and touches nothing" {
  new_config
  LAND_AS_UID=99999 fake_land
  [ "$status" -eq 2 ] && [[ "$output" == *"this land runs as 99999; nothing changed"* ]] || { echo "$output"; return 1; }
  [ "$(cat "$BATS_TEST_TMPDIR/agents/com.gathering.loki.plist")" = "OLD PLIST" ] || { echo "plist changed"; return 1; }
  [ ! -s "$BATS_TEST_TMPDIR/launchctl.log" ] || { cat "$BATS_TEST_TMPDIR/launchctl.log"; return 1; }
  [ -e "$BATS_TEST_TMPDIR/obs/config/loki/loki-native.yaml" ] || { echo "old file removed"; return 1; }
}


# NEGATIVE PROOF: on the new limits but the live replay is refused: the old plist goes back
@test "a refused live replay restores the old plist and keeps the old files" {
  new_config; echo refuse > "$BATS_TEST_TMPDIR/mode"
  fake_land
  [ "$status" -eq 1 ] && [[ "$output" == *"the live replay is not clean (refused=28"* && "$output" == *"old plist restored, Loki ready"* ]] || { echo "$output"; return 1; }
  [ "$(cat "$BATS_TEST_TMPDIR/agents/com.gathering.loki.plist")" = "OLD PLIST" ] || { echo "old plist not restored"; return 1; }
  [ -e "$BATS_TEST_TMPDIR/obs/config/loki/loki-native.yaml" ] || { echo "old file removed"; return 1; }
}

# NEGATIVE PROOF: every read answers, but slower than allowed: the old plist goes back
@test "a live replay slower than the allowed wait restores the old plist" {
  new_config; echo slow > "$BATS_TEST_TMPDIR/mode"
  LOKI_SLOWEST_MAX=1 fake_land
  [ "$status" -eq 1 ] && [[ "$output" == *"refused=0 failed=0 answered=28 empty=0"* && "$output" == *"slowest allowed 1 s"* && "$output" == *"old plist restored"* ]] || { echo "$output"; return 1; }
  [ "$(cat "$BATS_TEST_TMPDIR/agents/com.gathering.loki.plist")" = "OLD PLIST" ] || { echo "old plist not restored"; return 1; }
}

# NEGATIVE PROOF: an old file with uncommitted edits stops the land before anything switches
@test "an old config with uncommitted edits refuses before the switch" {
  new_config
  T="$BATS_TEST_TMPDIR"; mkdir -p "$T/obs/config/loki"
  printf 'live\n' > "$T/obs/config/loki/loki-native.yaml"; printf 'dead\n' > "$T/obs/config/loki/loki-config.yaml"
  git -C "$T/obs" init -q && git -C "$T/obs" add -A && git -C "$T/obs" -c user.name=t -c user.email=t@t commit -qm init
  printf 'edited\n' >> "$T/obs/config/loki/loki-native.yaml"
  mkdir -p "$T/agents"; printf 'OLD PLIST\n' > "$T/agents/com.gathering.loki.plist"
  printf '#!/bin/sh\necho "$*" >> "%s/launchctl.log"\n[ "$1" = print ] && exit 113\nexit 0\n' "$T" > "$T/launchctl"; chmod +x "$T/launchctl"
  CHORUS_HOME="$ROOT" OBS_REPO="$T/obs" LAUNCH_AGENTS_DIR="$T/agents" LAUNCHCTL="$T/launchctl" \
    run python3 "$ROOT/platform/scripts/land-4983-rows" --apply
  [ "$status" -eq 2 ] && [[ "$output" == *"has uncommitted edits; nothing changed"* ]] || { echo "$output"; return 1; }
  [ ! -s "$T/launchctl.log" ] && [ "$(cat "$T/agents/com.gathering.loki.plist")" = "OLD PLIST" ] || { echo "something switched"; return 1; }
}

# NEGATIVE PROOF: every read answers but matches nothing on the live store: a hollow pass, restored
@test "a live replay whose reads match nothing restores the old plist" {
  new_config; echo empty > "$BATS_TEST_TMPDIR/mode"
  fake_land
  [ "$status" -eq 1 ] && [[ "$output" == *"empty=28"* && "$output" == *"old plist restored"* ]] || { echo "$output"; return 1; }
  [ "$(cat "$BATS_TEST_TMPDIR/agents/com.gathering.loki.plist")" = "OLD PLIST" ] || { echo "old plist not restored"; return 1; }
}

# NEGATIVE PROOF: an old config file git does not track stops the land before anything switches
@test "an untracked old config refuses before the switch" {
  new_config
  T="$BATS_TEST_TMPDIR"; mkdir -p "$T/obs/config/loki"
  printf 'live\n' > "$T/obs/config/loki/loki-native.yaml"
  git -C "$T/obs" init -q && git -C "$T/obs" add -A && git -C "$T/obs" -c user.name=t -c user.email=t@t commit -qm init
  printf 'dead\n' > "$T/obs/config/loki/loki-config.yaml"
  mkdir -p "$T/agents"; printf 'OLD PLIST\n' > "$T/agents/com.gathering.loki.plist"
  CHORUS_HOME="$ROOT" OBS_REPO="$T/obs" LAUNCH_AGENTS_DIR="$T/agents" LAUNCHCTL=/usr/bin/false \
    run python3 "$ROOT/platform/scripts/land-4983-rows" --apply
  [ "$status" -eq 2 ] && [[ "$output" == *"loki-config.yaml is untracked or has uncommitted edits; nothing changed"* ]] || { echo "$output"; return 1; }
  [ "$(cat "$T/agents/com.gathering.loki.plist")" = "OLD PLIST" ] || { echo "plist changed"; return 1; }
}

# NEGATIVE PROOF: launchd never lets go of the label: no bootstrap over it, the old plist goes back
@test "a label launchd still holds after bootout is not bootstrapped over" {
  new_config
  fake_land_holding() {
    T="$BATS_TEST_TMPDIR"; mkdir -p "$T/agents" "$T/obs/config/loki"
    printf 'OLD PLIST\n' > "$T/agents/com.gathering.loki.plist"
    printf 'live\n' > "$T/obs/config/loki/loki-native.yaml"; printf 'dead\n' > "$T/obs/config/loki/loki-config.yaml"
    git -C "$T/obs" init -q && git -C "$T/obs" add -A && git -C "$T/obs" -c user.name=t -c user.email=t@t commit -qm init
    printf '#!/bin/sh\necho "$*" >> "%s/launchctl.log"\nexit 0\n' "$T" > "$T/launchctl"; chmod +x "$T/launchctl"
    CHORUS_HOME="$ROOT" OBS_REPO="$T/obs" LAUNCH_AGENTS_DIR="$T/agents" LAUNCHCTL="$T/launchctl" LOKI_URL=http://127.0.0.1:9 \
      LOKI_READY_WAIT=1 LAUNCHD_RELEASE_WAIT=2 run python3 "$ROOT/platform/scripts/land-4983-rows" --apply
  }
  fake_land_holding
  [ "$status" -eq 1 ] && [[ "$output" == *"still holds com.gathering.loki"* && "$output" == *"RESTORE FAILED"* ]] || { echo "$output"; return 1; }
  [ "$(grep -c '^bootstrap' "$BATS_TEST_TMPDIR/launchctl.log")" -eq 0 ] || { cat "$BATS_TEST_TMPDIR/launchctl.log"; return 1; }
  [ "$(cat "$BATS_TEST_TMPDIR/agents/com.gathering.loki.plist")" = "OLD PLIST" ] || { echo "old plist not written back"; return 1; }
}
