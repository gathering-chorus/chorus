#!/usr/bin/env bash
# @test-type: integration — reads the live process table (ps), read-only
# @domain: spine
# @needs: machine — prod after the land and the hooks/runner restart
# @card: #4951 · owner: silas
# Covers platform/services/chorus-hooks/src/shared/reap.rs and platform/services/chorus-runner/src/tasks.rs.
#
# #4951 AC2 — in prod, neither chorus-hooks nor chorus-runner holds a <defunct> child
# older than 60 s. An instant read: a helper that has just exited may be a zombie for
# a moment before its reaper thread waits on it, so only an aged zombie is a leak.
# A daemon that is not running is RED, never a vacuous pass.
set -u
MAX_AGE="${ZOMBIE_MAX_AGE_S:-60}"
secs() { # ps etime [[dd-]hh:]mm:ss → seconds
  awk -F'[-:]' '{ if (NF==4) print $1*86400+$2*3600+$3*60+$4; else if (NF==3) print $1*3600+$2*60+$3; else print $1*60+$2 }' <<<"$1"
}
bad=0
for d in chorus-hooks chorus-runner; do
  pid=$(pgrep -f "/\.chorus/bin/$d\$" | head -1)
  [ -n "$pid" ] || { echo "RED: $d is not running"; bad=1; continue; }
  n=0
  while read -r ppid stat etime; do
    [ "$ppid" = "$pid" ] && [[ "$stat" == Z* ]] || continue
    [ "$(secs "$etime")" -gt "$MAX_AGE" ] && n=$((n+1))
  done < <(ps -A -o ppid=,stat=,etime=)
  if [ "$n" -gt 0 ]; then echo "RED: $d (pid $pid) holds $n zombie(s) older than ${MAX_AGE}s"; bad=1
  else echo "ok: $d (pid $pid) holds no zombie older than ${MAX_AGE}s"; fi
done
[ "$bad" -eq 0 ] && echo "GREEN: no aged zombies under chorus-hooks or chorus-runner"
exit "$bad"
