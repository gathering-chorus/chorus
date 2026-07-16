#!/usr/bin/env python3
"""cron-due.py — #3617: is a rule's cron schedule due?

The alert-runner executed EVERY rule on EVERY cycle — the `schedule:` field in
alerts/*.yml was decorative (only "manual" was honored). That's why the 8am-only
fuseki-harvest rule fired at midnight and the 6-hourly lance rule joined the
00:00 battery. This matcher makes the declared schedule real.

Usage: cron-due.py '<5-field cron>' <last_run_epoch> <now_epoch>
Exit 0 if any minute in (last_run, now] matches the expression; else exit 1.
A last_run of 0 (never ran) checks only the current minute, so a fresh install
doesn't replay history. Deterministic: time comes in as arguments, never Date-now
inside — the bats tests pin exact epochs.
"""
import sys
import datetime


def field_matches(field: str, value: int) -> bool:
    for part in field.split(','):
        part = part.strip()
        if part == '*':
            return True
        if part.startswith('*/'):
            try:
                step = int(part[2:])
                if step > 0 and value % step == 0:
                    return True
            except ValueError:
                continue
        elif '-' in part:
            try:
                lo, hi = part.split('-', 1)
                if int(lo) <= value <= int(hi):
                    return True
            except ValueError:
                continue
        else:
            try:
                if int(part) == value:
                    return True
            except ValueError:
                continue
    return False


def cron_matches(expr: str, dt: datetime.datetime) -> bool:
    fields = expr.split()
    if len(fields) != 5:
        return True  # malformed schedule: fail open (rule runs, never silently dead)
    minute, hour, dom, month, dow = fields
    return (
        field_matches(minute, dt.minute)
        and field_matches(hour, dt.hour)
        and field_matches(dom, dt.day)
        and field_matches(month, dt.month)
        and field_matches(dow, dt.weekday() + 1 if dt.weekday() < 6 else 0)  # cron: 0=Sun
    ) or False


def main() -> int:
    expr, last, now = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
    now_dt = datetime.datetime.fromtimestamp(now)
    if last <= 0:
        return 0 if cron_matches(expr, now_dt) else 1
    # scan each whole minute in (last, now], capped at 24h so a stale state file
    # can't make this loop unbounded
    start = max(last + 60 - (last % 60), now - 86400)
    t = start
    while t <= now:
        if cron_matches(expr, datetime.datetime.fromtimestamp(t)):
            return 0
        t += 60
    return 1


if __name__ == '__main__':
    sys.exit(main())
