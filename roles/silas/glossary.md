# Glossary

Last updated: 2026-02-14
Status: v1 — accepted (garden frame terminology pending vision session)

Shared vocabulary for the team. When we use these words, this is what we mean.

---

## A

**ACL (Access Control List)**
A `.acl` file that declares who can access a resource or collection. Based on WAC (Web Access Control — see WAC entry) standard. ACLs are the enforcement artifact — generated at write time to stay in sync with the Turtle visibility declaration (`jb:hasVisibility`), which is the source of truth. See ADR-003.

**Adapter (Harvest)**
The component of a harvest pipeline that handles authentication and data retrieval from an external source. Each source has its own adapter (Google Photos adapter, Spotify adapter, etc.).

**Aggregate Turtle File**
A single Turtle file containing metadata for multiple resources, grouped by a logical boundary (per album, per month, per category). Used when individual files per resource would create too many files (e.g., 1M+ photos). See Pattern B in `content-ingestion-matrix.md`.

## C

**Collection**
A thematic grouping of resources within a pod. Blog, Books, Property, Ideas, Projects are the current collections. Collections are the unit of visibility control — visibility is set per collection, not per resource (in Phase 1).

**Conceptual Model**
The high-level description of what the system is about: its key concepts and how they relate. Not code, not schema — the shared understanding that everyone on the team works from. This document's companion: `conceptual-model.md`.

**Connection (Cross-Domain)**
A relationship between resources in different collections. "This book is on this shelf in this property." These are where the unique value of the knowledge graph lives. Measured by the cross-domain connection ratio.

**Cross-Domain Connection Ratio**
Quality metric: the percentage of resources that have at least one relationship to a resource in a different collection. If this drops as new sources are harvested, the graph is accumulating without connecting.

**Curation**
The human act of deciding what matters, what connects, and what graduates. The activity between harvest (automated intake) and graduation (intentional publishing). Jeff reviewing, annotating, connecting, deciding. This is the core user activity. The AI thinking partner exists to support curation, not replace it.

**Capture Channel**
The intake point for raw, unstructured input — a thought, a photo, a voice note. Pre-idea, pre-resource. The seed before it's planted. No capture channel exists today; all input enters through structured forms or harvest pipelines. Future work: a lightweight intake that feeds the emergence lifecycle (capture → triage → place or compost).

## D

**Default-Deny**
The security principle that if no ACL file exists for a collection, it is treated as private. Protects against missing or corrupt ACL files. See ADR-003.

## E

**Extended Mind**
Conceptual frame for the pod. The knowledge graph is Jeff's extended mind on disk — a structured, queryable, private-by-default representation of his world.

## F

**Fuseki**
Apache Jena Fuseki. The SPARQL query engine that indexes pod data. Holds a copy of the data in named graphs. Not the source of truth — the filesystem Turtle files are. Fuseki is the read-optimized query layer.

**Fire-and-Forget (Sync)**
Current Fuseki sync pattern: after a pod write, the system sends the data to Fuseki without waiting for confirmation. If Fuseki is down, the sync is lost. Known architectural concern — needs reconciliation mechanism at scale.

## G

**Graduation**
Moving content from a more restricted visibility to a less restricted one. Private → Selective → Public. Always an intentional act by Jeff. "The workshop is not the storefront."

**Graduation Model**
The privacy architecture. All content starts private. It graduates toward public only when Jeff explicitly promotes it. The system never pushes content toward public. See ADR-002.

## H

**Harvest / Harvester**
The process and tooling for pulling metadata from external sources into the knowledge graph. Content stays in the source; metadata comes into the pod. The WordPress harvester is the reference implementation.

## I

**Ingestion Depth**
How much of an external source enters the knowledge graph. Four tiers:
- **L0**: Reference only (pointer to source)
- **L1**: Catalog metadata (title, date, source URI)
- **L2**: Rich metadata + relationships (personal annotations, cross-domain links)
- **L3**: Content + metadata (actual content stored in pod)

See `content-ingestion-matrix.md`.

## K

**Knowledge Graph**
The total connected dataset: all pod resources, their properties, and their relationships. Queryable via SPARQL through Fuseki. The value is in the connections, not just the nodes.

## L

**Link Discovery**
Phase of a harvest pipeline where the harvester looks for connections between newly ingested metadata and existing resources. Temporal overlap, shared tags, entity matching. Prevents the data swamp.

