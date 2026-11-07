"""Which chorus-health checks nudge this run (#3022, #4633).

stdin: Loki query_range JSON of prior chorus.health events.
argv:  N fails_csv warns_csv advisory_csv
A FAIL fires on its transition into failure; a WARN fires once it has persisted
N runs straight; an advisory WARN never fires (#4633). Prints the names, space-separated.
Bad or missing input prints nothing (silent is the safe side).
"""
import json
import sys


def firing(rows, n, cur_fail, cur_warn, advisory):
    runs_fail, runs_bad = [], []
    for _, line in sorted(rows, reverse=True):
        try:
            ev = json.loads(line)
        except ValueError:
            continue
        if ev.get('event') != 'chorus.health':
            continue
        f = set(x for x in (ev.get('failures') or '').split(',') if x)
        w = set(x for x in (ev.get('warns') or '').split(',') if x)
        runs_fail.append(f)
        runs_bad.append(f | w)
    prior_fail = runs_fail[0] if runs_fail else set()
    alert = set(c for c in cur_fail if c not in prior_fail)
    for c in cur_warn:
        if c in advisory:
            continue
        streak = 0
        for bad in runs_bad:
            if c not in bad:
                break
            streak += 1
        if streak == n - 1:
            alert.add(c)
    return sorted(alert)


def main():
    csv = lambda i: [x for x in (sys.argv[i] if len(sys.argv) > i else '').split(',') if x]
    try:
        d = json.load(sys.stdin)
        rows = [(int(v[0]), v[1]) for s in d['data']['result'] for v in s['values']]
    except Exception:
        return
    print(' '.join(firing(rows, int(sys.argv[1]), csv(2), csv(3), set(csv(4)))))


if __name__ == '__main__':
    main()
