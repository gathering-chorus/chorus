#!/usr/bin/env python3
"""crawler-stale-check.py — parse one chorus.log line, print epoch seconds.

Reads ONE JSONL line on stdin (the most recent crawler.domain.indexed event),
parses its timestamp robustly (-0400 + Z normalized), prints the epoch as int.
Exits 2 on parse failure.

Sibling-script pattern (#2861).
"""
import datetime
import json
import re
import sys


def normalize_iso(ts: str) -> str:
    ts = re.sub(r"([+-])(\d{2})(\d{2})$", r"\1\2:\3", ts)
    return ts.replace("Z", "+00:00")


def main() -> int:
    line = sys.stdin.readline()
    if not line.strip():
        return 2
    try:
        d = json.loads(line)
        ts = d.get("timestamp", "")
        if not ts:
            return 2
        parsed = datetime.datetime.fromisoformat(normalize_iso(ts))
        print(int(parsed.timestamp()))
        return 0
    except Exception:
        return 2


if __name__ == "__main__":
    sys.exit(main())
