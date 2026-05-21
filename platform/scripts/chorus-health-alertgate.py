#!/usr/bin/env python3
# chorus-health-alertgate — #3022. The single source of the alert-gating
# decision, so it is testable in isolation (--selftest) and reused verbatim by
# chorus-health (no second copy to drift).
#
# Decision (severity-aware):
#   - FAIL = definitive down (ping fails, connection refused, exit!=0) → alert on
#     the TRANSITION into failure: immediate, and once (silent while it persists).
#   - WARN = inconclusive / can't-complete (timeout, can't-verify — what a deploy,
#     a werk test run, or a load spike produce) → alert ONLY after N runs straight.
#
# Usage:
#   chorus-health-alertgate.py <N> <fails_csv> <warns_csv>   # Loki JSON on stdin → prints names to alert
#   chorus-health-alertgate.py --selftest                    # runs the scenarios, no I/O, prints PASS/FAIL
import json, sys


def compute(history, n, cur_fail, cur_warn):
    """history: list of {'failures':set,'bad':set}, NEWEST FIRST (prior runs only)."""
    prior_fail = history[0]['failures'] if history else set()
    alert = set()
    for c in cur_fail:                      # definitive down → alert on transition, once
        if c not in prior_fail:
            alert.add(c)
    for c in cur_warn:                      # inconclusive → only once persistent N runs straight
        streak = 0
        for run in history:
            if c in run['bad']:
                streak += 1
            else:
                break                       # streak broken by a green run
        if streak == n - 1:                 # this run is the Nth consecutive → alert once
            alert.add(c)
    return sorted(alert)


def parse_loki(raw):
    d = json.loads(raw)
    rows = []
    for s in d['data']['result']:
        for v in s['values']:
            rows.append((int(v[0]), v[1]))
    rows.sort(reverse=True)                 # newest first
    hist = []
    for _ts, line in rows:
        try:
            ev = json.loads(line)
        except Exception:
            continue
        if ev.get('event') != 'chorus.health':
            continue
        f = set(x for x in (ev.get('failures') or '').split(',') if x)
        w = set(x for x in (ev.get('warns') or '').split(',') if x)
        hist.append({'failures': f, 'bad': f | w})
    return hist


def _mk(*runs):
    """Build history from newest-first (fails_csv, warns_csv) tuples."""
    h = []
    for f, w in runs:
        fs = set(x for x in f.split(',') if x)
        ws = set(x for x in w.split(',') if x)
        h.append({'failures': fs, 'bad': fs | ws})
    return h


def selftest():
    N = 3
    ok_all = True

    def check(label, history, cur_fail, cur_warn, expect):
        nonlocal ok_all
        got = compute(history, N, cur_fail, cur_warn)
        ok = (got == expect)
        ok_all = ok_all and ok
        mark = 'PASS' if ok else 'FAIL'
        print(f"  [{mark}] {label}")
        print(f"         alerts={got or '(none)'}   expected={expect or '(none)'}")

    print(f"chorus-health alert-gate self-test (N={N}) — fully isolated, no Loki, no nudges\n")
    check("A real DOWN alerts IMMEDIATELY (prior run was clean)",
          _mk(("", "")), ["svc"], [], ["svc"])
    check("A DOWN that's still down does NOT re-alert next cycle",
          _mk(("svc", "")), ["svc"], [], [])
    check("A transient inconclusive (1 run) stays SILENT",
          _mk(("", "svc"), ("", "")), [], ["svc"], [])
    check("A persistent inconclusive (3 runs straight) alerts ONCE",
          _mk(("", "svc"), ("", "svc"), ("", "")), [], ["svc"], ["svc"])
    check("That same inconclusive does NOT re-alert after it crossed",
          _mk(("", "svc"), ("", "svc"), ("", "svc")), [], ["svc"], [])
    check("A deploy/test blip (down 1 run, no history) — DOWN still alerts (real), but a WARN blip would not",
          _mk(("", "")), [], ["svc"], [])
    print()
    print("RESULT:", "ALL PASS ✓" if ok_all else "SOME FAILED ✗")
    return 0 if ok_all else 1


if __name__ == "__main__":
    if len(sys.argv) >= 2 and sys.argv[1] == "--selftest":
        sys.exit(selftest())
    n = int(sys.argv[1])
    cur_fail = [x for x in sys.argv[2].split(',') if x]
    cur_warn = [x for x in sys.argv[3].split(',') if x]
    try:
        history = parse_loki(sys.stdin.read())
    except Exception:
        history = []
    print(' '.join(compute(history, n, cur_fail, cur_warn)))
