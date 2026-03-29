---
from: kade
date: 2026-03-26
card: 1641
type: request
priority: P1 — demo tonight 6pm
---

# NiFi flow for webMethods package ingest — demo tonight

Silas — #1641 demo is tonight at 6pm. External guests (Deb, Allu, Kathy — Software AG/webMethods background). Jeff wants the webMethods reverse-engineering driven via ICD + NiFi, not throwaway scripts.

## What I've done

Wrote `icd-instance-webmethods.ttl` at:
`jeff-bridwell-personal-site/src/ontology/icd-instance-webmethods.ttl`

It defines the domain with 3 providers:
1. **MU118_Canonicals_v1** — 161 doc types, 18 BODs, 722 unique fields, OAGIS canonical model
2. **MU118_EDI** (4 packages) — 216 doc types, 187 flows, X12/UCS EDI processing
3. **MU118_LENG + common** (10 packages) — 890 doc types, 647 flows, logistics engine

## What's needed from you

A NiFi flow that:
1. **Reads node.ndf files** from `/tmp/wm-packages/extracted/` — walks the ns/ tree
2. **Parses IData XML** — extracts field_name, field_type, field_dim, rec_ref from rec_fields arrays
3. **Produces field inventory** — per-package, per-doc-type field list with types and cross-references
4. **Reads flow.xml files** — extracts INVOKE SERVICE calls (call graph) and MAPCOPY FROM/TO (field mappings)
5. **Outputs to Fuseki** or JSON — structured enough to render on convergence page

The ICD TTL needs to be loaded to Fuseki so it renders on `/harvesting/convergence`.

## Package locations

- Extracted: `/tmp/wm-packages/extracted/` (20 packages)
- Zips: `/tmp/wm-packages/wM_Packages/` (originals, May 2008)

## Demo story

Agent team receives unknown source (webMethods packages), reverse-engineers the schema via ICD discipline, ingests via NiFi. The guests built these packages — they'll recognize the canonical model, the EDI segments, the LENG logging. Jeff's Staples ICD pattern applied to their own platform.

## Demo test: end-to-end

The demo is: send an EDI 875 to an endpoint, show it land in Fuseki as RDF.

**Test message:** `/tmp/wm-packages/test-edi-875.edi` — 3-line grocery PO (Band-Aid, Tylenol, Listerine) from TRADINGPTR001 to JNJMU118. Real UCS 4010 format with G50/N1/N9/G68/G69/G72/G76/G23/G62/NTE/CTT segments.

## NiFi processor chain

1. **ListenHTTP** (port 8875) — receive EDI 875 at endpoint
2. **ParseEDI** (nifi-edi-nar) — parse X12/UCS segments, segment-aware
3. **JoltTransformJSON** — map parsed fields to canonical using ICD mappings (H01-PO-NO → SalesOrder/Header/DocumentId, L01-QTY-ORD → Line/Quantity, etc.)
4. **ConvertRecord** (or script) — JSON → Turtle RDF using ICD field definitions
5. **InvokeHTTP** — POST to Fuseki GSP (`http://localhost:3030/pods/data?graph=urn:gathering:webmethods/orders`)

## Non-functionals (now in ICD)

- Atomic parse: SEQUENCE EXIT-ON=FAILURE pattern → in NiFi this is provenance + back-pressure
- Dedup keys: H01-INT-CNTL-NO + H01-TRN-CNTL-NO → FlowFile attributes for dedup
- LENG logging → NiFi provenance tracking

## Verification query (run after ingest)

```sparql
SELECT ?po ?date ?product ?qty WHERE {
  GRAPH <urn:gathering:webmethods/orders> {
    ?order a jb:SalesOrder ;
           jb:poNumber ?po ;
           jb:poDate ?date .
    ?order jb:hasLine ?line .
    ?line jb:productDescription ?product ;
          jb:quantity ?qty .
  }
}
```

Expected: PO-2008-04153, 3 lines (Band-Aid 24 CA, Tylenol 48 CA, Listerine 12 EA).

## Loki observability (demo requirement)

Each NiFi processor must emit a structured JSON log line with business context. See `/tmp/wm-packages/demo-loki-queries.md` for exact format. Key fields on every event: `po_number`, `sender_id`, `receiver_id`, `stage`, `event_type`. NiFi → Loki via existing Loki push endpoint (localhost:3102). Demo shows Loki query by PO number alongside convergence page — two views of one transaction.

## Response needed

Wire this NiFi flow. If ParseEDI nar isn't installed, a simpler path: ListenHTTP → ExecuteScript (parse EDI with Groovy/Python regex — the format is delimiter-based, not complex) → ConvertRecord → InvokeHTTP. The key is: message in, RDF out, governed by ICD.
