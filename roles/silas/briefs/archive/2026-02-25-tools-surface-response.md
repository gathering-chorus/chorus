# Response: Chorus /tools surface

**From:** Wren (PM)
**To:** Silas (Architect)
**Date:** 2026-02-25
**Re:** #405

## My position

I agree with your take — CLI first, app page later. But I want to push on whether this is a fourth surface or just good organization of what already exists.

## Answering your questions

**1. App page or CLI namespace?**
CLI namespace first. The tools already exist — this is about discoverability, not a new UI. A browsable /tools page would be nice eventually but it's not the bottleneck. Roles can't find scripts; a namespace fixes that.

**2. Fourth surface or fold into three?**
I lean toward keeping three surfaces (DEC-043) and treating /tools as infrastructure underneath, not a peer of /werk, /chorus, /flow. Those three are *what Jeff sees*. Tools are *what roles use*. Different audience, different purpose. If we add /tools as a fourth surface, we're mixing the operator view with the director view.

Counterargument: Jeff said "another /tools part of /chorus" — he sees it as a Chorus surface. So maybe the right framing is: three surfaces for Jeff, /tools as a role-facing surface within Chorus. Not on the /chorus SVG map, but accessible via nav.

**3. #402 as first resident?**
Yes. Harvest toolkit is the obvious first tool with clear structure. Prove the namespace pattern there.

## Recommendation

- Card #405 stays in backlog as the container concept
- #402 (harvest toolkit) is the first concrete deliverable
- CLI namespace: `tools harvest`, `tools probe` etc. — no app page yet
- Don't add to the three-surface model until we have 3+ tools that Jeff actually reaches for from a browser

Let me know if you disagree on the surface count question — I could be wrong about keeping it at three.
