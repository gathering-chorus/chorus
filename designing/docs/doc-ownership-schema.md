---
owner: wren
topic: doc-management, km
status: canonical
card: 2457
---

# Doc ownership schema

Every `.md` and `.html` file the team authors must declare ownership in YAML front-matter so the KM service can triage drift (wrong cabinet, misfiled, unfiled).

## Front-matter fields

```yaml
---
owner: wren | silas | kade | jeff | shared
topic: comma-sep tags (e.g. doc-management, km)
status: draft | canonical | archived
card: <card-id or empty>
---
```

- `owner` — the role responsible for keeping this doc accurate. `shared` for multi-role artifacts. `jeff` for personal notes.
- `topic` — free-form tags for grouping in the catalog.
- `status` — `draft` (in progress), `canonical` (authoritative), `archived` (historical only).
- `card` — the card that birthed or last updated this doc (optional).

## TTL (ontology projection)

```ttl
@prefix chorus: <urn:chorus:> .
@prefix doc:    <urn:chorus:doc/> .

doc:Doc
    a owl:Class ;
    rdfs:label "Document" .

doc:owner a owl:ObjectProperty ; rdfs:domain doc:Doc ; rdfs:range chorus:Role .
doc:topic a owl:DatatypeProperty ; rdfs:domain doc:Doc ; rdfs:range xsd:string .
doc:status a owl:DatatypeProperty ; rdfs:domain doc:Doc ; rdfs:range xsd:string .
doc:currentPath a owl:DatatypeProperty ; rdfs:domain doc:Doc ; rdfs:range xsd:string .
```

## Example

```markdown
---
owner: silas
topic: observability, pulse
status: canonical
card: 2337
---

# Pulse Service Design
...
```

## Enforcement

- `doc-inventory.sh` flags any `.md` / `.html` without an `owner:` field as `misfiled`.
- New docs created without front-matter block at publish time (future card).
