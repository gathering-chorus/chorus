#!/usr/bin/env python3
"""#4438 — the spine's event registry lives in the model; the JSON is generated.

The source is designing/data/event-type-instances.ttl: one chorus:EventType row
per event type, with its producing domain, the class it is about, its payload
fields, its category and a contract version. The graph loads that file (via the
instance-seed manifest). designing/schemas/spine-events.json keeps its header
(envelope, vertebrae, aliases, product_map, value_stream_map) and its `events`
section is WRITTEN from the rows. Register a new event by adding a row, then run
this script; never edit the JSON's events by hand.

    python3 platform/scripts/event-types-generate.py           # rows -> JSON events
    python3 platform/scripts/event-types-generate.py --check   # exit 1 if the JSON drifted from the rows
    python3 platform/scripts/event-types-generate.py --migrate # one-time: JSON -> rows (done 2026-10-06)

--migrate is how the rows were first made: it read the JSON and the #4431
classification and inferred each producer from the event name's first segment.
It is kept so the first rows can be reproduced, not as the way to register.
"""
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
REGISTRY = ROOT / "designing/schemas/spine-events.json"
CLASSIFICATION = ROOT / "designing/data/event-type-classification-4431.tsv"
ROWS = ROOT / "designing/data/event-type-instances.ttl"
NS = "https://jeffbridwell.com/chorus#"
RDFS_LABEL = "http://www.w3.org/2000/01/rdf-schema#label"
RDFS_COMMENT = "http://www.w3.org/2000/01/rdf-schema#comment"
RDF_TYPE = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"

# --migrate only. First segment of the event name -> the domain that produces it.
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

# --migrate only. First segment -> the model class the event is about (only where clear).
ABOUT = {
    "card": "Card", "cards": "Card", "ac": "Card", "board": "Chunk",
    "session": "Session", "test": "TestResult", "testresult": "TestResult", "testcase": "TestResult",
    "pair": "Message", "chat": "Message", "decision": "ADR",
}

