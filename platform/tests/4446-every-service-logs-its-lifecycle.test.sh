#!/bin/bash
# @test-type: fitness — its subject is the source: is every service wired?
# @domain: services
#
# #4446 — Jeff, 2026-10-06: "all services must be rigorous about structured
# logging of starts stops and failures". 4446-service-lifecycle.bats proves the
# four helpers work under real launchd jobs; this proves every com.chorus.*
# LaunchAgent on the box calls one. Each label is either WIRED (its source file
# calls a lifecycle helper) or a NAMED GAP with the reason. A label in neither
# list is red: a new service that logs nothing cannot slip in unnoticed.
#
# A WRAP row is a service whose program we do not edit (inline `bash -c`, a
# binary with no source here, ssh, another repo): its installed plist must run
# it through platform/scripts/service-run. Until service-run is on the box
# (SERVICE_RUN, canonical) such a row is PENDING, never a pass; once it is,
# an unwrapped plist is red.
#
# Reads the installed LaunchAgents (read only). No LaunchAgents dir → UNMEASURED,
# never a pass. SERVICE_WIRING_AGENTS, SERVICE_WIRING_TABLE and SERVICE_RUN are
# the test seams the negative proofs use.
set -u
CHORUS_ROOT="${CHORUS_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
AGENTS="${SERVICE_WIRING_AGENTS:-$HOME/Library/LaunchAgents}"
MARK='service_lifecycle|serviceLifecycle|service-lifecycle\.sh|run_as_job'
SERVICE_RUN="${SERVICE_RUN:-$HOME/CascadeProjects/chorus/platform/scripts/service-run}"

# label <TAB> source file that must call a helper (repo-relative) | WRAP: why | GAP: reason
TABLE="${SERVICE_WIRING_TABLE:-$(cat <<'T'
com.chorus.hooks	platform/services/chorus-hooks/src/main.rs
com.chorus.athena-make	platform/services/athena-make/src/main.rs
com.chorus.athena-make.staging	platform/services/athena-make/src/main.rs
com.chorus.athena-validate	platform/services/athena-validate/src/main.rs
com.chorus.crawl-nightly	platform/services/chorus-crawl/src/main.rs
com.chorus.messages-project	platform/services/chorus-principal/src/main.rs
com.chorus.roles-up	platform/services/chorus-principal/src/main.rs
com.chorus.nightly-suites	platform/services/werk-test/src/main.rs
com.chorus.pair-heartbeat	platform/services/pair-heartbeat/src/main.rs
com.chorus.heartbeat	platform/services/chorus-hooks/src/shim.rs
com.chorus.api	platform/api/src/server.ts
com.chorus.eventloop-probe	platform/api/src/eventloop-probe.ts
com.chorus.mcp	platform/mcp-server/src/main.ts
com.chorus.pulse	platform/pulse/src/service.ts
com.chorus.clearing	directing/clearing/src/server.ts
com.chorus.bridge-subscriber-kade	platform/scripts/bridge-subscriber.js
com.chorus.bridge-subscriber-silas	platform/scripts/bridge-subscriber.js
com.chorus.bridge-subscriber-wren	platform/scripts/bridge-subscriber.js
com.chorus.share-guard	platform/scripts/chorus-share-guard.py
com.chorus.share-guard-path	platform/scripts/chorus-share-guard.py
com.chorus.alert-delivery-test	platform/scripts/alert-delivery-test.sh
com.chorus.alert-runner	proving/scripts/alert-runner.sh
com.chorus.bedroom-health	platform/scripts/health-check-bedroom.sh
com.chorus.cards-orphan-reaper	platform/scripts/cards-orphan-reaper.sh
com.chorus.chorus-health	platform/scripts/chorus-health
com.chorus.clearing-probe	platform/scripts/clearing-probe.sh
com.chorus.crawler-index	platform/scripts/index-crawler-snapshots.sh
com.chorus.cruft-scan	platform/scripts/cruft-scan.sh
com.chorus.daily-review-ops	platform/scripts/daily-review-ops.sh
com.chorus.daily-review-summary	platform/scripts/daily-review-summary.sh
com.chorus.daily-signal-scan	platform/scripts/daily-signal-scan.sh
com.chorus.deep-health	platform/scripts/deep-health.sh
com.chorus.embed-worker	platform/scripts/chorus-embed-worker.sh
com.chorus.fuseki-compact	building/products/convergence/fuseki-maintenance.sh
com.chorus.lance-maintain	platform/scripts/chorus-lance-maintain.sh
com.chorus.log-harvest	platform/scripts/log-harvest.sh
com.chorus.mcp-config-herald	platform/scripts/mcp-config-herald.sh
com.chorus.ops	platform/scripts/chorus-ops.sh
com.chorus.reindex-worker	platform/scripts/chorus-reindex-worker.sh
com.chorus.restore-drill	platform/scripts/fuseki-restore-dump.sh
com.chorus.security-scan-weekly	platform/scripts/test-security-scan.sh
com.chorus.seed-probe	platform/scripts/seed-probe.sh
com.chorus.service-harvest	platform/scripts/service-harvest-cycle.sh
com.chorus.standards-surface	platform/scripts/standards-surface-cron.sh
com.chorus.tm-thin	platform/scripts/tm-thin.sh
com.chorus.tmp-reaper	platform/scripts/tmp-reaper.sh
com.chorus.jeff-input-monitor	WRAP: binary with no source in the repo
com.chorus.session-watcher	WRAP: script lives only in ~/.chorus/scripts
com.chorus.heartbeat-probe	WRAP: script lives only in ~/.chorus/scripts
com.chorus.buzz-tunnel	WRAP: ssh, not our code
com.chorus.alert-notifier	WRAP: shared-observability repo
com.chorus.harvest-exporter	WRAP: shared-observability repo
com.chorus.launchagent-metrics	WRAP: shared-observability repo
com.chorus.fuseki-perf	WRAP: jeff-bridwell-personal-site repo
com.chorus.posture-capture	WRAP: jeff-bridwell-personal-site repo
com.chorus.building-pipeline	WRAP: inline bash -c in the plist
com.chorus.context-cache-daily	WRAP: inline bash -c in the plist
com.chorus.context-cache-hourly	WRAP: inline bash -c in the plist
com.chorus.context-cache-weekly	WRAP: inline bash -c in the plist
com.chorus.index-artifacts	WRAP: inline bash -c in the plist
com.chorus.perf-baseline	WRAP: inline bash -c in the plist
T
)}"

