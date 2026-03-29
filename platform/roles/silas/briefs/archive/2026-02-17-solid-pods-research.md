# Research: Tim Berners-Lee's SOLID and Pods

**From**: Silas (Architect)
**For**: All roles (Jeff, Wren, Kade)
**Date**: 2026-02-17
**Type**: Research briefing — what SOLID is, where it stands, and how it relates to Gathering

---

## 1. Origins and Motivation

Tim Berners-Lee invented the World Wide Web in 1989. By the mid-2010s, he was deeply concerned that his creation had been captured by centralized platforms. Facebook, Google, and Amazon own the data; users are tenants.

SOLID (**So**cial **Li**nked **D**ata) is TBL's answer: **re-decentralize the web** by separating data from applications. The core thesis:

> Your data should live in a place you control (a "pod"). Applications request access to your data — you grant or revoke it. No application owns your data. You can switch applications without losing anything.

This isn't a new idea — it's TBL's original web vision. The early web was decentralized by default (your website was your data). SOLID attempts to restore that property with modern protocols.

TBL started SOLID at MIT in 2015-2016 and co-founded **Inrupt** (2017) to commercialize it.

---

## 2. Conceptual Architecture

### The Big Picture

```
┌─────────────────────────────────────────────────┐
│                   Applications                    │
│  (any app that speaks the Solid Protocol)         │
│  ┌──────┐  ┌──────┐  ┌──────┐  ┌──────┐        │
│  │Photo │  │Health│  │Social│  │ Your │        │
│  │ App  │  │ App  │  │ App  │  │ App  │        │
│  └──┬───┘  └──┬───┘  └──┬───┘  └──┬───┘        │
│     │         │         │         │              │
└─────┼─────────┼─────────┼─────────┼──────────────┘
      │  Solid Protocol (HTTP + LDP + RDF)
      │         │         │         │
┌─────┼─────────┼─────────┼─────────┼──────────────┐
│     ▼         ▼         ▼         ▼              │
│  ┌──────────────────────────────────────┐        │
│  │           Your Pod                    │        │
│  │  /profile/  → WebID, identity         │        │
│  │  /photos/   → photo metadata          │        │
│  │  /health/   → health records          │        │
│  │  /social/   → contacts, messages      │        │
│  │  /.acl      → access control lists    │        │
│  └──────────────────────────────────────┘        │
│                Pod Provider                        │
│  (self-hosted, Inrupt, CSS, etc.)                 │
└───────────────────────────────────────────────────┘
```

### Core Concepts

**Pod (Personal Online Datastore)**: A web-accessible data store that you own. Technically, a set of HTTP-addressable resources organized in containers (like directories). A pod has a URL (e.g., `https://jeff.solidpod.example/`). You can self-host or use a provider.

**WebID**: Your decentralized identity — a URI that resolves to an RDF document describing you. Like a username, but you own it. Example: `https://jeff.solidpod.example/profile/card#me`. Any Solid app can authenticate you via your WebID.

**Linked Data**: All data in pods is RDF (Resource Description Framework) — triples of subject-predicate-object. This is the semantic web stack TBL championed for decades. Data is machine-readable, self-describing, and linkable across pods.

**Solid Protocol**: The HTTP-based protocol for reading, writing, and managing pod data. Built on Linked Data Platform (LDP), extended with authentication (Solid-OIDC) and authorization (WAC/ACP).

**Decoupling**: The radical idea — apps don't store your data. They read/write to your pod with your permission. Switch photo apps? Your photos stay in your pod. Delete an app? Your data persists.

---

## 3. Technical Specifications

### Protocol Stack

| Layer | Standard | Status |
|---|---|---|
| **Core Protocol** | Solid Protocol v0.12.0 | Editor's Draft (April 2025) |
| **Authentication** | Solid-OIDC v0.1.0 | CG-DRAFT |
| **Authorization** | Web Access Control (WAC) v1.0.0 | CG-DRAFT |
| **Authorization** | Access Control Policy (ACP) v0.9.0 | Alternative to WAC |
| **Identity** | WebID Profile v1.0.0 | CG-DRAFT |
| **Data Format** | RDF (Turtle, JSON-LD) | W3C Recommendations |
| **Container Model** | Linked Data Platform (LDP) | W3C Recommendation |
| **Notifications** | 5 channel types (WebSocket, Webhook, SSE, etc.) | CG-DRAFTs |
| **Interop** | Solid Application Interoperability v0.1.0 | CG-DRAFT |
| **Schemas** | Shape Trees | Editor's Draft |

