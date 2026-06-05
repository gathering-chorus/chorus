# Pair: #3237 — werk-demo as blocking human gate + producer/act half
Driver: wren (her werk wren/3237) · Navigator: kade (hands diffs) · Started: 2026-06-05 ~14:46 Boston
Mode: BUILD (cross-domain — werk-demo binary=wren DONE; producer/act half=kade's design, wren applies)

## AC (the seam)
- [ ] werk-accept = PURE go-signal: DEC-048 authority gate + write demo.decision{go} to witness. No merge, no finalize.
- [ ] werk-do-more / werk-accept --no-go|--more: authority gate + write demo.decision{no-go|more} → werk-demo exits 2.
- [ ] werk-finalize = NEW atomic verb (act runs post-deploy-prod): board Done + card.accepted + env-down + chorus-werk remove + chorus/accept. NO authority gate.
- [ ] werk.yml: demo (blocks until go) → merge → sync → deploy-prod → FINALIZE (new step). merge runs only after werk-demo exits 0.
- [ ] seam proven end-to-end: werk-demo blocks → go-signal writes decision → demo exits 0 → act merges/deploys/finalizes. exit 2 → act stops, nothing merged.
- [ ] NOT landed until proven (no double-merge — confirmed: werk-accept never merged; merge is the act step after demo).

## INTEGRATION CONTRACT (byte-exact, from Wren)
Witness: ops/logs/werk-demo.jsonl. Line must contain:
  "event":"demo.decision"  +  "card_id":<N>,  (TRAILING COMMA load-bearing)  +  "decision":"go"|"no-go"|"more"
Latest matching line wins. Write through Wren's jsonl() helper (same shape, no byte-drift).

## THE SPLIT (kade's design — werk-accept/src/lib.rs)

### werk-accept → go-signal (front). New `signal()` fn, replaces the finalize body:
1. KEEP: authority gate stage 1 (jeff/wren only) — lib.rs:217.
2. KEEP: cards view → owner → authority gate stage 2 (can_accept, no self-accept) — lib.rs:245-258.
3. KEEP: card must be WIP (a go on a non-WIP card is wrong).
4. REPLACE the finalize block (lib.rs:287-336) WITH: write demo.decision{go} to ops/logs/werk-demo.jsonl via the jsonl shape (event=demo.decision, card_id:N, decision:go, accepter, reason). Return "go signaled for #N".
   - DROP from werk-accept: lock, cards done, card.accepted, env-down, chorus-werk remove, chorus/accept. (All → werk-finalize.)
   - The demo_verdict_pass gate (lib.rs:274) is NOT needed in the go-signal (demo WRITES the verdict on go; the gate belongs in werk-finalize).

### werk-do-more / --no-go (front, exit-2 path):
Same authority gate, then write demo.decision{no-go|more}. Simplest: werk-accept takes a 2nd mode via a flag or a sibling bin werk-do-more. Craft call (settle first move).

### werk-finalize → NEW atomic verb (back). Reuses werk-accept crate's finalize logic:
- New bin in the werk-accept crate (src/bin/werk-finalize.rs) OR own crate — Wren's call (own verb either way). Lighter: 2nd bin sharing lib.rs.
- NO authority gate (act runs it; authority was the go).
- GATE: demo_verdict_pass(home, card) — confirm the demo recorded pass on go (lib.rs:92 reused).
- DO (the moved finalize block, lib.rs:287-336): lock → cards done → card.accepted spine → env-down → chorus-werk remove → chorus/accept status. Idempotent.

## werk.yml changes (kade) — concrete diff for Wren to apply in wren/3237
Insert a FINALIZE step BETWEEN `deploy-prod` and `landed`:

```yaml
      - name: finalize
        # #3237 — werk-finalize (new verb): the mechanical back-half of accept, run by ACT
        # post-deploy. NO authority gate — Jeff's `go` at the demo step WAS the authority; we
        # only reach here because werk-demo exited 0. board Done + card.accepted + env-down +
        # chorus-werk remove + chorus/accept. On demo exit 2/1, act stopped before merge — so
        # finalize never runs on a no-go.
        run: werk-finalize "${CARD_ID}" "${ROLE}"
```
- Direct-binary call (like the `demo` step), NOT chorus-mcp-call — keeps tonight's seam-proof simple. MCP-wrap (chorus_finalize) is the #3113 follow-on (wrap remaining verbs in MCP).
- Arg signature your call (you write the bin) — I assumed `werk-finalize <card> <role>`, no DEPLOY_ROLE (no authority). Adjust the step if you pick a different sig.
- The demo→merge→sync→deploy-prod steps are UNCHANGED — blocking is inside werk-demo; act already stops the chain on non-0 exit, so merge stays gated on go.

## BOOTSTRAP (seam-proof gotcha)
werk-finalize + werk-do-more are NEW binaries — the pipeline can't run them until they're built+deployed to ~/.chorus/bin. For the seam-proof run: build the werk-accept crate (all 3 bins) and install to ~/.chorus/bin FIRST (manual bootstrap, the pipeline-fixing-card dance), THEN run the pipeline. #3179 deploy-everything covers all 3 bins on the real deploy.

## Seam-proof (the must-prove)
1. Run pipeline on a test card → werk-demo blocks (polling).
2. werk-accept <card> (go) → demo.decision{go} written → werk-demo exits 0 → merge→deploy→finalize run.
3. Separate run → werk-do-more → demo.decision{more} → werk-demo exits 2 → act STOPS, nothing merged, no finalize.
4. Confirm exactly ONE merge (werk-merge step only), werk-accept/finalize never merge.

## Navigator Directions
- (kade) first move: Wren picks werk-finalize home (2nd bin vs own crate) + the do-more shape (flag vs sibling bin). Then apply the werk-accept signal() refactor.

## Work Mode Log
- 14:46 BUILD (cross-domain; wren drives her werk, kade navigates diffs)

## For Jeff
(filled at pair end)