def slug(name: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")

def lit(s: str) -> str:
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n") + '"'

# ---------------------------------------------------------------- rows -> JSON

def consumer_files() -> list:
    """Model files that declare a consumesEvent edge (any domain, product or service row)."""
    found = []
    for base in ("designing/data", "roles"):
        for f in sorted((ROOT / base).rglob("*.ttl")):
            if f != ROWS and "consumesEvent " in f.read_text(errors="ignore"):
                found.append(f)
    return found

def read_rows(path: Path) -> dict:
    """Parse the rows with riot (N-Triples out) and return {eventName: json entry}."""
    nt = subprocess.run(["riot", "--output=ntriples", str(path), *map(str, consumer_files())],
                        capture_output=True, text=True, check=True).stdout
    subj: dict = {}
    triple = re.compile(r'^<([^>]+)> <([^>]+)> (?:<([^>]+)>|"((?:[^"\\]|\\.)*)"(?:\^\^<[^>]+>|@[a-z-]+)?) \.$')
    for line in nt.splitlines():
        m = triple.match(line)
        if not m:
            continue
        s, p, iri, text = m.groups()
        value = iri if iri is not None else json.loads('"' + text + '"')
        subj.setdefault(s, {}).setdefault(p, []).append(value)
    readers: dict = {}
    for s_iri, props in subj.items():
        for et in props.get(NS + "consumesEvent", []):
            readers.setdefault(et, []).append(s_iri.rsplit("#", 1)[-1])
    events = {}
    for s_iri, props in subj.items():
        if NS + "EventType" not in props.get(RDF_TYPE, []):
            continue
        one = lambda p: (props.get(NS + p) or [None])[0]
        entry: dict = {}
        # The EDA fields every reader can filter on (#4438): who produces it,
        # what it is about, fact or diagnostic, contract version.
        local = lambda iri: iri.rsplit("#", 1)[-1] if iri else None
        entry["producer"] = local(one("producedBy"))
        if one("eventAbout"):
            entry["about"] = local(one("eventAbout"))
        entry["category"] = one("eventCategory")
        entry["version"] = one("eventVersion")
        if readers.get(s_iri):
            entry["consumers"] = sorted(readers[s_iri])
        if one("atVertebra"):
            entry["vertebra"] = one("atVertebra")
        if one("emitterSource"):
            entry["source"] = one("emitterSource")
        if props.get(RDFS_COMMENT):
            entry["description"] = props[RDFS_COMMENT][0]
        if one("legacyName"):
            entry["legacy_name"] = one("legacyName")
        if one("joinKeys") is not None:
            entry["keys"] = [k for k in one("joinKeys").split(",") if k]
        if one("envelopeFields") is not None:
            entry["envelope"] = [k for k in one("envelopeFields").split(",") if k]
        entry["fields"] = dict(sorted(f.split(": ", 1) for f in props.get(NS + "payloadField", [])))
        events[one("eventName")] = entry
    return dict(sorted(events.items()))

def write_json(check: bool) -> int:
    doc = json.loads(REGISTRY.read_text())
    generated = read_rows(ROWS)
    if check:
        drift = sorted(set(doc["events"]) ^ set(generated)) + sorted(
            n for n in set(doc["events"]) & set(generated) if doc["events"][n] != generated[n])
        print(f"rows {len(generated)} · json {len(doc['events'])} · drifted {len(drift)}")
        if drift:
            print("drifted:", ", ".join(drift[:20]), "…" if len(drift) > 20 else "")
            print("The events in spine-events.json are generated from designing/data/event-type-instances.ttl.")
            print("Register or change an event in the rows, then run platform/scripts/event-types-generate.py.")
            return 1
        return 0
    doc["events"] = generated
    REGISTRY.write_text(json.dumps(doc, indent=2, ensure_ascii=False) + "\n")
    print(f"wrote {REGISTRY.relative_to(ROOT)} events from {ROWS.relative_to(ROOT)} ({len(generated)} event types)")
    return 0

# ---------------------------------------------------------------- JSON -> rows (once)

def migrate() -> int:
    reg = json.loads(REGISTRY.read_text())["events"]
    cat = {}
    for line in CLASSIFICATION.read_text().splitlines():
        if line.startswith("#") or line.startswith("name\t"):
            continue
        parts = line.split("\t")
        if len(parts) >= 4:
            cat[parts[0]] = parts[3]
    # A type seen on the spine (classification) but missing from the registry is
    # registered too, with no payload: what is written must be known.
    for name in cat:
        if name and name not in reg:
            reg[name] = {"description": "Observed on the spine 2026-10-05 but never registered; payload unknown (#4438)."}
    unmapped = [n for n in reg if PRODUCER.get(n.split(".")[0]) is None]
    if unmapped:
        print("unmapped:", ", ".join(sorted(unmapped)))
        return 1
    out = [
        "# The spine's event registry (#4438). THIS FILE IS THE SOURCE: the events in",
        "# designing/schemas/spine-events.json are generated from it by",
        "# platform/scripts/event-types-generate.py. Register an event by adding a row.",
        "@prefix chorus: <https://jeffbridwell.com/chorus#> .",
        "@prefix rdfs:   <http://www.w3.org/2000/01/rdf-schema#> .",
        "",
    ]
    for name in sorted(reg):
        spec, prefix = reg[name], name.split(".")[0]
        lines = [f"chorus:eventtype-{slug(name)} a chorus:EventType ;",
                 f"    rdfs:label {lit(name)} ;",
                 f"    chorus:eventName {lit(name)} ;",
                 f"    chorus:eventCategory {lit(cat.get(name, 'fact'))} ;",
                 f"    chorus:producedBy chorus:{PRODUCER[prefix]} ;",
                 '    chorus:eventVersion "1" ;']
        if ABOUT.get(prefix):
            lines.append(f"    chorus:eventAbout chorus:{ABOUT[prefix]} ;")
        for key, prop in (("vertebra", "atVertebra"), ("source", "emitterSource"), ("legacy_name", "legacyName")):
            if spec.get(key):
                lines.append(f"    chorus:{prop} {lit(spec[key])} ;")
        for key, prop in (("keys", "joinKeys"), ("envelope", "envelopeFields")):
            if key in spec:
                lines.append(f"    chorus:{prop} {lit(','.join(spec[key] or []))} ;")
        if spec.get("description"):
            lines.append(f"    rdfs:comment {lit(spec['description'])} ;")
        for field, meaning in sorted((spec.get("fields") or {}).items()):
            lines.append(f"    chorus:payloadField {lit(field + ': ' + str(meaning))} ;")
        lines[-1] = lines[-1][:-2] + " ."
        out += lines + [""]
    ROWS.write_text("\n".join(out))
    print(f"wrote {ROWS.relative_to(ROOT)} ({len(reg)} event types)")
    return 0

if __name__ == "__main__":
    if "--migrate" in sys.argv:
        sys.exit(migrate())
    sys.exit(write_json("--check" in sys.argv))
