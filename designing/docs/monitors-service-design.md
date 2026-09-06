# Monitors — Service Design

**Silas, 2026-09-06. Card #4085. Status: Draft.**
**Owner:** Silas (operations — DEC-022).
**Persistence:** `urn:chorus:domains:monitors`.
**Schema:** `chorus:Monitor` + `chorus:MonitorShape`, `roles/silas/ontology/alerts-4085.ttl`.

Short by design, per #4064: a service's commitments are rows with a status and
a card, not paragraphs here.

## Why monitors is its own domain

A monitor measures. An alert routes. They were one row called `alerts-monitors`
until this card, and neither had a class.

| | asks | example |
|---|---|---|
| **Monitor** | is this working right now? | a probe, a health check |
| **Alert** | who gets interrupted, and when? | a rule, a route |

A monitor with nobody routing it is a number in a log. An alert with nothing
measuring it cannot fire. They need each other and they are not the same thing.

## Promise

Every check that measures something is a row naming three things:

1. **What it measures** — one sentence, in the check's own words (`measures`).
2. **Where it lives** — the file, so a reader can open it (`monitorFile`).
3. **What it watches** — the domain (`hasDomain`).

## What is out there

Counted 2026-09-06:

```
 9  security probes    platform/security/probes.d/*.sh
28  named checks       one chorus-health run (pass + warn + fail)
37  monitors
```

## Not in scope here

**The readings.** What each monitor last returned, and how often it has been
red, needs a join against the spine. That is its own card. This domain
describes the *declared* monitor surface — what is being watched, not what it
last said. Claiming otherwise would make the section look answered.

## Related

- `#4085` — this card: the split, both classes, both shapes.
- `#4088` — the monitors harvester and the readings join.
- `#4083` — security as one whole domain.
