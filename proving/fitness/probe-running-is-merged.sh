#!/usr/bin/env bash
# ROW 4 — the running process is the merged code.
#
# WHY THIS ROW EXISTS
#   Six times in three days a fix landed on main and the live process kept
#   serving the old behaviour. Every time the gap was invisible: the merge
#   succeeded, tests passed against a fresh instance, and the person using the
#   system got the old bug. Twice this week Jeff was told something was fixed
#   while the process in front of him was hours old.
#
#   Every other row measures the running system. This row is what makes their
#   answers mean anything: a green row against a two-day-old process tells you
#   about a binary nobody is using.
#
# WHAT IT ASSERTS
#   For each service: the process start time is not older than the newest commit
#   on origin/main touching that service's sources. Coarse on purpose — it can
#   prove a process IS stale, never that it is current (a restart onto a failed
#   build reads as fresh). Stated here rather than left as an unwritten limit.
#
# THE GHOST-PROCESS FIX (this probe's own defect, 2026-08-08)
#   The first version resolved pids with `pgrep -f <name> | head -1`. It picked a
#   July-30 orphan and I reported a live service as stale to Jeff, who checked and
#   found it wasn't. `head -1` is a guess wearing the costume of a measurement.
#   Now: the pid comes from launchd, which owns the service and knows exactly one
#   answer. No launchd job, or a job with no pid → UNMEASURABLE, never a verdict.
#
# VERDICTS
#   green         no service is provably stale
#   red           at least one service runs code older than its newest commit
#   unmeasurable  a service could not be resolved — never counted as a pass
set -u

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="${CHORUS_ROOT:-$(cd "$HERE/../.." && pwd)}"
PROVE_RED=0
[ "${1:-}" = "--prove-red" ] && PROVE_RED=1

# service label | source paths whose newest commit must predate the start
SERVICES="
com.chorus.share-guard|platform/scripts/chorus-share-guard.py
com.chorus.clearing|directing/clearing/src
com.chorus.api|platform/api/src
"

# The one authority on "which process is this service". launchd owns the job;
# asking it removes the guess that made this probe lie.
launchd_pid() {
  launchctl print "gui/$(id -u)/$1" 2>/dev/null \
    | awk '/^[[:space:]]*pid = /{print $3; exit}'
}

epoch_of_ps_time() {
  python3 - "$1" <<'PY'
import sys, datetime
s = sys.argv[1].strip()
try:
    print(int(datetime.datetime.strptime(s, "%a %b %d %H:%M:%S %Y").timestamp()))
except ValueError:
    print("")
PY
}

stale=0; unmeasurable=0; checked=0
stale_names=""

# The staleness decision, in ONE place, so the negative proof exercises the same
# code the live run does. A proof that re-implements the comparison proves the
# re-implementation.
is_stale() {  # is_stale <started_epoch> <commit_epoch>
  [ "$2" -gt "$1" ]
}

printf '%-24s %-16s %-16s %s\n' SERVICE STARTED "NEWEST COMMIT" VERDICT
while IFS='|' read -r label paths; do
  [ -z "$label" ] && continue

  pid="$(launchd_pid "$label")"
  if [ -z "$pid" ]; then
    printf '%-24s %-16s %-16s %s\n' "$label" "-" "-" "UNMEASURABLE (no launchd pid)"
    unmeasurable=$((unmeasurable + 1)); continue
  fi

  started="$(epoch_of_ps_time "$(ps -p "$pid" -o lstart= 2>/dev/null)")"
  if [ -z "$started" ]; then
    printf '%-24s %-16s %-16s %s\n' "$label" "?" "-" "UNMEASURABLE (pid $pid has no start time)"
    unmeasurable=$((unmeasurable + 1)); continue
  fi

  commit_ts="$(git -C "$REPO" log origin/main -1 --format=%ct -- $paths 2>/dev/null)"
  if [ -z "$commit_ts" ]; then
    # The declared path is wrong or gone. A guard whose target vanished must
    # fail loudly, never pass vacuously.
    printf '%-24s %-16s %-16s %s\n' "$label" "$(date -r "$started" '+%b %d %H:%M')" "-" \
      "UNMEASURABLE (no commits for '$paths' — fix the path, do not trust this row)"
    unmeasurable=$((unmeasurable + 1)); continue
  fi

  checked=$((checked + 1))
  if is_stale "$started" "$commit_ts"; then
    subject="$(git -C "$REPO" log origin/main -1 --format=%s -- $paths 2>/dev/null | cut -c1-34)"
    printf '%-24s %-16s %-16s %s\n' "$label" "$(date -r "$started" '+%b %d %H:%M')" \
      "$(date -r "$commit_ts" '+%b %d %H:%M')" "STALE — predates: $subject"
    stale=$((stale + 1)); stale_names="$stale_names $label"
  else
    printf '%-24s %-16s %-16s %s\n' "$label" "$(date -r "$started" '+%b %d %H:%M')" \
      "$(date -r "$commit_ts" '+%b %d %H:%M')" "ok (not provably stale)"
  fi
done <<EOF
$SERVICES
EOF
echo

# NEGATIVE PROOF — a fabricated service whose sources have always existed and
# whose "process" started at the epoch. If the comparison cannot call that stale,
# it cannot call anything stale, and this row's green means nothing.
if [ "$PROVE_RED" -eq 1 ]; then
  commit_ts="$(git -C "$REPO" log origin/main -1 --format=%ct 2>/dev/null)"
  if [ -z "$commit_ts" ]; then
    echo "row4 NEGATIVE PROOF UNMEASURABLE — no origin/main history to compare against" >&2
    exit 2
  fi
  # Drive the REAL decision function with a violation: a process that started
  # long before the newest commit. It must come back stale. And drive it with the
  # opposite — a process started after — which must NOT, or the function is a
  # rubber stamp that would call every healthy service stale too.
  ok=1
  if ! is_stale 0 "$commit_ts"; then
    echo "row4 NEGATIVE PROOF FAILED — a process started at the epoch was NOT called stale; this row cannot see the failure it exists for." >&2
    ok=0
  fi
  if is_stale "$((commit_ts + 3600))" "$commit_ts"; then
    echo "row4 NEGATIVE PROOF FAILED — a process started AFTER the newest commit was called stale; this row cannot tell the two states apart." >&2
    ok=0
  fi
  if [ "$ok" -eq 1 ]; then
    echo "row4 NEGATIVE PROOF OK — the live decision function calls an epoch-start process stale against main ($(date -r "$commit_ts" '+%b %d %H:%M')) and a later-start process fresh."
    exit 0
  fi
  exit 1
fi

if [ "$stale" -gt 0 ]; then
  echo "row4 RED —$stale_names running code older than main. Every other row's verdict about those services is about a binary nobody is using."
  exit 1
fi
if [ "$checked" -eq 0 ]; then
  echo "row4 UNMEASURABLE — no service could be resolved; this row asserts nothing today." >&2
  exit 2
fi
if [ "$unmeasurable" -gt 0 ]; then
  echo "row4 green for $checked service(s), $unmeasurable UNMEASURABLE — a service we cannot ask is not a service that is fine."
  exit 0
fi
echo "row4 green — $checked service(s) not provably stale (absence of the known failure, not proof of currency)."
