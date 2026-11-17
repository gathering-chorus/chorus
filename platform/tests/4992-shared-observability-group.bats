#!/usr/bin/env bats
# @test-type: contract — shared-observability chorus group access, tracked exporter link, and #3734 wrong-owner / dirty-file negative proofs
# @domain: metrics
# @card: #4992
# @owner: borg-queen
#
# #4992 AC2 — shared-observability grants group `chorus` write access via land-4992-rows
# (with wrong-owner negative proof) so chorus-borg-queen can commit and test exporter updates
# without privilege escalation.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
LAND="$ROOT/platform/scripts/land-4992-rows"

setup() {
  T="$BATS_TEST_TMPDIR"
  cat > "$T/owners-ok.json" <<'JSON'
{
  "https://jeffbridwell.com/chorus#repository-shared-observability": "principal-borg-queen",
  "https://jeffbridwell.com/chorus#alerts": "principal-borg-queen",
  "https://jeffbridwell.com/chorus#heralds": "principal-borg-queen",
  "https://jeffbridwell.com/chorus#logs": "principal-borg-queen",
  "https://jeffbridwell.com/chorus#metrics": "principal-borg-queen",
  "https://jeffbridwell.com/chorus#monitors": "principal-borg-queen"
}
JSON
  mkdir -p "$T/obs/scripts" "$T/obs/config/prometheus" "$T/obs/data/textfile_collector"
  printf 'scrape_interval: 15s\n' > "$T/obs/config/prometheus/prometheus-native.yml"
  git -C "$T/obs" init -q
  git -C "$T/obs" config user.name t
  git -C "$T/obs" config user.email t@t
  git -C "$T/obs" add config/prometheus/prometheus-native.yml
  git -C "$T/obs" commit -qm "initial"
  # Simulate pre-#4992 state: untracked old script, 0755 dirs, 0644 files, plus an unrelated dirty file
  printf '#!/bin/sh\necho old\n' > "$T/obs/scripts/process-memory-exporter.sh"
  printf '# local edit\n' >> "$T/obs/config/prometheus/prometheus-native.yml"
  chmod 0755 "$T/obs" "$T/obs/scripts" "$T/obs/config" "$T/obs/config/prometheus" "$T/obs/data" "$T/obs/data/textfile_collector"
  chmod 0644 "$T/obs/config/prometheus/prometheus-native.yml"
}

@test "land-4992-rows grants chorus group write + setgid on shared-observability, tracks canonical exporter symlink, and leaves unrelated edits untouched" {
  GRP="chorus"
  id -Gn | tr ' ' '\n' | grep -qx "chorus" || GRP="$(id -gn)"

  LAND_LIVE_OWNERS_JSON="$T/owners-ok.json" OBS_REPO="$T/obs" OBS_GROUP="$GRP" \
    CHORUS_HOME="$ROOT" GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t \
    GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t \
    run python3 "$LAND" --apply
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }

  # Group is chorus and group-writable + setgid on directories
  [ "$(stat -f '%Sg' "$T/obs")" = "$GRP" ] || { stat -f '%Sg %Sp' "$T/obs"; return 1; }
  [ "$(stat -f '%Sg' "$T/obs/scripts")" = "$GRP" ] || return 1
  [[ "$(stat -f '%Sp' "$T/obs")" == drwxrws* ]] || { stat -f '%Sp' "$T/obs"; return 1; }
  [[ "$(stat -f '%Sp' "$T/obs/config/prometheus/prometheus-native.yml")" == -rw-rw* ]] || { stat -f '%Sp' "$T/obs/config/prometheus/prometheus-native.yml"; return 1; }

  # Untracked old script was replaced by a tracked symlink to chorus proving/scripts/process-memory-exporter.sh
  [ -L "$T/obs/scripts/process-memory-exporter.sh" ] || { ls -la "$T/obs/scripts"; return 1; }
  git -C "$T/obs" ls-files --error-unmatch scripts/process-memory-exporter.sh >/dev/null || return 1

  # Unrelated dirty file in shared-observability was NOT committed or reverted
  git -C "$T/obs" diff --name-only | grep -qx "config/prometheus/prometheus-native.yml" || { git -C "$T/obs" status; return 1; }

  # Exporter ran at land and produced valid process_memory.prom
  [ -s "$T/obs/data/textfile_collector/process_memory.prom" ] || return 1
  grep -q "^host_workload_cpu_percent{" "$T/obs/data/textfile_collector/process_memory.prom" || return 1
}

@test "negative proof (#3734): wrong-owner account (LAND_AS_UID != repo owner) refuses exit 2 and touches nothing" {
  LAND_AS_UID=99999 LAND_LIVE_OWNERS_JSON="$T/owners-ok.json" OBS_REPO="$T/obs" \
    CHORUS_HOME="$ROOT" run python3 "$LAND" --apply
  [ "$status" -eq 2 ] || { echo "$output"; return 1; }
  [[ "$output" == *"this land runs as 99999; nothing changed"* ]] || { echo "$output"; return 1; }
  [ ! -L "$T/obs/scripts/process-memory-exporter.sh" ] || { echo "exporter was replaced despite wrong owner"; return 1; }
  [[ "$(stat -f '%Sp' "$T/obs")" == drwxr-xr-x* ]] || { stat -f '%Sp' "$T/obs"; return 1; }
}

@test "negative proof (#3734): tracked scripts/process-memory-exporter.sh with uncommitted edits refuses exit 2 before mutating" {
  git -C "$T/obs" add scripts/process-memory-exporter.sh
  git -C "$T/obs" commit -qm "track old exporter"
  printf '# uncommitted local tweak\n' >> "$T/obs/scripts/process-memory-exporter.sh"

  LAND_LIVE_OWNERS_JSON="$T/owners-ok.json" OBS_REPO="$T/obs" OBS_GROUP="$(id -gn)" \
    CHORUS_HOME="$ROOT" run python3 "$LAND" --apply
  [ "$status" -eq 2 ] || { echo "$output"; return 1; }
  [[ "$output" == *"has uncommitted edits; nothing changed"* ]] || { echo "$output"; return 1; }
  [ ! -L "$T/obs/scripts/process-memory-exporter.sh" ] || return 1
}

@test "negative proof (#3734): chgrp failure surfaces exit 1 instead of being swallowed" {
  CHGRP_BIN=/usr/bin/false LAND_LIVE_OWNERS_JSON="$T/owners-ok.json" OBS_REPO="$T/obs" \
    OBS_GROUP="$(id -gn)" CHORUS_HOME="$ROOT" run python3 "$LAND" --apply
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  [[ "$output" == *"chgrp"* ]] || { echo "$output"; return 1; }
}