if [ ! -d "$AGENTS" ]; then
  echo "UNMEASURED: no LaunchAgents dir at $AGENTS"
  exit 0
fi

PASS=0; FAIL=0; GAPS=0; PENDING=0
for plist in "$AGENTS"/com.chorus.*.plist; do
  [ -e "$plist" ] || continue
  label="$(basename "$plist" .plist)"
  # a card's demo env (com.chorus.<svc>.werk.<role>) runs the same code as <svc>
  base="${label%%.werk.*}"
  row="$(printf '%s\n' "$TABLE" | awk -F'\t' -v l="$base" '$1==l {print $2; exit}')"
  prog="$(/usr/libexec/PlistBuddy -c 'Print :ProgramArguments:0' "$plist" 2>/dev/null)"
  # a card's demo variant (<svc>.werk.<role>) runs its werk's own build on purpose
  if [[ "$prog" == */target/release/* && "$label" != *.werk.* ]]; then
    # #4446 reopen: heartbeat ran the build artifact; a rebuild on 10-06 changed
    # its signature and launchd refused to start it (exit 78) — no process, no event
    echo "FAIL: $label — runs a build artifact ($prog); run the installed copy in ~/.chorus/bin"
    FAIL=$((FAIL+1))
  elif [ -z "$row" ]; then
    echo "FAIL: $label — not in the wiring table: wire it to a lifecycle helper, or name it as a gap"
    FAIL=$((FAIL+1))
  elif [[ "$row" == WRAP:* ]]; then
    args="$(/usr/libexec/PlistBuddy -c 'Print :ProgramArguments' "$plist" 2>/dev/null)"
    if printf '%s\n' "$args" | grep -q "service-run"; then
      PASS=$((PASS+1))
    elif [ ! -x "$SERVICE_RUN" ]; then
      echo "PENDING: $label — ${row#WRAP: }; wrap it once $SERVICE_RUN is installed"
      PENDING=$((PENDING+1))
    else
      echo "FAIL: $label — ${row#WRAP: }: its plist does not run through service-run"
      FAIL=$((FAIL+1))
    fi
  elif [[ "$row" == GAP:* ]]; then
    echo "GAP: $label — ${row#GAP: }"
    GAPS=$((GAPS+1))
  elif [ ! -f "$CHORUS_ROOT/$row" ]; then
    echo "FAIL: $label — wired to $row, which does not exist (renamed or deleted)"
    FAIL=$((FAIL+1))
  elif ! grep -Eq "$MARK" "$CHORUS_ROOT/$row"; then
    echo "FAIL: $label — $row calls no lifecycle helper"
    FAIL=$((FAIL+1))
  elif [[ "$row" =~ \.(ts|js|py)$ ]] && ! grep -Eq '\.started\(' "$CHORUS_ROOT/$row"; then
    # an import alone is not wiring: pulse imported the helper and logged no start
    echo "FAIL: $label — $row sets up a lifecycle but never calls started()"
    FAIL=$((FAIL+1))
  else
    PASS=$((PASS+1))
  fi
done

echo "=== Results: $PASS wired, $PENDING pending wrap, $GAPS named gaps, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
