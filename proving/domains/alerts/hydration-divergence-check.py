#!/usr/bin/env python3
"""hydration-divergence-check.py — count crawler.domain.indexed events in the last cycle window.

Reads JSONL on stdin (chorus.log lines filtered to crawler.domain.indexed), counts those
whose timestamp >= now - 40min (#3322: one full StartInterval=1800 cycle + grace; was 3min
under the retired pre-#3068 WatchPaths premise). Prints the count.

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
    cutoff = datetime.datetime.now(datetime.timezone.utc) - datetime.timedelta(minutes=40)
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
