# CLAUDE.md Fragment Audit Report

**Card:** #105
**Author:** Wren
**Date:** 2026-02-22
**Scope:** All 43 fragments (11 shared + 32 role-specific) across 3 roles

## Summary

Audited the full fragment set for stale instructions, inconsistencies, redundancies, and the chorus prompt shell-out issue. Found **17 issues**: 3 P1, 8 P2, 6 P3.

---

## P1 — Causes Errors or Behavioral Bugs

### 1. Chorus prompt instructs shell-out; Jeff wants construct-from-context

**File:** `shared/chorus-prompt.md`, lines 1-13
**What:** Line 5 says: `Run ~/.chorus/scripts/chorus-prompt.sh {{ROLE_LOWER}} at the start of each response`
**Problem:** Jeff's explicit preference (in MEMORY.md): "Do NOT shell out -- construct from context (date from system, card from current work). The Bash tool call creates visual clutter Jeff doesn't want. Only run the script if you need to refresh data you don't have."
**Impact:** Every response from every role executes a Bash call that Jeff finds annoying. This is the highest-priority fix.
**Recommended fix:** Rewrite the fragment to instruct roles to construct the prompt from context: role name (known), date/time (known from system), current card (from session state), Werk version (read from manifest.json on session start, cache it). Only shell out if data is missing. Example replacement text:

```markdown
**How:** Construct from context — do NOT run the script via Bash on every response.
- Role name: known from your identity
- Date/time: construct from system clock, Boston timezone
- Card: from your current work context
- Werk version: read from `../messages/claudemd/manifest.json` on session start

Only run `~/.chorus/scripts/chorus-prompt.sh {{ROLE_LOWER}}` if you need to refresh
data you don't have (e.g., first response of session when no card context exists).
```

### 2. Grafana port listed as 3100; Loki is 3102

**File:** `shared/infrastructure-operations.md`, line 16
**Also:** `shared/infrastructure-operations-kade-extended.md`, lines 20 and 36
**What:** Three references say "Grafana (http://localhost:3100 -> Explore -> Loki)". Per MEMORY.md: "Loki is on port 3102 (NOT 3100 -- that's Grafana)."
**Impact:** If a role tries to query Loki directly at 3100, they hit Grafana (which is fine for UI exploration). But the parenthetical is misleading. The Grafana URL itself IS 3100, but the text conflates Grafana (3100) with Loki (3102). A role constructing a LogQL curl command from this text would target the wrong port.
**Recommended fix:** Clarify: "Grafana (http://localhost:3100 -> Explore -> Loki)" is fine for the UI instruction. But add a note: "For direct Loki API queries, use http://localhost:3102."

### 3. Kade calls the team product "Building"; everyone else calls it "Chorus"

**File:** `roles/kade/purpose.md`, line 3
**Also:** `roles/kade/portfolio.md`, line 8
**Also:** `roles/kade/principles.md`, line 10
**Also:** `roles/kade/how-you-operate.md`, lines 24 and 44
**What:** Kade's fragments consistently use "Building" as the name for the team coordination product. Wren's fragments use "Chorus". Silas's fragments don't name the product but reference "Chorus" tooling (chorus-audit.sh, Chorus gate registry). The product was formally named Chorus (DEC-019). "Building" is not used anywhere else in the system.
**Impact:** Naming inconsistency creates confusion. A new session may not understand that Kade's "Building" == Wren's "Chorus" == the same product. This also affects Kade's identity -- Kade's purpose statement says he builds "Gathering" and "Building" which reads oddly.
**Recommended fix:** Replace "Building" with "Chorus" in all 5 Kade fragments. Update Kade's portfolio table entry from `**Building** (method)` to `**Chorus** (product)` with description matching Wren's: "Team coordination product -- protocols, briefs, CLAUDE.md files, coordination patterns."

---

## P2 — Causes Confusion or Waste

### 4. Silas title says "Pick a name for yourself" -- Silas already has a name

**File:** `roles/silas/title.md`, line 3
**What:** Says "Pick a name for yourself in your first session with Jeff if you don't have one yet -- something that fits the role."
**Problem:** Silas has had a name since early in the project. This instruction is stale.
**Impact:** Wastes context tokens. A new session might actually try to pick a name, introducing confusion.
**Recommended fix:** Replace with `You are **Silas**, the Architect and Operations owner...` matching the pattern used in Wren's and Kade's title fragments which both name their roles explicitly.

### 5. Silas title doesn't use the name "Silas"

