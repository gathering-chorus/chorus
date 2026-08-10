#!/usr/bin/env bash
# Record what the security rows said, so nobody has to ask a role.
#
# WHY THIS EXISTS
#   Jeff, 2026-08-09: "so any time i want to know i gotta ask u and hope you give
#   me the same readout and dont improvise or confabulate or hallucinate."
#   Yes — and that was the real defect. The rows were honest; the only way to
#   read them was to ask a role to run them and retype the answer. A measurement
#   that reaches its audience through a narrator is not a measurement.
#
#   This writes the verdicts to a file the API serves and a page renders: same
#   words every time, with the time it was taken. A stale readout that shows its
#   age is honest; one that hides its age is not.
#
# EVERY ROW APPEARS, ALWAYS. A row with no probe yet is reported "not wired" —
# never omitted. A row omitted from a list of seven reads as a row that passed,
# which is how "5 of 7" got told to Jeff as "green" this week.
#
# ROW 7 hangs today. Each row gets a ceiling; a row that will not answer in time
# is unmeasurable — never green, never red — and says so with its name attached.
set -u

SELF="$(cd "$(dirname "$0")" && pwd)"
# Where the rows live. Defaults to this script's own directory; overridable so a
# snapshot can be taken against a checkout other than the one holding this file.
HERE="${FITNESS_DIR:-$SELF}"
OUT="${FITNESS_SNAPSHOT:-$HOME/.chorus/security-fitness.json}"
CEILING="${FITNESS_ROW_CEILING:-90}"

# What each row claims, in the words a person reads. The script names live in
# run-fitness.sh; this is the sentence, and it is the contract with the reader.
CLAIMS="
1|The public root serves the site, not a login form
2|A sign-in lands somewhere real
3|You can still speak in the Clearing, not just read
4|The running code is the merged code
5|Everyone who could write can still write
6|No two apps share an allow-list source
7|A nobody walking through the real door gets nowhere
"

wired() {  # is row $1 wired into the runner?
  grep -qE "^[[:space:]]*\"$1\|" "$HERE/run-fitness.sh"
}

# Run a row under a ceiling. Exit 124 means it never answered.
run_row() {
  FITNESS_TIMEOUT="$CEILING" perl -e '
    my $pid = fork();
    if ($pid == 0) { open(STDERR, ">&", \*STDOUT); exec(@ARGV); exit 127; }
    local $SIG{ALRM} = sub { kill 9, $pid; waitpid($pid, 0); exit 124 };
    alarm($ENV{FITNESS_TIMEOUT});
    waitpid($pid, 0);
    my $rc = $? >> 8;
    alarm(0);
    exit $rc;
  ' bash "$HERE/run-fitness.sh" "$1" 2>&1
}

json_escape() { sed 's/\\/\\\\/g; s/"/\\"/g' | tr -d '\n' | cut -c1-200; }

taken="$(TZ=America/New_York date '+%Y-%m-%d %H:%M')"
tmp="$OUT.tmp"

{
  printf '{\n  "taken": "%s Boston",\n  "rows": [\n' "$taken"
  first=1
  while IFS='|' read -r n claim; do
    [ -z "$n" ] && continue

    if ! wired "$n"; then
      verdict="not wired"
      detail="no probe for this row yet"
    else
      out="$(run_row "$n")"
      rc=$?
      if [ "$rc" -eq 124 ]; then
        verdict="unmeasurable"
        detail="no verdict within ${CEILING}s"
      else
        # Read the runner's OWN verdict word. The exit code cannot tell red from
        # untrustworthy (both non-zero), and those are different facts: red means
        # reality is wrong, untrustworthy means the row's negative proof stopped
        # failing so its answer — including a green one — means nothing.
        verdict="$(printf '%s' "$out" | grep -oiE "^row$n[[:space:]]+(green|red|unmeasurable|untrustworthy)" \
          | awk '{print tolower($2)}' | tail -1)"
        detail="$(printf '%s' "$out" | grep -iE "^row$n" | tail -1)"
        if [ -z "$verdict" ]; then
          verdict="unmeasurable"
          detail="the row printed no verdict (exit $rc)"
        fi
      fi
    fi

    [ $first -eq 0 ] && printf ',\n'
    first=0
    printf '    {"row": %s, "claim": "%s", "verdict": "%s", "detail": "%s"}' \
      "$n" "$claim" "$verdict" "$(printf '%s' "$detail" | json_escape)"
  done <<EOF
$CLAIMS
EOF
  printf '\n  ]\n}\n'
} > "$tmp" && mv "$tmp" "$OUT"

echo "security fitness snapshot → $OUT ($taken Boston)"
command -v chorus-log >/dev/null 2>&1 &&
  chorus-log fitness.snapshot.written "${DEPLOY_ROLE:-wren}" "domain=chorus" "card=3765" >/dev/null 2>&1 || true
