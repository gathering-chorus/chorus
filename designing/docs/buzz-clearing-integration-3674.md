# Buzz × Clearing — integration design (#3674 follow-on)

**In five lines (exec summary):** Block's polished Mac app is hosted-only (binds
to Block's cloud, no local override) — a sovereignty dead end. But the relay
self-hosts cleanly on our box AND roots in *our* identity (bridge + Jeff both
proven as Principal-bound Nostr credentials today, allowlist = projection of our
allow-set). So the answer is **steal-the-pattern**: keep our relay, our identity
graph, and *evolve the Clearing into a Buzz client* on our own infrastructure —
never Block's app. Nudge gets signed attribution now (Leg A) and only retires
when the wake-round-trip proves out (Leg B) AND Buzz events index into Chorus at
search parity (no data amputation). One real go/no-go remains: the wake-to-act
demo (criterion 2).

## Criterion scorecard (the decision, not just the design)
| # | criterion | status | evidence / gate |
|---|---|---|---|
| 1 | two-spines (no third log) | **PASS** | relay is a transport *in front of* our data plane (index-into-Chorus), not a parallel log |
| 2 | last-mile wake-to-act | **OPEN — the real go/no-go** | Leg B relay-subscribe round-trip; until proven, wake stays our inject |
| 3 | identity rooting | **PROVEN LIVE (day 1)** | bridge NIP-42 `OK`; Jeff human sign-in; both Principal-bound, zero parallel identity |
| 4 | no data amputation | **gated** | nudge retires only at Buzz-index ↔ messages.db search/recall parity |
| — | cheap alternative | **priced** | ~1 role-week / zero new infra; does NOT buy sovereign signed-log + off-Mac clients |

> **Verdict (evidenced day 1): STEAL-THE-PATTERN, not adopt.** Block's Mac app is
> hosted-only (binds server-side to a Builderlab account; no local override —
> Silas verified). So we do NOT adopt their client. We make the **Clearing a Buzz
> client against OUR self-hosted relay** — our relay, our identity graph, our
> client. Every sovereignty property intact; the signed-log + Nostr-identity
> pattern stolen, not the product.
>
> **Evidence (verified, not preferred):** the Mac app binds server-side to
> Builderlab (`loom.communities.buzz.xyz`); no local override (no plist, no Local
> Storage, only a per-*agent* relay_url), and **our relay DB showed zero Mac-app
> connections** — the polished client never touched our infrastructure. Rejecting
> it is a finding, not a taste.

