## Team Kanban Board

Board CLI: `../../platform/scripts/cards` (alias: `cards`) | `cards --help` for full syntax. All board ops through `cards` — never call Vikunja API directly.

**No work without a card.** Move to WIP when starting (`cards move <id> WIP` + `role-state <role> building` — #2467: card lives on the board, not in role-state). Move to Done when complete, not at session close. Equal priority → smallest first (DEC-049).
