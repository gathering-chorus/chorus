#!/usr/bin/env bats
# @test-type: unit — a writer holding its log open with >> in a temp dir; the rotate script over it. No service, no Loki.
# @domain: logs — rotation must never blind a service's log (#4718)
#
# #4718 — launchd holds StandardOutPath open. Rotation by mv left Fuseki and
# Loki writing into renamed, then unlinked, files for three days (10-17 to
# 10-20). After a rotation the writer's next line must land in the live file.

ROOT="$BATS_TEST_DIRNAME/../.."

setup() {
  T="$(mktemp -d)"
  head -c 2000 /dev/zero | tr '\0' 'x' > "$T/fuseki.log"
  # a writer holding the log open the way launchd does (one fd, append)
  exec 7>>"$T/fuseki.log"
}
teardown() { exec 7>&-; rm -rf "$T"; }

@test "after a rotation the held writer's next line is in the live log, and the old content is kept gzipped" {
  run env LOG_ROTATE_DIR="$T" LOG_ROTATE_BYTES=1000 bash "$ROOT/platform/scripts/gathering-log-rotate.sh"
  [ "$status" -eq 0 ] || return 1
  [[ "$output" == *"fuseki.log rotated"* ]] || return 1
  echo "line after rotation" >&7
  grep -q "line after rotation" "$T/fuseki.log" || { echo "the writer's line is not in the live log"; false; }
  [ "$(gzip -dc "$T"/fuseki.log.*.gz | wc -c | tr -d ' ')" -eq 2000 ]
}

@test "under the size line nothing is rotated" {
  run env LOG_ROTATE_DIR="$T" LOG_ROTATE_BYTES=999999 bash "$ROOT/platform/scripts/gathering-log-rotate.sh"
  [ -z "$output" ] && ! ls "$T"/fuseki.log.* 2>/dev/null
}

@test "negative proof: rotation by mv loses the held writer's next line (the 10-17 shape)" {
  mv "$T/fuseki.log" "$T/fuseki.log.old" && : > "$T/fuseki.log"
  echo "line after rotation" >&7
  ! grep -q "line after rotation" "$T/fuseki.log" || return 1
}

@test "the repo plist runs the script, not an inline mv" {
  P="$ROOT/proving/config/launchagents/com.gathering.log-rotate.plist"
  grep -q "platform/scripts/gathering-log-rotate.sh</string>" "$P"
  ! grep -vE '^\s*(<!--|this plist)' "$P" | grep -qE '\bmv "\$FILE"' || return 1
}
