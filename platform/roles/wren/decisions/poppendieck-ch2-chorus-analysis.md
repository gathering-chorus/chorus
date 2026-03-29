# Poppendieck Chapter 2: Technical Excellence — Applied to Chorus

**Source:** Mary & Tom Poppendieck, *Leading Lean Software Development: Results Are Not the Point*, Chapter 2
**Analysis by:** Wren | 2026-03-23
**Context:** Jeff shared 57 photos of the chapter pages for deep reading during a session focused on photos data quality, golden source analysis, and the board-ts redesign.

---

## The Plank Roads Warning (p.46)

The Poppendiecks open with Surowiecki's **information cascade** — plank roads succeeded initially, so everyone copied the approach without evaluating alternatives. By the time the planks rotted, the investment was unrecoverable.

**Chorus parallel:** We did this with Google Takeout as canonical anchor. It had the most records, the first pipeline worked, and every subsequent card built on that assumption. The 62K canonical looked like success until we asked "how many photos does Jeff actually have?" and couldn't answer. The information cascade was: Takeout works → build reconciliation on Takeout → build enrichment on reconciliation → build embeddings on enrichment. Each layer reinforced the layer below without going back to source.

DEC-106 is the counter-pattern: break the cascade by going back to source before each layer.

---

## Structured Programming → What Happened to It (pp.48-50)

Dijkstra and Mills proved that incremental development with verification at each step produces correct programs. But the industry adopted the surface practice (eliminate GOTOs, use functions) and missed the principle (verify correctness at each step). Structured programming became waterfall — the opposite of what Dijkstra intended.