**Key point**: None of these are W3C Recommendations yet. They're Community Group drafts — technically mature but not formally standardized. The Solid Protocol has been at v0.x for years.

### How It Works (HTTP Level)

```
# Read a resource
GET /books/dune.ttl HTTP/1.1
Host: jeff.pod.example
Accept: text/turtle
Authorization: DPoP <token>

# Response: RDF triples
@prefix dcterms: <http://purl.org/dc/terms/> .
<> a <http://schema.org/Book> ;
   dcterms:title "Dune" ;
   dcterms:creator <https://dbpedia.org/resource/Frank_Herbert> .
```

- **GET/HEAD/OPTIONS** for reading
- **PUT** to create/replace resources
- **POST** to create resources in containers
- **PATCH** (using N3 patch format) for partial updates
- **DELETE** to remove
- Content negotiation: servers MUST serve `text/turtle` and `application/ld+json`

### Authentication Flow

1. App discovers user's identity provider (from WebID)
2. App redirects to identity provider (Solid-OIDC, built on OpenID Connect)
3. User authenticates, grants app access
4. App receives DPoP-bound token (proof of possession)
5. App uses token to access pod resources
6. Pod server checks WAC/ACP rules to authorize the request

### Access Control (WAC)

Each resource can have an `.acl` file:

```turtle
# /books/.acl
@prefix acl: <http://www.w3.org/ns/auth/acl#> .

<#owner>
    a acl:Authorization ;
    acl:agent <https://jeff.pod.example/profile/card#me> ;
    acl:accessTo </books/> ;
    acl:mode acl:Read, acl:Write, acl:Control .

<#public>
    a acl:Authorization ;
    acl:agentClass foaf:Agent ;
    acl:accessTo </books/> ;
    acl:mode acl:Read .
```

**Critical limitation**: ACLs operate at the document level, not the triple level. If a document has both public and private data, you can't selectively share — it's all or nothing per resource.

---

## 4. Pod Architecture — What's Inside

### Document-Centric Model (Current Practice)

A pod is a hierarchical filesystem of containers and resources, exposed over HTTP:

```
https://jeff.pod.example/
├── profile/
│   ├── card.ttl          (WebID, identity)
│   └── card.ttl.acl      (access control)
├── books/
│   ├── .acl              (container-level ACL)
│   ├── dune.ttl
│   └── siddhartha.ttl
├── photos/
│   ├── .acl
│   └── 2024-garden/
│       ├── rose.ttl      (metadata, not the image)
│       └── tomato.ttl
├── inbox/                (Linked Data Notifications)
└── settings/
    └── preferences.ttl
```

Each `.ttl` file contains RDF triples. Each container has an `.acl` file. The hierarchy is meaningful — it determines ACL inheritance and resource grouping.

### Graph-Centric Model (Emerging Research)

A 2024 research paper from SolidLab ("What's in a Pod?") argues the document-centric model has fundamental problems:

- **Conflicting hierarchies**: A contacts app wants `/people/amal.ttl`, a birthday app wants `/birthdays/january.ttl`. Same data, different organizational needs. Only one hierarchy can exist.
- **ACL granularity**: A blood test with vitamin levels (shareable) and sensitive results (private) is one document — can't share selectively.
- **Interoperability**: Apps assume specific directory structures. No standard paths.

The proposed alternative: treat pods as **knowledge graphs** with multiple projections. The filesystem hierarchy becomes one view; SPARQL queries, quad pattern fragments, and other interfaces become others. Data is organized by semantics, not by file location.

**This is directly relevant to Gathering** — we already use the graph-centric approach (pods for storage + Fuseki for query).

---

## 5. Current State (2024-2026)

### Governance Transition

**October 2024**: Solid governance transferred from TBL/Inrupt to the **Open Data Institute (ODI)** — the organization TBL co-founded with Sir Nigel Shadbolt. The ODI now provides governance and management while the community focuses on implementation. New governance structures include an Operations Advisory Group and an Advisory Committee.

### W3C Linked Web Storage Working Group

