#!/usr/bin/env python3
# @domain: events
"""#4438 — the spine's event registry lives in the store; the JSON is generated.

Every event type is a chorus:EventType row in the events domain's graph, read
through the generated API (GET /owl/v1/events/types). designing/schemas/
spine-events.json keeps its header (envelope, vertebrae, aliases, product_map,
value_stream_map) and its `events` section is WRITTEN from those rows.

Register an event by writing its row through the door
(POST /owl/v1/events/types), then run this script. Never edit the JSON's events
by hand, and never put rows in a seed file: seed is for the OWL (Jeff,
2026-10-06).

    python3 platform/scripts/event-types-generate.py          # store -> JSON events
    python3 platform/scripts/event-types-generate.py --check  # exit 1 if the JSON drifted from the store

EVENT_TYPES_URL / EVENT_TYPES_REGISTRY override where rows are read from and
which JSON is written (tests point them at fixtures).
"""
import json
import os
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
REGISTRY = Path(os.environ.get("EVENT_TYPES_REGISTRY", ROOT / "designing/schemas/spine-events.json"))
URL = os.environ.get("EVENT_TYPES_URL", "http://localhost:3340/owl/v1/events/types?limit=5000")


def many(v):
    # the API serves an absent field as "" and a repeated one as a list
    return [x for x in (v if isinstance(v, list) else [v]) if x]


def local(iri):
    return iri.rsplit("#", 1)[-1].rsplit(":", 1)[-1] if iri else None


def read_rows() -> dict:
    with urllib.request.urlopen(URL, timeout=30) as r:
        doc = json.load(r)
    if doc.get("count") != len(doc.get("data", [])):
        raise SystemExit(f"read {len(doc.get('data', []))} of {doc.get('count')} event types: raise the limit, never write a partial registry")
    events = {}
    for row in doc["data"]:
        entry = {"producer": local(row.get("producedBy"))}
        if row.get("eventAbout"):
            entry["about"] = local(row["eventAbout"])
        entry["category"] = row.get("eventCategory")
        entry["version"] = row.get("eventVersion")
        for key, field in (("vertebra", "atVertebra"), ("source", "emitterSource"),
                           ("description", "comment"), ("legacy_name", "legacyName")):
            if row.get(field):
                entry[key] = row[field]
        for key, field in (("keys", "joinKeys"), ("envelope", "envelopeFields")):
            if row.get(field) is not None:
                entry[key] = [k for k in row[field].split(",") if k]
        entry["fields"] = dict(sorted(f.split(": ", 1) for f in many(row.get("payloadField"))))
        events[row["eventName"]] = entry
    return dict(sorted(events.items()))


def main(check: bool) -> int:
    doc = json.loads(REGISTRY.read_text())
    rows = read_rows()
    if check:
        drift = sorted(set(doc["events"]) ^ set(rows)) + sorted(
            n for n in set(doc["events"]) & set(rows) if doc["events"][n] != rows[n])
        print(f"store {len(rows)} · json {len(doc['events'])} · drifted {len(drift)}")
        if drift:
            print("drifted:", ", ".join(drift[:20]), "…" if len(drift) > 20 else "")
            print("spine-events.json's events are generated from the store: write the EventType row, then run platform/scripts/event-types-generate.py.")
            return 1
        return 0
    doc["events"] = rows
    REGISTRY.write_text(json.dumps(doc, indent=2, ensure_ascii=False) + "\n")
    print(f"wrote {REGISTRY} events from {URL} ({len(rows)} event types)")
    return 0


if __name__ == "__main__":
    sys.exit(main("--check" in sys.argv))