**Chorus parallel:** Our ICD discipline has the same risk. The principle is "understand your source before building on it." The surface practice is "write a TTL file." If the roles start treating ICD as a checkbox (write the file, skip the verification), we'll get plank road ICDs — they exist but they don't match reality. The golden source analysis (#1633) is the verification step that keeps the ICD honest.

---

## Separation of Design from Implementation (p.55)

Dijkstra argued that correctness must be built in during design, not verified after implementation. "Whether the correctness of a piece of software can be established or not depends greatly on the structure of the thing made."

**Chorus parallel:** This is exactly what Jeff identified about data quality. We can't verify correctness of the canonical after loading it — the structure has to be right before the first record goes in. DEC-106 step 4 (decide the anchor) is design. Steps 5-6 (record in ICD, then build) is implementation. Separating them prevents the "load first, discover gaps later" anti-pattern.

---

## Life Cycle Considered Harmful (p.56)

Royce's original 1970 paper presented the waterfall as an example of a process that **does not work**. It was adopted as best practice by people who read only the first diagram and not the rest of the paper. The Poppendiecks list two fundamental problems: (1) it presumes requirements can be fully specified before development, (2) it separates the people who specify from the people who build.

**Chorus parallel:** We don't do waterfall, but we do separate specification from execution. Wren writes the spec (verb model, ICD requirements), Silas/Kade build. The risk is the same: the spec doesn't survive contact with reality. The mitigation is the evolutionary cycle — Kade discovers the Takeout count is wrong, we update the spec, the chain adjusts. The card sequencing (#1636) makes this explicit: when reality invalidates a card, the chain shows the gap.

---

## Evolutionary Development (pp.56-58)

The Poppendiecks describe how internet development in the 1980s-90s happened outside the corporate lifecycle — small teams, rapid iteration, no formal process. ARPANET succeeded because of: (1) technical vision, (2) robustness through independent components, (3) emergent standards. The corporate world wasn't paying attention.

**Chorus parallel:** This is Chorus itself. Three AI roles, rapid iteration, no formal process in the enterprise sense. Jeff watches and steers, the roles build and ship. The "one hot session" insight from today is the lean version — don't parallelize for utilization, serialize for flow. The ARPANET lesson applies: robustness comes from independent components (roles with vertical ownership), not from centralized control.

---

## Essential Complexity / No Silver Bullet (p.63)

Brooks: the complexity of software is an essential property, not an accidental one. No tool or technique will produce an order-of-magnitude improvement because most of the complexity is inherent in the problem, not in the implementation.

**Chorus parallel:** The photos golden source problem isn't a tooling problem. Better pipelines, faster queries, richer embeddings — none of them address the essential complexity: Jeff has 55K+ photos across three devices spanning 20 years, with no single authoritative record. That complexity is in the domain, not in our code. DEC-106 acknowledges this — the six steps exist because the problem is genuinely hard, not because our tools are inadequate.

---

## Dependency Architecture / Low-Dependency Architecture (pp.64-65)

Parnas 1972: decompose by what's likely to change, not by processing flow. The internet succeeded because of low-dependency design — black boxes (routers) that could be replaced independently. Conway's Law: systems reflect the communication structures of the organizations that build them.

**Chorus parallel:** Our role vertical ownership (DEC-058) is Parnas applied to team structure. Wren owns product, Silas owns ops, Kade owns app. Each can change independently. Conway's Law predicts our system architecture matches this — and it does. The board-ts redesign (#1634-#1638) was successful because it was one role's vertical scope (Silas), touching well-defined interfaces. The photos canonical problem is harder because it spans all three roles' domains.

---

## "Our Architecture Is Too Complicated for Cross-Functional Teams" (p.66)

A story about a company with 8 product teams and 7 architecture layers. Each team owned a layer. Cross-functional work was "impossible" because the architecture enforced the silos. The Poppendiecks argue: change the architecture, not the team structure.

**Chorus parallel:** When we couldn't reconcile photos across sources, the instinct was "assign it to Kade." But the problem isn't in Kade's domain alone — it spans sources (Silas's ops territory), schema (the ICD), and product decisions (Wren's call on what "canonical" means). The card chain (#1633 → #1642 → #1619 → #1620) makes this cross-domain dependency explicit. The architecture of the solution matches the architecture of the problem, not the architecture of the team.

---

## Quality by Construction (pp.70-76)

TDD, continuous integration, testing pyramid, "Every Few Minutes" cadence. The key insight: find defects before they propagate. Fagan's data showed that most defects originate in the first half of development but are found in the second half. The cost of finding them late is 10-100x the cost of finding them early.

**Chorus parallel:** Our data quality problem is the same pattern at the data layer. Errors entered in the bronze layer (wrong source count, wrong anchor assumption) propagated through silver (reconciliation) and gold (embeddings, search). Finding them now — after 62K records are loaded — costs far more than finding them at ingest. DEC-106 is quality by construction for data: verify at source, not after loading.

---

## "How Often Is 'Continuously'?" (p.76)

Every few minutes for unit tests. Every commit for integration. Every iteration for acceptance. Every release for stress/performance. The cadence matters — tests that run daily catch problems daily, tests that run weekly catch them weekly.

**Chorus parallel:** We don't have data quality tests at any cadence. The ICD consistency test exists but doesn't run automatically. #1620 (revalidation workflow) addresses this — flag stale instances when schema changes. But the deeper need is continuous data quality verification, not just schema verification. "How many photos does Jeff have?" should be answerable from a test that runs on a schedule, not from a 20-minute investigation.

---

## Evolutionary Development / Cycles of Discovery (pp.83-88)

The three-phase cycle: **Understand It** (ethnography) → **What Might Work** (collaborative modeling) → **Try It** (quick experiments). Repeat. "Follow Me Home" — watch actual users, don't ask them what they want. Set-based development: try multiple approaches, converge when you have data.

**Chorus parallel:** Today's session was a pure cycle of discovery. We started with "let's do data quality on photos." Tried the scorecard approach. Discovered the Takeout count was wrong. Pivoted to normalized exports. Discovered Apple Photos is pruned. Pivoted to include iPhone. Each iteration narrowed the solution space. This IS evolutionary development — the Poppendiecks would recognize it.

The "Follow Me Home" pattern is literally what Jeff does with `/gemba`. He watches the role work, sees what they actually do (not what they report), and steers based on observation. The Poppendiecks wrote about this for customers — Jeff applies it to his AI team.

---

## Deep Expertise / Expertise Is Important (pp.89-91)

Brooks: "Software construction is a creative process." Developers are authors, not translators. The difference between good and great developers is 10:1. Expertise matters more than process.

The Dreyfus model: Novice → Advanced Beginner → Competent → Proficient → Expert. Experts work from intuition and pattern recognition, not from rules. Deliberate practice: identify a skill, practice repeatedly, get immediate feedback, concentrate on results.

**Chorus parallel:** Jeff's data integration expertise — Dallas Systems → Staples → Gathering — is the Dreyfus model in action. He moved from rule-following (badly documented specs) to pattern recognition (ICD discipline) to intuition ("I don't trust 129K, that doesn't fit my sense of scope"). When Jeff said "this number doesn't feel right," that was expert intuition catching what the pipeline missed. The roles don't have this — they follow rules and process. Jeff's domain expertise is irreplaceable signal.

---

## The Competency Leader Portrait (pp.96-97)

Jeff Sutherland asked his team: "The most successful parts of your architecture have been in place a long time." The competency leader's job: make the important decisions correctly, based on deep knowledge of the business. Create opportunities for deliberate practice. Provide fast feedback.

**Chorus parallel:** Jeff is the competency leader. The team's technical decisions are correct when Jeff steers them (DEC-106 came from Jeff questioning the data, not from the roles finding a bug). The deliberate practice loop is: Jeff observes via gemba → gives feedback → roles adjust → Jeff observes again. The speed of that loop determines the quality of the system.

---

## "Oh, You Mean the Maestro?" (p.97)

A music analogy: the maestro doesn't play every instrument — the maestro listens to the orchestra and shapes the performance. The musicians are experts at their instruments. The maestro's contribution is hearing how the parts fit together.

**Chorus parallel:** That's Jeff's role exactly. He doesn't write code or run SPARQL queries. He hears when the numbers don't fit — "I have 55K photos on my phone, why does the graph show 62K?" That's the maestro hearing a wrong note. The roles are the instrumentalists. The system works when Jeff is conducting, and drifts when he's not.

---

## Summary

The deepest connection: the Poppendiecks' entire argument is that practices become fads when people adopt them without understanding the underlying principles. Our data pipeline became a plank road because we adopted "ingest and reconcile" without understanding the source. DEC-106 is the return to principles.

Jeff's observation that "plank roads are a little like AI" extends this further — the AI information cascade is happening industry-wide right now. Everyone is building on LLMs because everyone else is, without understanding what makes the output trustworthy. Chorus's ICD discipline, golden source analysis, and source-chain provenance are the counter-pattern: don't trust the output until you've verified it against source truth.
