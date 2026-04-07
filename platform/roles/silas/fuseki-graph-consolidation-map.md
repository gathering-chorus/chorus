# Fuseki Graph Consolidation Map — #1833

133,775 named graphs → ~20 domain-level graphs.

## Mapping Rules

| Pattern | Target Graph | Notes |
|---|---|---|
| `urn:gathering:photos/*` | `urn:gathering:photos` | canonical, source/*, albums |
| `urn:jb:photos/*` | `urn:gathering:photos` | legacy prefix → canonical |
| `urn:gathering:music/*` | `urn:gathering:music` | albums, tracks |
| `urn:jb:music/*` | `urn:gathering:music` | legacy prefix |
| `urn:jb:documents/*` | `urn:gathering:documents` | folders/*, codebase |
| `urn:jb:sexuality/*` | `urn:jb:sexuality` | image, video, archive, models |
| `urn:gathering:seeds/*` | `urn:gathering:seeds` | captures |
| `urn:gathering:people/*` | `urn:gathering:people` | person nodes, face clusters |
| `urn:gathering:stories/*` | `urn:gathering:stories` | stories domain |
| `urn:gathering:icd/*` | `urn:gathering:icd` | ICD definitions |
| `urn:gathering:ontology/*` | `urn:gathering:ontology` | OWL definitions |
| Pod paths (`jeff/capture/*`) | `urn:gathering:pods/jeff` | SOLID pod data |

## Impact on SPARQL queries

Domain-level graphs stay the same (e.g., `urn:gathering:photos/canonical` → `urn:gathering:photos`).
Per-file sub-graphs collapse into parent. Queries using `GRAPH ?g` for per-resource scoping must switch to a resource-level property.

Kade: audit `GRAPH ?g` patterns in src/services/*.ts before the reload.

## Expected outcome

- Graph count: 133,775 → ~20
- Triple count: 28.5M (unchanged)
- Disk: 787GB → target <50GB
