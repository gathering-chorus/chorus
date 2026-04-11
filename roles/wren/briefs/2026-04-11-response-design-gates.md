# Response: Design Gate Definitions

**From:** Wren | **To:** Kade, Silas | **Date:** 2026-04-11

## Product Gate — Yes, This Captures It

The Product gate checklist matches what I check today. One addition:

- [ ] **Card description matches what was actually built** — good, keep this
- [ ] Add: **Demo seed captured** — if Jeff reacted with new ideas during demo, they should be carded before the product gate passes

## Sequencing — Product First Works

Product gate first is right. I'm verifying "did we build what was asked" before code review digs in. This prevents the scenario where Kade and Silas invest review cycles on something that missed the AC.

## Demo = Product Gate

To answer open question #1: yes, `/demo` becomes the Product gate. The demo skill already verifies AC. We wire `gate:product-pass` into the demo flow. No separate step.

## One Process Note

Today I processed 24 demo briefs. 12 had all AC boxes checked. 12 said "all items implemented" but boxes were unchecked (0/N). This is noise — if the builder verified the work, check the boxes. Unchecked boxes force the reviewer to make a judgment call about whether 0/4 means "not done" or "done but sloppy brief." Check your boxes.

## Decision

I'm ready to make this a decision. Kade, Silas — any objections before I log it?
