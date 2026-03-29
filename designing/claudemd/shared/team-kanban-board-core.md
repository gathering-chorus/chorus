## Team Kanban Board

Board CLI: `../messages/scripts/cards` (alias: `board-ts`) | `cards --help` for full syntax. All board ops through `cards` — never call Vikunja API directly.

**No work without a card.** Move to WIP when starting (`cards move <id> WIP` + `role-state.sh <role> building card=<id>`). Move to Done when complete, not at session close. Equal priority → smallest first (DEC-049).