**L0 / L1 / L2 / L3**
See Ingestion Depth.

## N

**Named Graph**
In Fuseki, each resource gets its own graph, like a labeled folder. The graph URI maps to the pod filesystem path (e.g., `http://localhost:3000/pods/jeff/books/my-book.ttl`). This is how the system searches "only within books" without scanning everything. Central to visibility enforcement at the data layer.

## O

**Ontology**
The OWL/RDF vocabulary that defines what kinds of things exist in the knowledge graph, what properties they have, and how they relate. The ontology is architecture — changes ripple to SPARQL queries, UI, and AI context. Current version: v0.4.0.

**Opaque URI**
A cross-collection reference that is visible as a URI but not resolved for users who lack access to the target collection. "This book references a shelf" — the link exists but you can't follow it without property collection access. Aligned with Linked Data principles. Phase 2 target.

## P

**Pattern A / B / C / D**
Storage patterns for pod data at different scales:
- **A**: One Turtle file per resource (under ~5k items)
- **B**: Aggregate Turtle files by logical group (10k-100k items)
- **C**: Fuseki-primary with reference manifest (100k+ items)
- **D**: External catalog with harvest bridge

See `content-ingestion-matrix.md`.

**Pod**
The personal data store. A structured collection of Turtle (RDF) files on the filesystem. Jeff owns his pod. It's the source of truth — not Fuseki, not any external service.

**PodWriteService**
The single choke point for all pod writes. Writes Turtle to disk and triggers Fuseki sync. Cache invalidation for ACLs flows through here.

**Prometheus Guardrail**
Design principle: no capability that makes the system harder to reason about, maintain, or debug. Every addition must earn its complexity. Named after Jeff's observation about technology as fire.

## R

**Resource**
A single item within a collection. A book, a blog post, a garden bed. Stored as a Turtle file (or within an aggregate). Has a stable URI that persists across its lifetime and across ingestion tier promotions.

## S

**Semantic Memory Layer**
What this system is. Not a content management system — a memory layer. Content lives where it lives. The knowledge graph owns the metadata: what Jeff has, when it mattered, how it connects.

**SHACL**
Shapes Constraint Language. Defines validation rules — think of it as "the data must look like this." Currently validates ontology quality (are classes and properties well-formed?). Future use: validate instance data (does every book have a title and author?). Instance shapes should be collection-scoped to avoid cross-visibility-boundary dependencies.

**Source (Harvested)**
An external system the knowledge graph harvests from. Each source has its own ingestion tier (L0-L3), adapter, and sync status. Examples: WordPress (L3), Google Photos (L1), Spotify (L1). The graph doesn't replace the source — it remembers what was there and how it connects. See `content-ingestion-matrix.md`.

**SOLID**
Social Linked Data. The web standard for personal data pods with identity (OIDC) and access control (WAC). The architectural foundation for pod ownership and the graduation model.

**SPARQL**
Query language for RDF data. Used to search and analyze the knowledge graph via Fuseki. Admin-only access in current architecture. Collection handlers read from the filesystem, not Fuseki (audit 2026-02-14). Scoped query pattern established for when non-admin SPARQL paths are built.

**Storefront**
The experience an unauthenticated visitor has. What they see when they arrive at the public URL. The graduation destination — where content ends up after it goes public. Currently: individual public collection pages. Future: a curated front door showing public collections as a portfolio.

## T

**Turtle (.ttl)**
Terse RDF Triple Language. The file format for pod data. Human-readable, text-based, portable. Each Turtle file contains RDF triples describing one or more resources.

**TDB2**
Jena's persistent storage backend for Fuseki. Confirmed in current configuration (2026-02-14). Docker volume at `/fuseki`, 1GB heap sufficient for 4-7M triples. Scaling path: bump heap when harvesters add volume.

## V

**Visibility**
Access level for a collection. Three tiers: Private (owner only, default), Selective (specific people, Phase 2), Public (anyone). Enforced by `collectionVisibilityMiddleware` at route level and by query scoping at data level.

## W

**WAC (Web Access Control)**
W3C standard for access control in Linked Data / SOLID systems. Uses `.acl` files to declare who can read, write, or control resources. The foundation of the visibility and graduation model.

**WebID**
A URI that identifies a person in the SOLID ecosystem. Used for authentication (SOLID OIDC) and authorization (WAC ACLs reference WebIDs to grant access).
