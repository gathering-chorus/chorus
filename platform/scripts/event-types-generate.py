#!/usr/bin/env python3
"""#4438 — the spine's event registry, as model rows.

Reads designing/schemas/spine-events.json (the registry, 373 types) and
designing/data/event-type-classification-4431.tsv (fact | diagnostic per type),
and writes designing/data/event-type-instances.ttl: one chorus:EventType per
registered type, with its producing domain, the class it is about, its payload
fields, its category and a contract version.

The producer is read from the event name's first segment (card.pulled → cards).
A prefix with no producing domain is reported, never guessed: --check exits 1
while any registered type has no producer.

    python3 platform/scripts/event-types-generate.py          # write the TTL
    python3 platform/scripts/event-types-generate.py --check  # report, write nothing
"""
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
REGISTRY = ROOT / "designing/schemas/spine-events.json"
CLASSIFICATION = ROOT / "designing/data/event-type-classification-4431.tsv"
OUT = ROOT / "designing/data/event-type-instances.ttl"

# First segment of the event name -> the domain that produces it (a Domain row).
PRODUCER = {
    "card": "cards", "cards": "cards", "board": "board", "ac": "cards",
    "demo": "pipelines", "werk": "pipelines", "env": "pipelines", "workflow": "pipelines",
    "accept": "pipelines", "review": "pipelines", "signal": "pipelines",
    "build": "builds", "deploy": "deploys", "binary": "deploys", "manifest": "deploys",
    "test": "tests", "tests": "tests", "testresult": "tests", "testsuiterun": "tests", "unit": "tests",
    "merge": "version-control", "commit": "version-control", "push": "version-control",
    "rebase": "version-control", "pull": "version-control", "unpull": "version-control",
    "continue": "version-control", "abort": "version-control", "slot": "version-control",
    "canonical": "version-control", "fetch": "version-control",
    "model": "domains", "athena": "athena-domain", "hydration": "code",
    "crawler": "code", "codebase": "code", "sharedlib": "code",
    "session": "identity", "role": "roles", "guard": "gates", "hook": "gates",
    "decision": "decisions", "seed": "seeds", "harvest": "integrations",
    "pair": "messages", "chat": "messages", "brief": "messages", "interaction": "messages",
    "search": "search", "ops": "observability", "service": "infrastructure",
    "observer": "observability", "self": "observability", "library": "infrastructure",
    "app": "infrastructure", "design": "documents", "protocol": "documents",
    "batch": "integrations", "consumer": "integrations", "content": "knowledge",
    "gate": "gates", "membrane": "gates", "gh": "version-control", "lock": "version-control",
    "worktree": "version-control", "werk-code": "pipelines", "teardown": "pipelines",
    "nightly": "tests", "testcase": "tests", "process": "infrastructure",
    # observed on the spine but never registered (measured 10-05, #4431)
    "context": "memory", "nudge": "messages", "reply": "messages", "messages": "messages",
    "terminal": "messages", "jeff": "messages", "clearing": "messages", "security": "security",
    "mcp": "infrastructure", "system": "observability", "heartbeat": "observability",
    "health": "observability", "eventloop": "observability", "chorus": "observability",
    "daily": "observability", "agent": "streams", "owl": "domains", "graph": "domains",
    "hooks": "gates", "stop": "gates", "word": "gates", "stated": "gates", "quality": "gates",
    "skill": "skills", "wip": "board", "emit": "events", "lance": "search",
}

# First segment -> the model class the event is about (only where it is clear).
ABOUT = {
    "card": "Card", "cards": "Card", "ac": "Card", "board": "Chunk",
    "session": "Session", "test": "TestResult", "testresult": "TestResult", "testcase": "TestResult",
    "pair": "Message", "chat": "Message", "decision": "ADR",
}

def slug(name: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")

def lit(s: str) -> str:
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", " ") + '"'

def main(check: bool) -> int:
    reg = json.loads(REGISTRY.read_text())["events"]
    cat = {}
    for line in CLASSIFICATION.read_text().splitlines():
        if line.startswith("#") or line.startswith("name\t"):
            continue
        parts = line.split("\t")
        if len(parts) >= 4:
            cat[parts[0]] = parts[3]
    # A type seen on the spine (classification) but missing from the registry is
    # registered here too, with no payload: what is written must be known.
    for name in cat:
        if name and name not in reg:
            reg[name] = {"description": "Observed on the spine 2026-10-05 but never registered; payload unknown (#4438)."}
    rows, unmapped = [], []
    for name in sorted(reg):
        prefix = name.split(".")[0]
        producer = PRODUCER.get(prefix)
        if producer is None:
            unmapped.append(name)
            continue
        rows.append((name, producer, ABOUT.get(prefix), reg[name], cat.get(name, "fact")))
    print(f"registered {len(reg)} · with producer {len(rows)} · unmapped {len(unmapped)}")
    if unmapped:
        print("unmapped:", ", ".join(unmapped))
    if check:
        return 1 if unmapped else 0
    out = [
        "# GENERATED by platform/scripts/event-types-generate.py (#4438) — do not edit.",
        "# Source: designing/schemas/spine-events.json + event-type-classification-4431.tsv.",
        "@prefix chorus: <https://jeffbridwell.com/chorus#> .",
        "@prefix rdfs:   <http://www.w3.org/2000/01/rdf-schema#> .",
        "",
    ]
    for name, producer, about, spec, category in rows:
        lines = [f"chorus:eventtype-{slug(name)} a chorus:EventType ;",
                 f"    rdfs:label {lit(name)} ;",
                 f"    chorus:eventName {lit(name)} ;",
                 f"    chorus:eventCategory {lit(category)} ;",
                 f"    chorus:producedBy chorus:{producer} ;",
                 '    chorus:eventVersion "1" ;']
        if about:
            lines.append(f"    chorus:eventAbout chorus:{about} ;")
        if spec.get("description"):
            lines.append(f"    rdfs:comment {lit(spec['description'][:400])} ;")
        for field, meaning in sorted((spec.get("fields") or {}).items()):
            lines.append(f"    chorus:payloadField {lit(field + ': ' + str(meaning)[:200])} ;")
        lines[-1] = lines[-1][:-2] + " ."
        out += lines + [""]
    OUT.write_text("\n".join(out))
    print(f"wrote {OUT.relative_to(ROOT)} ({len(rows)} event types)")
    return 0

if __name__ == "__main__":
    sys.exit(main("--check" in sys.argv))
