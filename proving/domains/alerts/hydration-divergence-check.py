#!/usr/bin/env python3
"""hydration-divergence-check.py — count crawler.domain.indexed events in last 3 min.

Reads JSONL on stdin (chorus.log lines filtered to crawler.domain.indexed),
counts those whose timestamp >= now - 3min. Prints the count.

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
    cutoff = datetime.datetime.now(datetime.timezone.utc) - datetime.timedelta(minutes=3)
    hits = 0
    for line in sys.stdin:
        try:
            d = json.loads(line)
            parsed = datetime.datetime.fromisoformat(normalize_iso(d.get("timestamp", "")))
            if parsed >= cutoff:
                hits += 1
        except Exception:
            pass
    print(hits)
    return 0


if __name__ == "__main__":
    sys.exit(main())
