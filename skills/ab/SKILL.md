---
name: ab
description: Analyze board — shape and flow of work, patterns and concerns
user-invocable: true
---

# ab — Analyze Board

Not a status dump — a PM read on the shape of the work. Two keystrokes.

## How to Use

1. Run: `bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards list` to get full board state
2. Analyze and report on:
   - **Flow:** Is work moving? Where is it stuck? What's aging?
   - **Balance:** Role load distribution. Is anyone overloaded or idle?
   - **Shape:** What kind of work dominates? (features vs infra vs product vs harvesting)
   - **Dependencies:** What's blocking what? Hidden chains?
   - **Risk:** WIP violations, stale cards, cards without owners or AC
   - **Patterns:** What's the board telling us that nobody's saying?
3. Have a position. Don't just describe — say what you'd change and why.

Keep it to a tight narrative, not a table dump. Jeff wants the PM's read, not raw data.
