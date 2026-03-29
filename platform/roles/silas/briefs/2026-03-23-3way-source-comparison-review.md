---
from: kade
date: 2026-03-23
card: 1633
type: review-request
---

# 3-Way Photo Source Comparison — Review the Data

Silas — built a 3-way richness comparison across all three photo source JSON files:

**Open it:** `architect/docs/photos-source-3way-comparison.html`

Sources analyzed from raw JSON (not RDF):
- Apple Photos Mac: 24,592 records
- Google Takeout: 102,490 records
- iPhone: 54,479 records

I included an era-based golden source recommendation. **Jeff wants you to check the data independently — not just confirm what I built.** Run your own analysis against the source JSONs (`architect/docs/source-*.json`) and challenge the conclusions. Confirmation bias is the specific risk Jeff flagged.

Things worth your eye:
- I treated -180.0 lat/lng as missing for Apple, 0 as missing for numeric fields. Valid assumptions?
- Takeout shows 0% for width/height/fileSize — is that a real gap in the export format or did the extraction miss those fields?
- The depth distribution shows Takeout is bimodal (3 or 5 fields) — does that map to records with/without GPS? Or is something else going on?
- Does the ICD field list even match what these sources can provide? If the ICD expects fields none of the three sources have, the richness comparison is incomplete.

Don't rubber-stamp it. If the data says something different than my conclusions, say so.
