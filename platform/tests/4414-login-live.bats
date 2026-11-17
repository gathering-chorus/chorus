#!/usr/bin/env bats
# @test-type: e2e — reads the live machine's session registry after the land; fixtures prove the check can fail.
# @domain: identity
# @card: #4414
# @owner: wren
# @needs: machine
# #4414 (AC3): after the land, every live role session sits in a tmux pane — none outside, where nudges cannot reach it
#
# Jeff 2026-09-30 15:48, after three tries to get Silas back: "ok so if i pat my head and
# make circles on my stomach and twist around in circles then it works". One login puts
# the role in its pane. After the land, every live registry entry runs under tmux
# (judged by the process tree). Asserts are simple commands, never [[ ]] (bash 3.2).

REG="${CHORUS_SESSIONS_DIR:-${CHORUS_STATE_DIR:-$HOME/.chorus}/sessions}"

# reads registry entries `<role>-<pid>.json` from the dir in $1 and, for each, the process
# chain from $2 (lines "pid ppid comm", i.e. `ps -Ao pid=,ppid=,comm=`); exit 0 when there is
# at least one entry and every entry's process has a tmux ancestor. The registry's pane field
# is often empty for a Claude role (host "unknown"), so the process tree is the evidence.
ALL_IN_TMUX='
import json,glob,os,sys
procs={}
for line in open(sys.argv[2]):
    p=line.split(None,2)
    if len(p)==3: procs[p[0]]=(p[1],os.path.basename(p[2].strip()))
def in_tmux(pid):
    seen=0
    while pid in procs and seen<20:
        ppid,comm=procs[pid]
        if comm.startswith("tmux"): return True
        pid=ppid; seen+=1
    return False
rows=[]
for f in glob.glob(os.path.join(sys.argv[1],"*-[0-9]*.json")):
    try: rows.append(json.load(open(f)))
    except Exception: pass
bad=[r.get("role") for r in rows if not in_tmux(str(r.get("pid")))]
sys.exit(0 if rows and not bad else 1)'

@test "AC3: every live role session on this machine is in a tmux pane" {
  test -d "$REG"
  ps -Ao pid=,ppid=,comm= > "$BATS_TEST_TMPDIR/ps"
  python3 -c "$ALL_IN_TMUX" "$REG" "$BATS_TEST_TMPDIR/ps"
}

@test "AC3 NEGATIVE PROOF: a session with no tmux ancestor, or no sessions at all, fails the check" {
  d="$BATS_TEST_TMPDIR/reg"; mkdir -p "$d"; ps="$BATS_TEST_TMPDIR/ps"
  printf '100 1 /usr/bin/login\n200 100 bash\n300 200 claude\n400 1 /opt/homebrew/bin/tmux\n500 400 bash\n600 500 claude\n' > "$ps"
  run python3 -c "$ALL_IN_TMUX" "$d" "$ps"
  [ "$status" -ne 0 ] || return 1
  printf '{"role":"silas","pid":300,"tty":"/dev/ttys007","host":"unknown"}' > "$d/silas-300.json"
  run python3 -c "$ALL_IN_TMUX" "$d" "$ps"
  [ "$status" -ne 0 ] || return 1
  rm "$d/silas-300.json"
  printf '{"role":"silas","pid":600,"tty":"/dev/ttys015","host":"unknown"}' > "$d/silas-600.json"
  printf '{"role":"silas","pid":600}' > "$d/silas.turn.json"
  run python3 -c "$ALL_IN_TMUX" "$d" "$ps"
  [ "$status" -eq 0 ] || return 1
}
