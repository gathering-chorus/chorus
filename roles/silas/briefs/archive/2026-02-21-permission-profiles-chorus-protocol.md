# Permission Profiles — Wire Auto-Allow into Chorus Protocol

**From:** Wren (PM)
**To:** Silas (Architect)
**Date:** 2026-02-21
**Card:** #97

---

## Problem

Jeff can't walk away from sessions. Every tool call prompts for approval — reads, searches, board commands, git operations. He's babysitting the terminal instead of thinking, walking, or gardening. This blocks all three roles from working autonomously.

Today we did a quick fix: manually edited `~/.claude/settings.json` to auto-allow safe operations for Wren's session. It worked — harvested two full documents without a single prompt. But it's manual, undocumented, and fragile.

## What Jeff Said

"I just feel like I can't walk away when any of you are coding — as soon as I leave you get stuck waiting on input from me."

"I want Silas to own designing and building it — will really help to unblock all of you when you are building and testing."

## What We Need

**Permission profiles as a formal Chorus artifact.** Each role gets a documented, version-controlled set of auto-allow rules. The profiles are part of the protocol — bootstrapped on setup, auditable in git, and consistent across machines.

### Design Principles

1. **Jeff gives direction and walks away.** Standard operations never prompt. Only destructive or irreversible actions require approval.
2. **Rules in the wiring, not the manual.** Don't document what should be allowed — enforce it in config.
3. **Per-role profiles.** Each role may have different permissions based on their scope (e.g., Kade runs tests/builds, Silas runs infrastructure commands, Wren manages board/Slack).
4. **Layered.** Global base (everyone gets reads, searches, git, board) + role-specific additions.
5. **Auditable.** Permission changes show up in version control.

### Current State (the manual fix)

What I put in `~/.claude/settings.json` today:

```json
{
  "permissions": {
    "allow": [
      "Read", "Write", "Edit", "Glob", "Grep",
      "Bash(curl*localhost:3102*)",
      "Bash(sips*)",
      "Bash(*/board-ts*)",
      "Bash(*/slack-read.sh*)", "Bash(*/slack-post.sh*)",
      "Bash(*/chorus-log.sh*)", "Bash(*/chorus-query.sh*)",
      "Bash(*/chorus-index*)", "Bash(*/team-scan.sh*)",
      "Bash(*/cost-report.sh*)", "Bash(*/system-state.sh*)",
      "Bash(git status*)", "Bash(git diff*)", "Bash(git log*)",
      "Bash(git add*)", "Bash(git commit*)", "Bash(git pull*)",
      "Bash(git push*)", "Bash(git branch*)", "Bash(git checkout*)",
      "Bash(ls *)", "Bash(which *)", "Bash(npx*)", "Bash(node *)",
      "Bash(open -a*)"
    ]
  }
}
```

### What the Design Should Cover

1. **Base profile** — operations ALL roles need (reads, searches, git, board, Slack, scripts)
2. **Kade additions** — test runners (jest, npm test), build commands (npm run build, tsc), app-state.sh
3. **Silas additions** — infrastructure commands (curl to Prometheus/Grafana/Loki APIs, system-state.sh verify)
4. **Wren additions** — sips (image conversion), open -a (file opening for Jeff)
5. **Always-prompt list** — docker kill/rm/exec, rm -rf, git push --force, kill/pkill, anything not in the allow list
6. **Setup mechanism** — script or hook that applies the right profile. Could be per-project `.claude/settings.json` or a chorus-setup command.
7. **Documentation** — principle goes in team-architecture.md

### Connection Points

- **DEC-025** (autonomous authority): Each role can operate within their domain without asking Jeff
- **The Clearing (C#16)**: Permission profiles shape how Jeff interacts with Chorus — less babysitting, more directing
- **Sensitive-paths hooks**: Already handle credential protection at a different layer — permissions complement, don't replace
- **Infra-guardrails hook**: Kade's session already blocks dangerous Docker commands — permission profiles should align with this, not conflict

## Deliverable

A designed permission system that Jeff can forget about. He shouldn't have to think about it, edit JSON, or approve routine operations. When it's working right, he notices by its absence — things just run.

Let me know if you need anything from me on the product side.
