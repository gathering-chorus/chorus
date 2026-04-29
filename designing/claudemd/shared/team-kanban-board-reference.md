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

## Card Shape → Gate Set Mapping (#2566)

Each card carries a `shape:` label that determines which gates fire. The taxonomy is set at filing time by the builder (writing-surface discipline per #2561 v5). `/pull` reads the shape; `/demo` Step 5 nudges only the relevant gates. Full design at `designing/docs/gate-set-service-design.md`.

| Shape | Gates fired | Description / when |
|---|---|---|
| `shape:substrate` | code + quality + arch + ops + product | New data-model / endpoint / write-path. Examples: #2549, #2550, #2510 |
| `shape:surface` | code + quality + product + demo | UI/UX/route over existing data. Example: #2522 |
| `shape:coord-ci` | code + quality + arch + ops + product | Coordination touching deploy/runtime. Example: #2556 (PR merge cleanup with arch implications) |
| `shape:coord-process` | code + quality + product | Pure process / merge / branch hygiene without runtime impact |
| `shape:platform-tooling` | code + quality + arch (no product) | Hooks, scripts, skills, gate logic. Example: #2575. No user surface, so no product gate |
| `shape:refactor-internal` | code + quality | Pure rename, dead-code removal, comment changes — boundary-preserving |
| `shape:refactor-cross-cutting` | code + quality + arch | Touches module-boundary / hermeticity / cross-call state. Examples: #2543, #2577 |
| `shape:artifact` | product only | Investigation, audit, governance, doc, SHACL shape. Examples: #2523, #2524, #2525, #2554, #2561 |

### Refactor split — filing-time question

For `shape:refactor` cards, builder answers at filing time: **"Does this commit interact with timing, hermeticity, or visibility?"**

- YES → `shape:refactor-cross-cutting` (gate-arch fires)
- UNCLEAR → default to `shape:refactor-cross-cutting` (safer)
- NO → `shape:refactor-internal` (skip arch)

Mechanical diff-pattern check: does the diff add module-level const, static state, or cross-call captures? If yes → cross-cutting regardless of builder's answer.

### gate:ops-N/A token

Within multi-gate shapes, gate-runner declares `gate:ops-N/A` when no service surface is touched. Taxonomy says WHO gates; gate-runner says WHETHER-substance.

### gate:product distribution (Move 7 of #2561)

When live, gate:product splits into:
- **AC-verification** (peer-runnable): mechanical rubric, three-criteria test (deterministic command + binary observable + no Wren-context)
- **experience-integration** (Wren/Jeff only): scope-vs-promise, did-deliver-experience-promised, fits-product-narrative

**Belt-and-suspenders:** experience-integration on Wren-built cards always runs by another role or Jeff. **Judge-separation** (Liang 2024): peer running AC-verification MUST NOT share largest framing overlap with builder's domain.

### Tagging mechanism

Today (until #2567 lands shape support in cards CLI):
- New cards: include `shape:<value>` in the card's `Domains:` line at filing time, OR add via Vikunja UI label picker
- Existing cards: backfill happens organically as #2567 rolls out

Once #2567 ships:
- `cards add "title" --shape substrate` at filing
- `cards set <id> shape=substrate` for retrofit
- `/pull` reads shape and prints expected gate set
- `/demo` Step 5 nudges only the relevant gates
