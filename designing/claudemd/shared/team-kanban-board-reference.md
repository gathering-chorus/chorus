## Board CLI Quick Reference

Board URL: http://localhost:3456 | CLI: `../../platform/scripts/cards`

```
cards add "title" --owner <role> --priority P1|P2|P3   # create card (NOT "create")
cards move <id> <status>                                # Next, WIP, Done, "won't do"
cards done <id>                                         # mark Done + emit accept event
cards demo <id>                                         # log demo started (proving gate)
cards reject <id> "reason"                              # reject with reason
cards view <id>                                         # full card details
cards mine <role>                                       # role's cards
cards set <id> key=value [key=value ...]                 # unified mutation (domain=, chunk=, owner=, priority=, title=, desc=, after=, gates=)
cards deps <id>                                         # show card dependencies (after/gates)
cards ready                                             # cards with all deps done, ready to pull
cards comment <id> "text"                               # add comment
cards sequence [name]                                   # show sequence cards (no arg = summary)
cards sequence-tag <ids> <seq>                          # bulk-tag cards with sequence (comma-sep IDs)
cards audit-start <role>                                # session start audit
cards audit-close <role>                                # session close audit
```

**Product filter:** `cards --product chorus list` (Chorus only), `cards list` (all).