**January 2025**: The W3C chartered the **Linked Web Storage (LWS) Working Group** — the first formal W3C Working Group for Solid-related work (previous work was Community Group level). This is the path to making Solid specs actual W3C Recommendations. The group is developing the "Linked Web Storage Protocol" specification.

### Key Implementations

| Server | Type | Status |
|---|---|---|
| **Community Solid Server (CSS)** | Open source (Node.js) | Active development, reference implementation |
| **Node Solid Server (NSS)** | Open source (Node.js) | Legacy, maintenance mode |
| **Inrupt ESS** | Commercial (Kubernetes) | v2.2, enterprise features (audit, TLS, OIDC, scaling) |

### Adoption

- **Flanders (Belgium)**: Government-backed Solid deployment for citizen data — the largest real-world Solid deployment. Citizens control health, education, and social service data in pods.
- **UK NHS**: Explored Solid for patient data sovereignty (research stage).
- **BBC**: Investigated Solid for media data interoperability.
- **ActivityPods**: Bridges Solid and ActivityPub (the protocol behind Mastodon). Released Mastopod (Mastodon-compatible, data in pods). Presented at FOSDEM 2025. Replacing Jena Fuseki with NextGraph for encryption.

### Inrupt

TBL's company to commercialize Solid. Enterprise Solid Server (ESS) is their product — Kubernetes-native, microservices architecture, enterprise features. They've raised significant funding and have government contracts (Flanders). However, the community has expressed tension between Inrupt's commercial direction and the open-source project's pace.

---

## 6. Criticisms and Challenges

### From Practitioners

A widely-cited 2024 critique ("Baffled by Solid" by Leigh Dodds, former ODI CTO):

- **Poor developer experience**: Pods are basic document stores with no query API. Developers must build their own search/indexing.
- **No standardized schemas**: Solid says "store RDF" but doesn't say which vocabularies to use. Apps can't interoperate if they use different schemas.
- **ACL granularity**: Document-level only — can't share individual fields.
- **No auditing**: Users can't see which apps accessed their data or when.
- **Missing basics**: No mobile apps, no data import tools, no migration utilities.
- **Value proposition unclear**: "Why would I use a pod instead of Dropbox?"

### Architectural Concerns

- **Performance**: RDF parsing, content negotiation, and HTTP-per-resource add overhead vs. traditional databases. No built-in query optimization.
- **Complexity**: The full Solid stack (OIDC + DPoP + WAC + LDP + RDF + content negotiation) is complex to implement correctly. Few developers are fluent in all of these.
- **Slow standardization**: The core protocol has been at v0.x for years. Community Group specs aren't binding.
- **Chicken-and-egg**: No apps without pods, no pods without apps, no users without both.

### What Defenders Say

- The Flanders deployment proves it works at scale with government backing
- Solid-OIDC and WAC are mature enough for production
- The slow pace is deliberate — getting decentralization right is hard
- The W3C LWS Working Group (2025) signals formal standardization is finally happening

---

## 7. Relationship to Other Standards

### vs. ActivityPub (Mastodon/Fediverse)

| | Solid | ActivityPub |
|---|---|---|
| **Focus** | Data ownership | Social federation |
| **Data model** | RDF (any structured data) | ActivityStreams (social activities) |
| **Storage** | Personal pods | Server-hosted accounts |
| **Protocol** | HTTP + LDP | HTTP + JSON-LD activities |
| **Adoption** | Niche (government, research) | Mainstream (10M+ Mastodon users) |

**ActivityPods** bridges the two — Solid pods with ActivityPub federation. TBL has expressed interest.

### vs. AT Protocol (Bluesky)

| | Solid | AT Protocol |
|---|---|---|
| **Philosophy** | Data portability via personal stores | Data portability via portable accounts |
| **Identity** | WebID (self-hosted URI) | DID (decentralized identifier) |
| **Data format** | RDF (Turtle, JSON-LD) | CBOR/JSON (Lexicon schemas) |
| **Federation** | Pod-to-app (vertical) | Relay-to-relay (horizontal) |
| **Adoption** | Small | Growing (Bluesky: 25M+ users) |

AT Protocol is pragmatic where Solid is principled. AT Protocol sacrificed some decentralization ideals for usability and got mainstream adoption. Solid held firm on principles and hasn't.

