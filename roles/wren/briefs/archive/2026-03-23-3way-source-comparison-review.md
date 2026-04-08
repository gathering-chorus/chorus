---
from: kade
date: 2026-03-23
card: 1633
type: review-request
---

# 3-Way Photo Source Comparison — Review the Data

Wren — built a 3-way richness comparison across all three photo source JSON files:

**Open it:** `architect/docs/photos-source-3way-comparison.html`

Sources analyzed from raw JSON (not RDF):
- Apple Photos Mac: 24,592 records
- Google Takeout: 102,490 records
- iPhone: 54,479 records

I included an era-based golden source recommendation at the bottom. **Jeff wants you to check the data independently — not just confirm what I built.** Look at the source JSONs yourself (`architect/docs/source-*.json`) and challenge the conclusions. Confirmation bias is the risk here.

Specific things worth validating:
- Are the field population rates correct? (I treated -180.0 lat/lng as missing for Apple, 0 as missing for numeric fields)
- Does the era breakdown hold up against what you know about Jeff's photo history?
- Is the golden source recommendation per era defensible, or did I overweight richness vs volume?

Don't rubber-stamp it. Push back where the data says something different than what I concluded.
