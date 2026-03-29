# Card #980 — People & Network Harvest

**From:** Wren | **To:** Kade | **Date:** 2026-03-04

## What

Harvest LinkedIn connections and Facebook friends into RDF as `jb:Person` triples. Searchable in app, browseable, cross-source deduped.

## Data Sources

**LinkedIn** — `~/Desktop/Complete_LinkedInDataExport_03-01-2026.zip.zip`
- `Connections.csv` — 2,104 rows (First Name, Last Name, URL, Email Address, Company, Position, Connected On)
- `ImportedContacts.csv` — synced phone contacts (FirstName, LastName, Emails, PhoneNumbers, timestamps)

**Facebook** — `~/Downloads/facebook-jeffbridwell169465-2026-03-03-34FwtHTO/`
- `connections/friends/your_friends.json` — 334 friends (`friends_v2` array, each has `name` + `timestamp`)

## Ontology

`jb:Person` already exists in `jb-ontology.ttl`. I've added five new harvest properties:
- `jb:sourceNetwork` (string — "linkedin", "facebook")
- `jb:connectedAt` (xsd:dateTime)
- `jb:company` (string, LinkedIn only)
- `jb:position` (string, LinkedIn only)
- `jb:profileUrl` (xsd:anyURI, LinkedIn only)

Existing properties to also use: `jb:fullName`, `jb:firstName`, `jb:lastName`, `jb:harvestedIn` → "people".

**Graph:** `http://localhost:3000/pods/jeff/people/`

**URI pattern:** `http://localhost:3000/pods/jeff/people/<kebab-case-name>` (disambiguate with suffix if needed)

## Dedup Strategy

- Normalize names: lowercase, trim whitespace
- Match FB friend name against LI "FirstName LastName"
- Same person from both sources → single URI, multiple `jb:sourceNetwork` values
- ImportedContacts → merge onto matching Connection by name (don't create standalone Person for unmatched imports — that's phone contacts noise)

## Pattern

Follows #443 (social harvest). Same harvester structure — CSV/JSON parser → RDF triples → Fuseki load → search index.

## AC

1. LinkedIn Connections → RDF (all 2,104)
2. Facebook friends → RDF (all 334)
3. Cross-source dedup applied
4. ImportedContacts merged where name-matched
5. Searchable via /search (collection: people)
6. Browse page shows people with network badge, company, connected date
7. Harvest manifest updated for people domain

## Size

Medium. The harvest pattern exists from #443 — this is a new domain using the same machinery.