### vs. MCP (Model Context Protocol)

MCP solves a different problem (connecting AI models to tools/data), but there's conceptual overlap:
- Both use JSON-RPC
- Both separate the client (consumer) from the data (provider)
- A Solid pod could be exposed as an MCP resource server
- An MCP server could read/write to a Solid pod

No integration exists today, but the architectural fit is natural.

---

## 8. What This Means for Gathering

### What We Took from SOLID

Gathering uses SOLID concepts but not the full SOLID stack:

| SOLID Concept | Gathering Implementation | Full SOLID Spec? |
|---|---|---|
| Pods (data stores) | Turtle files on filesystem | Partial — no LDP, no HTTP API on pods |
| RDF/Turtle | Yes — all metadata in Turtle | Yes |
| ACL system | WAC-style `.acl` files | Partial — our own middleware, not spec WAC |
| WebID/OIDC | Solid-OIDC via Pivot | Yes — closest to spec compliance |
| Linked Data | Yes — URIs, triples, ontology | Yes |
| Decoupling | No — tight coupling between app and data | No — we are app AND pod |
| Federation | No — single user, single machine | N/A for our use case |
| SPARQL query | Fuseki alongside pods | Not in SOLID spec (pods have no query API) |

### Where We Diverge (Deliberately)

1. **We added SPARQL indexing.** SOLID's biggest criticism is that pods are dumb document stores. We solved this by running Fuseki alongside — pods are the source of truth, Fuseki is the query index. The SolidLab "What's in a Pod?" paper argues this is the right direction (graph-centric model over document-centric).

2. **We don't federate.** SOLID assumes multiple users, multiple pods, multiple apps. Gathering is one user, one machine. Federation is irrelevant for us — but the data model would support it if we ever wanted it.

3. **Our ACLs are custom.** We use `.acl` files and a visibility middleware (ADR-003), but our enforcement layer is Express middleware, not a SOLID-compliant authorization server. This is simpler and sufficient for one user.

4. **Our pods aren't HTTP-addressable independently.** In SOLID, a pod is a web server. In Gathering, pods are a filesystem directory that the Express app reads. The app is the only interface.

### Architectural Validation

SOLID validates several of our design choices:
- **Turtle/RDF as the data format** — we're on the right standard
- **ACL-per-resource** — our visibility model aligns with WAC patterns
- **Separation of content and metadata** — SOLID's core principle, our L1 harvest pattern
- **Graph-centric data model** — the direction SOLID research is heading (pods + SPARQL)

### What We Could Adopt Later

- **Solid-OIDC compliance** — we're close already (Pivot). Full compliance would let Gathering interoperate with other Solid apps.
- **LDP container semantics** — making our pod hierarchy browsable over HTTP. Low priority but conceptually clean.
- **Notifications** — SOLID's notification spec (WebSocket, Webhook, SSE) could replace our Fuseki sync approach.
- **Shape Trees** — formal schema declarations for pod structures. Could complement our SHACL validation.

---

## Sources

- [Solid Project](https://solidproject.org/) — official site
- [Solid Protocol v0.12.0](https://solidproject.org/ED/protocol) — Editor's Draft
- [Solid Technical Reports](https://solid.github.io/specification/) — full spec index
- [W3C Linked Web Storage WG](https://www.w3.org/groups/wg/lws/) — formal W3C Working Group (2025)
- [ODI + Solid Governance](https://theodi.org/news-and-events/news/odi-and-solid-come-together-to-give-individuals-greater-control-over-personal-data/) — October 2024 transition
- [What's in a Pod?](https://solidlabresearch.github.io/WhatsInAPod/) — SolidLab research paper on pod architecture
- [Baffled by Solid](https://blog.ldodds.com/2024/03/12/baffled-by-solid/) — Leigh Dodds' practitioner critique
- [Inrupt ESS](https://www.inrupt.com/products/enterprise-solid-server) — enterprise implementation
- [Community Solid Server](https://github.com/CommunitySolidServer/CommunitySolidServer) — open source reference
- [ActivityPods](https://activitypods.org/) — Solid + ActivityPub bridge
- [Solid Wikipedia](https://en.wikipedia.org/wiki/Solid_(web_decentralization_project)) — overview

---

— Silas
