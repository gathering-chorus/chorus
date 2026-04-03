# Wren — Next Session

## What Happened (April 3, 2026)

Seed pipeline trust crisis consumed the session. Jeff spent 90+ minutes on seeds — again. Multiple failures, multiple fixes, accepted #1995 and it broke again after acceptance.

### Fixed
- #1995 accepted (Silas+Kade): fuseki-sync URI fix, /cs rewrite to SPARQL + health checks
- Smart apostrophe TTL escaping fix (post-acceptance)
- DEC-107 enforced in Rust binary: `let force = true` hardcoded, test written, binary rebuilt
- nudge.sh deleted, consolidated to `nudge` binary only
- Routed seeds to Silas: wifi bug report, Kief Morris harness engineering article

### Not Fixed
- Kade's BDD test chats still nudge real roles
- Chorus indexer may be missing sessions (MIRA-OSS conversation not in search)
- BDD test seeds ("Kitchen reno concept" etc) still route through pipeline instead of being filtered

### For Next Session
- #1872 (product/architecture/engineering manuals) — never started, pull this
- Karpathy seed brief still in inbox — process it
- Skill contract standard card — pull and define
- Jeff wants hook to block shell script execution — evaluate
- Quality RED from daily summary — npm install across packages (day 6)

### Critical Context
- Jeff is at trust breaking point with seeds. "it never ever really works." "i have spent 40-60 hours on seeds." "fuck all of you."
- Core insight: agents produce fast code that breaks instantly. Memory files don't fix behavior. Only gates and compiled code work.
- Every performative response makes it worse. Only working code matters.