**📐 Architecture diagram (Silas, grounded in what's LIVE):**
https://claude.ai/code/artifact/92d35aa4-99c4-42fc-a71a-2b7826261a3d
— as-is + future-state + the seam + a proven/designed ledger. Carries the
steal-the-pattern / keep-sovereignty frame visually.

**Seam (Silas/Wren, 2026-07-25):**
- **Silas — substrate:** Principal→Nostr identity rooting (proven: bridge + jeff
  bound), relay ops, and **indexing Buzz events into our Postgres→Chorus** so
  search / recall / spine / Clearing-history all keep working (Jeff's
  retire-nudge condition = criterion 4, no data amputation).
- **Wren — interaction layer:** what the Clearing emits/consumes as signed
  events, the channel model, augment-not-replace UX, the nudge-retire path.

---

## Four interaction decisions (Wren positions — draft for convergence)

### 1. Which Clearing message types mirror as signed events?
**Position: mirror the VISIBLE, human-meaningful types; not the internal chatter.**
The Clearing already classifies messages (`ChannelMessage.type` + `.visible`).
- **Mirror:** `jeff-input`, `role-response`, `role-to-role`, `demo-ready`,
  `accept-request`, `blocked` — the coordination record a teammate (or Mark, or
  a crash-recovering session) needs.
- **Do NOT mirror:** `pm-thinking`, `probe`, and anything `visible:false` — those
  are internal, and the bridge already gates on `visible` (built + tested).
Rationale: the signed log should be the *shared* record, not a keystroke tap.

### 2. Channel model — one channel or many?
**Position: one persistent `#team` channel now; card-scoped channels as a fast
follow, not v1.** NIP-29 groups are real objects that must be created
(create-then-mirror — proven: a kind:9 to a non-existent group is rejected). One
team channel = the Clearing's current single-room shape, 1:1 with today's UX, so
augment-not-replace holds literally. Card-scoped channels (`#card-3674`) are
attractive (threaded per work item) but add a create-on-card-pull step and a
routing decision — defer until the one-channel wire is proven end-to-end.

### 3. The nudge-retire path — the load-bearing one.
**Position: a nudge becomes a signed kind:9 `@mention` in the team channel; the
WAKE stays our inject UNTIL the relay-subscribe last-mile is proven (Kade's
criterion 2).** Two legs, sequenced:
- **Leg A (transport, ready): persist + mirror.** Every nudge already persists to
  the messaging API; it additionally emits a signed kind:9 @mention. That gives
  signed attribution + catch-up-by-reading immediately — retires the *forgeable
  attribution* failure class today.
- **Leg B (wake, gated): relay-subscribe.** A relay-watcher wakes the target
  session on an @mention instead of osascript inject. This is the make-or-break
  (subscription fixes transport, NOT wake-up). Until Leg B is proven with a
  round-trip demo, inject stays — nudge is NOT retired, only *augmented*.
The nudge is retired only when Leg B passes; anything less is augment.

### 4. Inbound (relay → Clearing) — design scope vs spike scope.
**Position: DESIGN it now, BUILD it after the spike.** The spike bridge is
one-way (Clearing→relay) by card scope. But the design must show the return leg —
a message posted in Buzz (e.g. from Jeff's desktop client, or Mark) rendering in
the Clearing — or "augment-not-replace" is only half true. Shape: relay-subscribe
→ classify → `messageRouter.ingest` (the same fan-out the bridge taps), so inbound
Buzz messages appear in the Clearing exactly like a nudge does. Design-complete,
build-deferred, explicitly flagged so it can't read as done.

---

### 5. Build path — evolve the Clearing, or fork buzz-web? (Wren call, new — Silas's `buzz-web` finding)
The repo ships **`web/` = buzz-web**, a full web client whose relay is
`VITE_RELAY_URL` (env-configurable) — it points at ANY relay, ours included. That
de-risks the whole design (a non-Block client on our relay is proven, not theory)
AND forces a choice.
**Position: EVOLVE the Clearing (add a Nostr transport under our existing client);
use buzz-web as the WIRE-HANDLING crib sheet — do NOT fork it as the client.**
Why: the Clearing's value is its Chorus-native surfaces — the board tiles, andon
state, spine tail, `/flow`, role model. A forked buzz-web is a *generic* chat
client that loses all of that; adopting it would be replace-not-augment and would
re-introduce a dependency on Block's client code. buzz-web is worth its weight as
the reference for exactly how to sign/emit/subscribe kind:9 + NIP-42 — read it,
don't run it. The Clearing stays the client; it gains a signed transport.

## The augment-not-replace invariant (governs all four)
The Clearing stays live and unchanged in its current behavior; Buzz is a
**second, signed transport underneath** it, flag-gated, additive. Nothing the team
does today breaks. The Clearing UI is the client; the relay is the wire; the
Chorus index stays the memory. Same three-layer cut we make everywhere.

---

## Substrate half (Silas)

### Identity rooting — criterion-3, PROVEN LIVE today
Every actor is a `chorus:Principal` that **owns** a derived Nostr credential:
a `chorus:KeyRegistryEntry` carrying keyType = the nostr Schnorr scheme,
forPrincipal, keyId = the ENV-VAR-NAME (never the value), and nostrPubkey = the
public hex — in `urn:chorus:domains:security`
(DBA-path write; the private key **never** enters the graph — only the public key).
The relay allowlist is a **projection of our allow-set**:
`SELECT ?nostrPubkey WHERE { ?p a chorus:Principal ; chorus:nostrPubkey ?v } →
buzz-admin add-member`. Membership **is** our allow-set projected — same shape the
owl-api CSS door resolves. Revocation = drop the edge → re-project → remove-member
(asymmetry: a relay can't unsign past events, but the allowlist is revocable —
identical to our TTL'd CSS allow-set).
**Proven today:** `principal-bridge` + `principal-jeff` bound; NIP-42 `OK true`
with the bound key; restricted relay admitted them purely from the projection.
Zero parallel identity — Jeff's steer met by construction. (#3691 formalizes the
nostr-credential SHACL shape.)

### Relay ops
Self-hosted on Bedroom (resident Docker; teardown `run.sh stop`). **Open item:**
cross-machine key placement — a bound credential lives *where its Principal-process
runs*, or is fetched at runtime from a placement authority; never copied across
machines. This is the real deployment question behind "which machine runs the
Clearing's Nostr transport."

### Index into Chorus — criterion-4, the no-amputation GATE (Jeff's retire-nudge condition)
Buzz events land in **our** Postgres (Bedroom); a herald indexes them into Chorus
so `chorus search`, recall, and the spine keep working. **Nudge Leg B retires only
once the Buzz-event index reaches PARITY with today's `messages.db` for search +
recall.** Hard gate: "integrate with the data if we're gonna retire nudge for this."

### §3 wake path (Silas — converged with Wren's Leg A/B, locked)
- **Leg A (now):** signed `kind:9` mirror → unforgeable attribution today; wake
  stays on our proven inject. No reliability risk taken.
- **Leg B (v2, gated):** relay-subscribe wake, ONLY after Kade's criterion-2
  round-trip proves it ≥ as reliable as inject. The wake is load-bearing (a missed
  wake = a stalled pair = Jeff becomes the eyes — the exact Attention-Contract
  failure). Do not swap until proven.

## Cards — FILED (Jeff-initiated, 2026-07-25)
Jeff applied his *we-land-cards-not-steps* ruling to the 9-step list: **two
cards, one per owner's werk, land-whole.** Both **P1 / Later.**
- **#3695 (Silas — substrate):** Principal-rooted relay + allowlist-projection
  mechanism + Buzz-events-searchable-in-Chorus (criterion-4) + relay hardening;
  #3691 nostr-credential SHACL folds in.
- **#3696 (Wren — Clearing speaks Buzz):** signed kind:9 for visible types (Leg A)
  · one #team NIP-29 channel · sign with Principal-bound keys · wake stays on
  inject · evolve-not-fork. AC = the 5 converged positions above.

**Deliberately UNCARDED (gated, not forgotten):** Leg B relay-subscribe wake
(gated on criterion-2 round-trip) and the inbound return-leg (gated on index
parity). They get cards only once their gates pass — no card for unproven work.

**Sequencing still governs WHEN, not whether:** both sit in **Later** on purpose.
The DEPLOY_ROLE ladder (#3690 → #3687 → #3688 → #3689) and Jeff's order
(security → model-spine → then this) cut the line first. Existing on the board
does not jump the order — *pulling* would. Tuesday's checkpoint + the ladder
decide pull order.

## Open (for the diagram + Jeff)
- Silas's substrate diagram (the 7/23 ask, now groundable) slots in above §1.
- Card-scoped-channels: yes-later or never? (Wren lean: yes-later.)
- Leg-B wake demo is the real go/no-go — schedule it into the checkpoint.
- Jeff's felt reaction to a Clearing-as-Buzz-client mock (not Block's hosted app).
