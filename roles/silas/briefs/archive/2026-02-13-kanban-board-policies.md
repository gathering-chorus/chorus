# Brief: GitHub Projects Board — Policies & Automation

**From**: Wren (PM)
**To**: Silas (Architect)
**Date**: 2026-02-13
**Priority**: High — Jeff wants this ready for morning

## What We Need

Configure the existing GitHub Projects kanban board (https://github.com/users/WJeffBridwell/projects/1) with proper policies and automation. No migration — we're staying on GitHub Projects.

## Tasks

### 1. Configure the GitHub MCP Server

The [GitHub MCP server](https://github.com/github/github-mcp-server) gives Claude Code sessions native access to issues, PRs, and projects. Configure it so all three roles (Wren, Silas, Kade) can manage the board from their sessions.

- Install/configure the MCP server
- Document how each role accesses it
- Test basic operations: list items, create item, move item, update fields

### 2. Board Policy Design

Define and configure board policies that reflect how the team works:

**WIP Limits:**
- GitHub Projects supports soft WIP limits on board columns. Recommend:
  - In Progress: limit 3 (one person, three roles — shouldn't have more than 3 active items)
  - Todo: no limit
  - Done: no limit
- Configure these on the board

**Workflow States:**
- Current: Todo, In Progress, Done
- Consider adding: Blocked, Ready (for items that are unblocked and prioritized but not started)
- Your call on what makes sense architecturally

**Fields:**
- Already have: Status, Theme, Project
- Consider adding: Owner (Wren/Silas/Kade), Priority (P1/P2/P3), Blocked By (text field for dependency notes)

### 3. GitHub Actions Automation (if feasible overnight)

Design 2-3 GitHub Actions workflows:
- Auto-move items to Done when related PR is merged
- WIP limit warning (if a column exceeds limit, post a comment or log)
- Auto-assign owner based on Theme (Core Security → Kade, architecture items → Silas, product items → Wren)

If Actions are too heavy for overnight, just document the automation design and we'll build it tomorrow.

### 4. Wrapper Script

Create a small shell script (`board.sh` or similar) in `../messages/` or `../product-manager/scripts/` that wraps common `gh project` commands:
- `board.sh list` — show all items by status
- `board.sh add "title" --status "Todo" --theme "Core Security"` — add an item
- `board.sh move <id> "In Progress"` — change status
- `board.sh done <id>` — move to Done

This gives all three roles a clean interface without memorizing GraphQL mutations.

## Context

Jeff wants the board to feel like a real team kanban — WIP limits, clear workflow states, assignment policies. He's used to managing engineering teams with these tools. The board should be ready for use when he starts exploratory testing tomorrow morning.

— Wren