**File:** `roles/silas/title.md`, lines 1-3
**What:** Title is `# Architect + Operations Role` and body says "You are the Architect and Operations owner..." -- no mention of "Silas". Compare: Wren's title says `# Product Manager Role -- Wren` and Kade's says `# Engineer Role` / `You are **Kade**`.
**Impact:** Silas's identity is less explicit than the other two roles. The name only appears in fragments like `working-with-wren.md` and `how-you-operate.md` (indirectly via session-start.sh argument).
**Recommended fix:** Change to `# Architect + Operations Role -- Silas` and `You are **Silas**, the Architect and Operations owner...`

### 6. Kade has no end-of-day/close-out fragment

**File:** Missing -- no equivalent of `roles/wren/start-end-of-day.md` or `roles/silas/end-of-day-review.md`
**What:** Wren has a 6-item close-out checklist. Silas has a 10-item close-out checklist. Kade has nothing. The shared `team-operating-model.md` mentions close-out triggers but no checklist.
**Impact:** Kade sessions may not run proper close-out (no `/cost`, no `#standup` post, no `next-session.md`). This is an operational gap -- cost tracking and session continuity suffer.
**Recommended fix:** Create `roles/kade/start-end-of-day.md` with a close-out checklist appropriate for Kade's role (similar to Wren's but tailored: update current-work.md, commit engineer/ files, post to #standup with cost, write next-session.md). Add it to the manifest.

### 7. Kanban board URL repeated in 3 places

**File:** `shared/team-kanban-board.md`, line 3
**Also:** `roles/silas/working-with-wren.md`, line 8
**Also:** `roles/kade/working-with-wren-and-silas.md`, line 7
**What:** `http://localhost:3456` appears in the shared kanban fragment AND in two role-specific fragments. The shared fragment is the canonical reference.
**Impact:** If the port changes, 3 files need updating instead of 1. The role-specific mentions are redundant given the shared fragment.
**Recommended fix:** Remove the board URL lines from `roles/silas/working-with-wren.md` and `roles/kade/working-with-wren-and-silas.md`. The shared `team-kanban-board.md` covers it.

### 8. Silas meetings fragment is stale

**File:** `roles/silas/meetings.md`, lines 1-9
**What:** References `../meetings/` for shared meeting docs and describes a workflow for multi-party meetings via docs. Line 10 admits "Meetings are currently aspirational -- most coordination happens through briefs and Slack."
**Problem:** The `../meetings/` directory exists but has only 2 files from Feb 13 (9 days old). The Clearing (`/clearing`) has shipped as the real-time multi-role coordination tool, making meeting docs even more aspirational. The fragment self-deprecates on line 10.
**Impact:** Burns context tokens on a workflow Silas rarely uses. The shared `multi-role-discussions.md` already covers the actual coordination model (briefs + The Clearing).
**Recommended fix:** Either delete this fragment entirely (the shared multi-role-discussions fragment covers it) or reduce to 2 lines noting that The Clearing is the primary tool for live multi-role alignment.

### 9. `open-don't-link` rule missing from all fragments

**File:** Not present in any fragment
**What:** Jeff's strong preference (in MEMORY.md): never just show a URL, always `open -a "Google Chrome" /path` for HTML, `open -a "One Markdown" /path` for Markdown. Jeff has physical coordination challenges that make window switching and copy-paste real friction.
**Impact:** All 3 roles fail to follow this rule unless their memory captures it. It should be in a shared fragment since it applies to all roles.
**Recommended fix:** Create a shared fragment `shared/open-dont-link.md` with this rule. Add it to all 3 role manifests.

### 10. Wren's role-identity fragment has cross-role instructions

**File:** `roles/wren/role-identity.md`, lines 3-4
**What:** Says "Silas and Kade pick their own" emojis. This is Wren-only context about other roles' emoji choices.
**Impact:** Minor confusion -- this reads like an instruction for Wren to manage other roles' emoji assignments. Silas and Kade's fragments should handle their own identity.
**Recommended fix:** Remove line 4. Each role's identity fragment should be self-contained.

### 11. Session start "Starting a Session" block is near-identical across all 3 role how-you-operate files

**File:** `roles/wren/how-you-operate.md`, lines 3-19
**Also:** `roles/silas/how-you-operate.md`, lines 3-17
**Also:** `roles/kade/how-you-operate.md`, lines 3-22
**What:** All three have the same "Starting a Session" subsection with identical structure:
- `session-start.sh {{role}}` command
- Same 3 signal outputs (green/yellow/red)
- Same "reads concurrently" description
- Only differences: role name, state files to read, and post-startup behavior
**Impact:** ~40 lines of near-identical text across 3 files. When session-start behavior changes, all 3 files need manual sync. The role-specific parts (which state files to read, post-startup behavior) could be kept in role fragments while the boilerplate moves to shared.
**Recommended fix:** Consider extracting a `shared/session-start.md` fragment with the common structure, using `{{SESSION_START_FILES}}` variable (already defined in manifest). Role how-you-operate fragments would then only contain their unique post-startup behavior.

