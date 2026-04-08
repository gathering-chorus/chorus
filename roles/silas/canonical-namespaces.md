# Canonical Namespace Registry

Authoritative URN table for all Fuseki graphs. No new namespaces without updating this file.

## 1. Instance Data — `urn:jb:*`

| Domain | Graph Namespace | Entity URI Pattern | Graph Count |
|---|---|---|---|
| Blog | `urn:jb:blog/posts/` | `urn:jb:blog/posts/{slug}.ttl` | 43 |
| Books | `urn:jb:books/` | `urn:jb:books/{slug}.ttl` | 69 |
| Capture | `urn:jb:capture/` | `urn:jb:capture/{id}.ttl` | 253 |
| Documents | `urn:jb:documents/folders/` | `urn:jb:documents/folders/{slug}.ttl` | 43,166 |
| Episodes | `urn:jb:episodes/` | `urn:jb:episodes/{slug}.ttl` | 28 |
| Glimmers | `urn:jb:glimmers/` | `urn:jb:glimmers/{slug}.ttl` | 5 |
| Ideas | `urn:jb:ideas/` | `urn:jb:ideas/{slug}.ttl` | 5 |
| Intentions | `urn:jb:intentions/items/` | `urn:jb:intentions/items/{slug}` | 5 |
| Music | `urn:jb:music/albums/` | `urn:jb:music/albums/{slug}` | 13,613 |
| Notes | `urn:jb:notes/items/` | `urn:jb:notes/items/{slug}` | 823 |
| People | `urn:jb:people/` | `urn:jb:people/{slug}.ttl` | 2,831 |
| Photos | `urn:jb:photos/` | `urn:jb:photos/{subgraph}/*.ttl` | 68,098 |
| Photos (subgraphs) | `albums/`, `detections/`, `google/`, `harvests/`, `locations/`, `people/`, `sources/` | per-item TTL | — |
| Practices | `urn:jb:practices/` | `urn:jb:practices/{slug}.ttl` | 13 |
| Property | `urn:jb:property/` | `urn:jb:property/{type}/{slug}.ttl` | 10 |
| Property (subgraphs) | `gardens/`, `houses/`, `lands/` | per-item TTL | — |
| Reading | `urn:jb:reading/items/` | `urn:jb:reading/items/{slug}` | 3 |
| Sexuality | `urn:jb:sexuality/volumes/` | `urn:jb:sexuality/volumes/{slug}.ttl` | 44 |
| Social Posts | `urn:jb:socialposts/items/` | `urn:jb:socialposts/items/{date}-{platform}-{hash}` | 2,075 |
| Stories | `urn:jb:stories/` | `urn:jb:stories/{slug}.ttl` | 132 |
| Values | `urn:jb:values/` | `urn:jb:values/{slug}.ttl` | 5 |
| Home Cloud | `urn:jb:home-cloud/` | `urn:jb:home-cloud/{type}/*.ttl` | 8 |
| Ontology | `urn:jb:ontology` | (schema graph) | 1 |

## 2. Product Schema — `urn:gathering:*`

| Namespace | Purpose | Contents |
|---|---|---|
| `urn:gathering:icd/current` | Active ICD graph | Domains, providers, fields, mappings, implementation contracts |
| `urn:gathering:icd/v1` | ICD version 1 snapshot | Historical — do not write |
| `urn:gathering:icd/v2` | ICD version 2 snapshot | Historical — do not write |
| `urn:gathering:icd/v3` | ICD version 3 snapshot | Historical — do not write |
| `urn:gathering:icd/notes-v2` | Notes v2 migration snapshot | Historical — do not write |
| `urn:gathering:icd#` | ICD property namespace | `icd:Domain`, `icd:Provider`, `icd:CanonicalField`, etc. |

## 3. Chorus / Borg — `urn:chorus:*`, `urn:borg:*`

| Namespace | Purpose | Status |
|---|---|---|
| `urn:chorus:*` | Chorus coordination data (sessions, spine events, decisions) | Reserved — not yet in Fuseki |
| `urn:borg:*` | Borg observation data (gemba notes, behavioral patterns) | Reserved — not yet in Fuseki |

## Rules

1. **All instance data uses `urn:jb:` prefix** — no `http://localhost:*` graph URIs
2. **All product schema uses `urn:gathering:` prefix** — ICD, ontology extensions
3. **Entity URIs inside triples use `urn:jb:` prefix** — not `https://jeffbridwell.com/`
4. **No new top-level namespace without updating this file and running validate-namespaces.sh**
