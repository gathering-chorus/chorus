#!/usr/bin/env bats
# @test-type: unit — reads the canonical Fuseki log config as text; fuseki-config-deploy.sh against temp paths only. Nothing real is installed or restarted.
# @domain: persistence — Fuseki is the store; its log volume made Loki refuse reads (#4946)
#
# #4946 — Fuseki logged three lines per query at INFO (query text, "200 OK (n ms)",
# NCSA access line): 1.37M lines in fuseki.log on 2026-11-10, and Loki refused
# 18-21 of ~500 queries per 15m. The canonical log config keeps the Fuseki action
# logger and the Request logger at WARN; fuseki-config-deploy.sh installs it.

ROOT="$BATS_TEST_DIRNAME/../.."
CANON="$ROOT/platform/scripts/launchagents-canonical/fuseki-log4j2.properties"

setup() { T="$(mktemp -d)"; }
teardown() { rm -rf "$T"; }

# level <file> <logger-key>: the level a log4j2 properties file gives that logger
level() { sed -nE "s/^logger\.$2\.level[[:space:]]*=[[:space:]]*([A-Z]+).*/\1/p" "$1"; }

# quiet <file>: per-query loggers at WARN or above, startup loggers still INFO, reload on
quiet() {
  local f="$1" l
  for k in fuseki-fuseki fuseki-request; do
    l=$(level "$f" "$k")
    case "$l" in WARN|ERROR|FATAL|OFF) ;; *) echo "$k is '${l:-unset}'"; return 1;; esac
  done
  [ "$(level "$f" fuseki-server)" = INFO ] || { echo "fuseki-server not INFO"; return 1; }
  [ "$(level "$f" fuseki-config)" = INFO ] || { echo "fuseki-config not INFO"; return 1; }
  grep -qE '^monitorInterval[[:space:]]*=[[:space:]]*[1-9]' "$f" || { echo "no monitorInterval"; return 1; }
}

deploy() { run env FUSEKI_SERVER_RUNTIME="$T/config.ttl" FUSEKI_LOG_RUNTIME="$T/log4j2.properties" bash "$ROOT/platform/scripts/fuseki-config-deploy.sh" "$@"; }

@test "the canonical log config keeps the per-query loggers at WARN" {
  run quiet "$CANON"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "negative proof: the action logger back at INFO is caught" {
  sed -E 's/^(logger\.fuseki-fuseki\.level[[:space:]]*=[[:space:]]*)WARN/\1INFO/' "$CANON" > "$T/bad"
  ! cmp -s "$CANON" "$T/bad" || return 1
  run quiet "$T/bad"
  [ "$status" -eq 1 ] && [[ "$output" == *"fuseki-fuseki is 'INFO'"* ]]
}

@test "negative proof: the Request logger back at INFO is caught" {
  sed -E 's/^(logger\.fuseki-request\.level[[:space:]]*=[[:space:]]*)WARN/\1INFO/' "$CANON" > "$T/bad"
  ! cmp -s "$CANON" "$T/bad" || return 1
  run quiet "$T/bad"
  [ "$status" -eq 1 ] && [[ "$output" == *"fuseki-request is 'INFO'"* ]]
}

@test "negative proof: a missing logger line is caught, never read as quiet" {
  grep -v '^logger\.fuseki-request\.level' "$CANON" > "$T/bad"
  run quiet "$T/bad"
  [ "$status" -eq 1 ] && [[ "$output" == *"fuseki-request is 'unset'"* ]]
}

@test "config-deploy installs the log config, keeping the old one beside it" {
  echo "old-log" > "$T/log4j2.properties"
  deploy
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  cmp -s "$T/log4j2.properties" "$CANON" || return 1
  [ "$(cat "$T"/log4j2.properties.pre-4946.*)" = old-log ] || return 1
  deploy --check
  [ "$status" -eq 0 ]
}

@test "negative proof: --check is red when only the runtime log config drifts" {
  deploy
  [ "$status" -eq 0 ] || return 1
  echo "logger.fuseki-fuseki.level = INFO" >> "$T/log4j2.properties"
  deploy --check
  [ "$status" -eq 1 ] && [[ "$output" == *"log4j2.properties differs from canonical"* ]]
}
