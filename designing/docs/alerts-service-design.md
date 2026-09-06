# Alerts — Service Design

**Silas, 2026-09-06. Card #4085. Status: Draft.**
**Owner:** Silas (operations — DEC-022).
**Persistence:** `urn:chorus:domains:alerts`.
**Schema:** `chorus:Alert` + `chorus:AlertShape`, `roles/silas/ontology/alerts-4085.ttl`.

Short by design. Per #4064 the commitments of a service belong in the graph as
rows with a status and a card, not in prose here. This page says what the
service is for and where its boundary sits; the rest is queryable.

## Why alerts is its own domain

Alerts and monitors were one row called `alerts-monitors` until this card. They
answer different questions:

| | asks | example |
|---|---|---|
| **Monitor** | is this healthy right now? | a probe, a health check |
| **Alert** | who gets interrupted, and when? | a rule, a route, a threshold |

A monitor that nobody routes is a number in a log. An alert with nothing
measuring it cannot fire. Keeping them in one row meant neither had a shape,
and the Alerts section of every domain page fell back to a hand-typed list.

Jeff asked for this split on 2026-06-14, 2026-08-13, and 2026-09-03.

## Promise

Every rule that can interrupt a person is a row that names three things:

1. **What it watches** — the domain it belongs to (`hasDomain`).
2. **Where it is authored** — the file, so a reader can open it (`alertFile`).
3. **Who it interrupts** — a role, or a contact point (`alertRoute`).

No row is typed by hand. The harvester reads the files that already exist and
writes what it finds. A domain page's Alerts section is then a query, and a
domain with no alerts says "no rows" and shows the query, rather than showing
someone's stale memory of May.

## What is actually out there

Counted on disk 2026-09-06, not estimated:

```
 7  Grafana provisioned rules   shared-observability/config/grafana/provisioning/alerting/
13  scripts firing via ops-nudge platform/scripts/
20  alerting sources total
```

The 13 are the important half. Most of what reaches Jeff is not a Grafana rule
— it is a shell script deciding on its own to interrupt him. A harvester that
read only Grafana would report 7 and hide two thirds of the real surface, which
is worse than the empty section it replaces.

## Boundary

This service **reads rule files and writes rows**. It does not:

- install or depend on the Grafana/Prometheus/Loki MCP integrations
- query Prometheus or Loki live
- change the core MCP server

Those are separate cards, deliberately after this one. Jeff named the risk on
2026-09-03: "on alerts I think we get sucked into installing the prometheus
grafana loki mcp features + upgrading our core mcp."

## Not in scope here

**Which alerts have fired, and how often.** That needs a join against the spine
and it is its own card. Until it exists, this domain describes the *declared*
alert surface only — what could fire, not what did. Saying otherwise would make
the section look complete while answering a question it cannot answer.

**Monitors.** `chorus:monitors` is split out by this card so nothing points at
a dead subject, but its class, shape, design and harvester are #4088. The row
is deliberately marked `exploring` with its gaps named, not stubbed to look
finished.

## Related

- `#4085` — this card: the split, the class, the shape, the harvester.
- `#4088` — the monitors half.
- `#4083` — security as one whole domain; this fills its Alerts section.
- `#4064` — commitments as rows, the pattern this doc defers to.
