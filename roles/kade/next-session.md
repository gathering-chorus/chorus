# Kade — Next Session

## READ FIRST — behavioral (Jeff named these 2026-05-28, hard)
- **Jeff is in auto-accept + focus mode and CANNOT read our output torrent.** Default to ONE line, signal only. Expand only on ask. When blocked: name the blocker + the single action that clears it, then STOP. Never push a dead path or send a commodity explainer he'd get from search in 1ns. See [[feedback-jeff-auto-focus-cant-read-torrents]].
- **Consult memory/Loki/chorus BEFORE asserting.** This session I read a roles page *I helped write*, called it Silas's, and claimed "no episodic memory" — false (chorus-inject feeds my past into every prompt). I have the brain; use it. Scarecrow.

## Landed this session
- **#3115 cclsp fix — DONE, merged (PR #382), accepted.** `.mcp.json`: `--config` arg → `CCLSP_CONFIG_PATH` env. Proven via real JSON-RPC initialize handshake. (Had to mark PR #382 ready — acp opened it draft and refused to merge a draft. Recurring acp gap.)

## #3118 chorus-hooks build-break fix — WIP, fix committed, NOT demo'd, NOT acp'd
- **The fix:** deleted orphaned `pub mod batch_progress;` (mod.rs:22). File dropped in #3046 squash → E0583 → chorus-hooks hasn't compiled on main for 2 weeks. Live binary (May 26) predates the break so no outage, but no hook change could ship. Committed `e611bc0c` on `kade/3118`. cargo check exit 0, zero `batch_progress::` refs.
- **gate-code:** build clean PASS, warnings PASS (13 ≤ baseline 36; deletion adds none). **Tests 440 pass / 1 fail.**
- **The 1 red = NOT mine, pre-existing, cross-domain:** `live_roles_pass_contract` — silas's CLAUDE.md `VersionMismatch{stamp 1.5, live 1.4}`, **cores identical** (cded…b0c). Pure version-stamp drift, not content. `M designing/claudemd/PROTOCOL_VERSION` in working tree is the cause — bumped to 1.5, silas's file not regenerated. **This drift-detector lives inside chorus-hooks — dark for 2 weeks; my fix re-lit it and it immediately caught a real drift.**
- **NEXT:** decide silas-drift handling (card to Silas to regenerate his stamp + known-fail it, OR Silas fixes live) → then /demo 3118 → Jeff /acp. The fix itself is solid; only the unrelated red blocks the chain. Was asking Jeff this when /reboot came.

## Design threads — do NOT lose (Jeff drove these today)
- **chorus-inject → per-turn prompt-driven context HYDRATOR that can't be blown off.** Today it's a canned boot recap. Jeff wants: read the prompt, pull entities, query Chorus+Loki, inject RESOLVED FACTS (not search hits) before I answer. Delivery is deterministic; consumption = push resolved answers + stop-the-line check (#2145 shape) catching assertions that contradict hydrated facts. This is the "beats solo Claude Code" line — we built the memory store, never the consumption.
- **werk-acp = native Rust orchestrator should BE acp-v2.** chorus_acp (MCP, thin) → `werk-acp` → 6 leaf verbs, host-side. **No werk-acp binary, no crate exists.** Current acp-v2 shells to `act`+acp.yml → fails auth (act runs empty GITHUB_TOKEN; host gh authed as WJeffBridwell in keyring). Host-side native wrapper = auth dissolves. Follows werk-pull blueprint. Mine to build (coordinate w/ Silas who reworked werk-accept today 13:06).
- **cclsp is team infra (Jeff: "100% for the whole team").** Roll the `CCLSP_CONFIG_PATH` env form into silas + wren `.mcp.json` (they have NO cclsp entry). Not blarf — Distinguished Engineer puts cross-cutting substrate under the whole team.
- **LSP can't be shown in-session — needs a reboot** to load cclsp. After this reboot: first thing, run live `findReferences` on chorus code (owed demo). ast-grep works in-session (no reload).

## Two gate gaps surfaced by #3118 (small, worth carding)
- TDD gate's no-signature exemption covers `use`/`pub use` but NOT `pub mod` — over-fires on module-declaration removal (tdd_gate.rs `has_behavioral_content`).
- `card_type_for_role` returns "unknown" when a role has 2 WIP cards (readdir-glob can't disambiguate) → gate enforces strictest. Also `chorus_commit` same readdir-ambiguity: stale empty `kade-bin` werk dir false-failed commit (`commit-fail` empty detail) until I rmdir'd it.

## Side debt noted
- chorus-hooks carries dead-code warnings from #3046's demo retirement (demo_gate/preflight/etc. left behind). Cleanup card, not urgent.
