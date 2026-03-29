## Board CLI Quick Reference

Board URL: http://localhost:3456 | CLI: `../messages/scripts/board-ts`

```
board-ts add "title" --owner <role> --priority P1|P2|P3   # create card (NOT "create")
board-ts move <id> <status>                                # Next, WIP, Done, "won't do"
board-ts done <id>                                         # mark Done + emit accept event
board-ts demo <id>                                         # log demo started (proving gate)
board-ts reject <id> "reason"                              # reject with reason
board-ts view <id>                                         # full card details
board-ts mine <role>                                       # role's cards
board-ts set <id> key=value [key=value ...]                 # unified mutation (domain=, chunk=, owner=, priority=, title=, desc=, after=, gates=)
board-ts deps <id>                                         # show card dependencies (after/gates)
board-ts ready                                             # cards with all deps done, ready to pull
board-ts comment <id> "text"                               # add comment
board-ts sequence [name]                                   # show sequence cards (no arg = summary)
board-ts sequence-tag <ids> <seq>                          # bulk-tag cards with sequence (comma-sep IDs)
board-ts audit-start <role>                                # session start audit
board-ts audit-close <role>                                # session close audit
```

**Product filter:** `board-ts --product chorus list` (Chorus only), `board-ts list` (all).
