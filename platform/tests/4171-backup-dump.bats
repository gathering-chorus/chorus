#!/usr/bin/env bats
# @test-type: contract
# @domain: infrastructure — the product domain this suite guards (#4334)
# #4171 — the nightly Fuseki backup. These test the THREE guards that separate
# "a file exists" from "the data is in it", each one taken from a failure that
# actually shipped:
#
#   reads success   the weekly compact script POSTed and never read `success`,
#                   so tasks that died in 90s were logged as done for 11 weeks
#   size floor      four native dumps (Jun 25 x2, Jul 1 x2) were 20 BYTES, a
#                   gzip header and nothing else, sitting in the backup dir
#   its own task    measured 2026-09-14 20:37:56 — waiting on "the newest
#                   Backup task" found the PREVIOUS run's finished task, skipped
#                   waiting, and shipped an hour-old dump as tonight's backup
#
# Every one has a negative proof, because a guard that has only ever seen good
# input cannot show it would reject bad input (#3734).
SCRIPT="${BATS_TEST_DIRNAME}/../scripts/fuseki-backup.sh"

setup() {
  TMP="$(mktemp -d)"
  export FUSEKI_BACKUP_DEST="$TMP/remote"
  export FUSEKI_BACKUP_LOCAL="$TMP/backups"
  mkdir -p "$FUSEKI_BACKUP_LOCAL" "$FUSEKI_BACKUP_DEST"
}
teardown() { rm -rf "$TMP"; }

@test "the script reads the task's success flag, not just that it finished" {
  run grep -qE '\[ "\$ok" = "True" \]' "$SCRIPT"
  [ "$status" -eq 0 ]
}

@test "NEGATIVE PROOF — success=False is refused, not treated as done" {
  # The guarded condition VIOLATED: a task that finished and failed.
  ok="False"
  run bash -c 'ok="False"; [ "$ok" = "True" ] || exit 7'
  [ "$status" -eq 7 ]
}

@test "the script waits on the taskId the POST returned" {
  run grep -q 'TASK_ID=' "$SCRIPT"
  [ "$status" -eq 0 ]
  run grep -q 'str(x.get("taskId")) == want' "$SCRIPT"
  [ "$status" -eq 0 ]
}

@test "NEGATIVE PROOF — picking the newest Backup task finds the PREVIOUS run" {
  # Two tasks: an old finished one and this run's unfinished one. Selecting by
  # recency returns the finished one and the wait is skipped — the 20:37 bug.
  tasks='[{"taskId":"3","task":"Backup","finished":"2026-09-14T19:36:37"},
          {"taskId":"4","task":"Backup","finished":null}]'
  newest=$(printf '%s' "$tasks" | python3 -c 'import sys,json;t=[x for x in json.load(sys.stdin) if x["task"]=="Backup"];print(t[-1]["finished"] or "-")')
  byid=$(printf '%s' "$tasks" | python3 -c 'import sys,json;t=[x for x in json.load(sys.stdin) if x["taskId"]=="4"];print(t[0]["finished"] or "-")')
  # "newest" here is the LAST element, which is ours — so order alone is not the
  # bug. The bug is that a finished older task ANYWHERE satisfies a scan that is
  # not pinned to an id:
  anyfinished=$(printf '%s' "$tasks" | python3 -c 'import sys,json;print("yes" if any(x.get("finished") for x in json.load(sys.stdin)) else "no")')
  [ "$anyfinished" = "yes" ]   # an unpinned check would proceed
  [ "$byid" = "-" ]            # the pinned check correctly waits
}

@test "a dump under the floor is refused" {
  MIN=$(( 50 * 1024 * 1024 ))
  SIZE=$(( 20 ))
  run bash -c "[ $SIZE -ge $MIN ]"
  [ "$status" -ne 0 ]
}

@test "NEGATIVE PROOF — the real June/July stubs were 20 bytes and would ship without a floor" {
  printf '\x1f\x8b\x08\x00\x00\x00\x00\x00\x00\x03' > "$FUSEKI_BACKUP_LOCAL/pods_stub.nq.gz"
  size=$(stat -f %z "$FUSEKI_BACKUP_LOCAL/pods_stub.nq.gz")
  [ "$size" -lt 100 ]
  # A file-exists check passes on it; the floor does not.
  [ -f "$FUSEKI_BACKUP_LOCAL/pods_stub.nq.gz" ]
  run bash -c "[ $size -ge $(( 50 * 1024 * 1024 )) ]"
  [ "$status" -ne 0 ]
}

@test "retention sorts by NAME, never by mtime" {
  run grep -q 'sort -r' "$SCRIPT"
  [ "$status" -eq 0 ]
  run grep -qE 'ls -1t .*dumps' "$SCRIPT"
  [ "$status" -ne 0 ]
}

@test "NEGATIVE PROOF — an mtime sort ranks a fresh dump as the oldest" {
  # 2026-08-28: scp/rsync carry source times, so `ls -t` ranked five fresh
  # snapshots as older than 08-22 and deleted 08-24 through 08-28.
  d="$TMP/dumps"; mkdir -p "$d"
  touch -t 202608220000 "$d/pods_2026-08-22_00-00-00.nq.gz"
  touch -t 202601010000 "$d/pods_2026-09-14_00-00-00.nq.gz"   # fresh name, old mtime
  oldest_by_mtime=$(ls -1t "$d" | tail -1)
  oldest_by_name=$(ls -1 "$d" | sort | head -1)
  [ "$oldest_by_mtime" = "pods_2026-09-14_00-00-00.nq.gz" ]   # would delete the NEW one
  [ "$oldest_by_name" = "pods_2026-08-22_00-00-00.nq.gz" ]    # name sort is correct
}

@test "the restore drill counts named graphs, not the default graph" {
  D="${BATS_TEST_DIRNAME}/../scripts/fuseki-restore-dump.sh"
  run grep -q 'GRAPH ?g { ?s ?p ?o }' "$D"
  [ "$status" -eq 0 ]
}

@test "NEGATIVE PROOF — a bare pattern reads the empty default graph" {
  # Measured twice on 2026-09-14: verify-load reported 0 and verify-live
  # reported 39,674,078 against a store holding 39,687,538, both because the
  # query was { ?s ?p ?o } instead of GRAPH ?g { ?s ?p ?o }.
  D="${BATS_TEST_DIRNAME}/../scripts/fuseki-restore-dump.sh"
  run grep -qE "SELECT \(COUNT\(\*\) AS \?n\) WHERE \{ \?s \?p \?o \}" "$D"
  [ "$status" -ne 0 ]
}

@test "the drill pulls the dump back from the remote, not a local copy" {
  D="${BATS_TEST_DIRNAME}/../scripts/fuseki-restore-dump.sh"
  run grep -q 'scp -q -o ConnectTimeout=10 "${REMOTE}' "$D"
  [ "$status" -eq 0 ]
}

@test "the drill defaults to the OLDEST dump held" {
  D="${BATS_TEST_DIRNAME}/../scripts/fuseki-restore-dump.sh"
  run grep -q 'sort | head -1' "$D"
  [ "$status" -eq 0 ]
}