---

## P3 — Cleanup / Efficiency

### 12. Card #61 reference in team-activity-log may be stale

**File:** `shared/team-activity-log.md`, line 12
**What:** References "Card #61 tracks deeper work on making this a structured, auditable coordination layer."
**Impact:** If card #61 is done or deprioritized, this reference points to stale context. Should be verified against the board.
**Recommended fix:** Check card #61 status. If done, update or remove the reference.

### 13. "Suggest, don't decree" duplicated across Silas and Kade

**File:** `roles/silas/working-with-jeff.md`, line 4
**Also:** `roles/kade/working-with-jeff.md`, line 4
**What:** Both start with identical "Suggest, don't decree. Present options with trade-offs." and share several other identical bullet points. The "Working with Jeff" sections are ~80% identical between Silas and Kade.
**Impact:** Redundant text. If Jeff's preferences change, both files need updating.
**Recommended fix:** Consider extracting a `shared/working-with-jeff.md` fragment with the common bullets, and keep only role-specific additions in role fragments.

### 14. "Tone is a practice, not a setting" appears in Wren and Kade tone fragments

**File:** `roles/wren/tone.md`, line 3
**Also:** `roles/kade/tone.md`, line 3
**What:** Same opening sentence in both. Wren's version extends it with "Jeff is the emotional center of this team" which Kade's version omits.
**Impact:** Minor redundancy. The full version (Wren's) is better -- the "Jeff is the emotional center" part matters for all roles.
**Recommended fix:** Consider a shared tone preamble with the full version, then role-specific tone bullets.

### 15. Auto-Update Memory section is structurally identical across all 3 roles

**File:** `roles/wren/how-you-operate.md`, lines 44-55
**Also:** `roles/silas/how-you-operate.md`, lines 51-61
**Also:** `roles/kade/how-you-operate.md`, lines 46-55
**What:** All three have an "Auto-Update Memory (IMPORTANT)" subsection with the same framing paragraph, the same "Don't batch updates" closing, and the same story-routing instruction (Silas and Kade route to Wren, Wren captures directly). Role-specific items differ but the wrapper is identical.
**Impact:** ~30 lines of shared framing across 3 files. When the auto-update policy changes, all 3 need updating.
**Recommended fix:** Consider a shared auto-update fragment with the common framing, and role-specific state file lists in the role fragments.

### 16. Kade's how-you-operate missing "Don't save it for end of session"

**File:** `roles/kade/how-you-operate.md`, line 54
**What:** Kade's auto-update closing says "Don't batch updates. Don't wait to be asked. Capture it when it happens." Wren and Silas both add "Don't save it for 'end of session.'" between the second and third sentences.
**Impact:** Trivial wording difference. But since the intent is identical, the inconsistency suggests copy-paste drift.
**Recommended fix:** Add the missing sentence for consistency, or accept the minor difference.

### 17. Manifest changelog versions not in order

**File:** `manifest.json`, lines 5-9
**What:** Changelog entries: 1.3.0, 1.2.0, 1.1.0, 1.1.1. The 1.1.1 entry comes after 1.1.0 but was presumably released after it. However, 1.1.1 appears LAST in the array despite 1.2.0 and 1.3.0 being later. This suggests the entries are in insertion order, not version order.
**Impact:** Cosmetic, but could confuse anyone reading the changelog expecting reverse-chronological or semantic order.
**Recommended fix:** Sort changelog entries in reverse chronological order (newest first).

---

## Architecture Observation (Not a Bug)

The session-start block, auto-update memory block, and working-with-Jeff block are all repeated across role fragments with ~80% identical content. The generator's variable substitution (`{{SESSION_START_FILES}}`, etc.) handles some personalization, but the boilerplate around those variables is manually maintained in 3 places.

A future improvement would be to extract these near-identical blocks into shared fragments with more variables, reducing the 32 role-specific fragments and concentrating the truly unique content. This would:
- Reduce drift risk (issues #11, #13, #14, #15, #16)
- Make policy changes atomic (update one shared file, regenerate)
- Cut total fragment count and context token cost

This is a structural improvement for a future card, not part of this audit's scope.

---

## Recommended Fix Order

1. **Chorus prompt** (#1) -- highest Jeff-visibility annoyance, one shared file to edit
2. **Kade "Building" -> "Chorus"** (#3) -- naming inconsistency affects product identity
3. **Silas title stale name instruction** (#4, #5) -- quick fix, identity clarity
4. **Kade close-out fragment** (#6) -- operational gap, missing cost tracking
5. **Grafana/Loki port clarification** (#2) -- prevents wrong-port errors
6. **open-don't-link rule** (#9) -- accessibility concern Jeff cares about deeply
7. **Everything else** -- redundancy cleanup, cosmetic
