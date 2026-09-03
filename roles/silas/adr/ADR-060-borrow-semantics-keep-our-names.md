# ADR-060: Adopt industry ops models by semantics, under one Chorus vocabulary

**Status:** Proposed — drafted 2026-09-03 (Jeff: "are there any ops models that cover our core domains that we may just adopt rather than build" / "is that an adr"), lands with #4084 (logs writer), the first class it governs
**Author:** Silas (OWL-DBA)
**Builds on:** ADR-025 (ontology vs instances), ADR-027 (derived mappings live in the graph), ADR-028 (substrate class contract), ADR-059 (vocabulary semver)

## Context

Five borg domains (logs, alerts, monitors, infrastructure, deploys) have no shaped, served, populated class today; every domain-page fold they should fill reads a pre-graph hand list. Rather than invent five models, the industry already has one per domain: OpenTelemetry semantic conventions (logs, resources), the Prometheus/Alertmanager rule model (alerts), OpenSLO (monitors), DORA (deploys), Backstage catalog kinds (code), CloudEvents (events).

Adopting them whole means six vocabularies with six naming styles: `service.name` (dotted), `spec.owner` (YAML path), `labels.severity`, `objectives[].target`. Every page, query and writer would carry the translation, and "service" and "host" would exist under four spellings. That is the complexity Jeff asked about, and it is real.

## Decision

1. **One vocabulary, one name per concept.** Every class and property is `chorus:` and follows the existing naming (camelCase properties, PascalCase classes). No property is named after a standard's path.
2. **Borrow the semantics, record the source.** A property adopted from a standard carries `chorus:adoptedFrom` with the standard's term as a literal (for example `chorus:logPath` → `"otel:log.file.path"`, `chorus:severity` → `"alertmanager:labels.severity"`). Meaning, cardinality and value set come from the standard; the name is ours. Import and export are a rename, never a remodel.
3. **One backbone shared by all borg classes.** The OpenTelemetry *resource* (service, instance, host, site, environment) is modeled once as properties on `ServiceInstance` and `Machine` (#3870, #4086). LogSource, Monitor, Alert and Deploy point at those rows; they never repeat host or service fields.
4. **Derivations are queries, not nouns.** DORA's four keys and CloudEvents' envelope add no classes. DORA is a query over Deploy rows and spine events; the envelope is ADR-024.
5. **The domain edge is authored once.** Per ADR-027, the unit → domain mapping lives in the graph as rows; the crawler, log writer, alert writer and service harvester all read it. No writer carries its own mapping.

## Consequences

- Each of #4084–#4088 ships a shape whose properties cite their `adoptedFrom` term; a shape with a borrowed name and no citation fails `athena-validate` (negative proof shipped with #4084).
- A reader who knows OTel or Alertmanager can find the term in one hop; a reader of our pages sees one vocabulary.
- Semver (ADR-059) applies: adopting a standard's field is a minor bump; renaming an existing property to match a standard is forbidden (it would be a major for no meaning change).
- Versions of the standards are pinned in the ADR appendix when the first shape lands, so "conformant" is checkable, not felt.

## Not decided here

Which of the standards' optional fields we take per class (each card decides against its fixture, security). Whether Backstage kinds replace our Product/Service/Domain (they do not; ADR-020 stands).
