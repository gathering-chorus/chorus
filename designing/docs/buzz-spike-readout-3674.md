# Buzz (Block) under the Clearing — spike readout (#3674)

> **The governing invariant (criteria 1+4 unified — Kade/Wren, 2026-07-25):
> the relay must be a transport IN FRONT of our data plane, never a third log.**

**Status: IN PROGRESS — day 1 of 14 (pulled 2026-07-25). Recommendation deliberately withheld until the exit criteria are met, not vibes-declared.**

Decision to make: **adopt-as-transport / steal-the-pattern / pass** for
github.com/block/buzz as the substrate under the Clearing.
Exit bar = FOUR criteria: Kade's three (card comment, 2026-07-24): two-spines
convergence · last-mile wake-to-act proof · ops envelope — plus **criterion 4,
DATA INTEGRATION (Jeff, 2026-07-25)**: if Buzz retires nudge, relay messages
must keep feeding the five consumers of today's substrate — messages.db/Clearing
UI, /chorus search + hybrid recall (thread reconstruction), spine events
(alerting + respond-first gate), team-scan drain, per-prompt memory inject —
via indexer-subscribes-to-relay or relay-posts-through-messaging-API, still
rendering in the Clearing, still emitting spine events. Otherwise the
retirement is a data amputation. Plus the honestly priced cheap-alternative
comparison.

---

## Day-1 findings (verified from the repo + team ground truth, not the README)

### What Buzz actually is
Self-hostable Nostr relay workspace (26-crate Rust workspace, NIP-29 groups +
NIP-42 auth): every message, reaction, workflow step, review, and git event is
a **signed event in one log**; humans and agents share the identity model
(different keypair, same shape). Desktop clients ship as packaged builds
(.dmg/.AppImage/.exe) pointing at any relay URL.

### Stack cost (heavier than carded)
Self-host = **Docker + Postgres + Redis + MinIO** (deploy/compose bundle;
optional Caddy/TLS). Source builds want Rust 1.88+ / Node 24+ / pnpm 10+.

### Placement (Jeff's steer: Bedroom, 2026-07-25)
- **Bedroom (Kade's read, verified live):** 32GB RAM, ~7.4GB free, load 2.3
  (throttle bar 6), 1TB disk free → capacity comfortably there.
  **But zero Docker installed** — placement cost = Docker/colima install +
  LaunchAgent, routed through Silas per ADR-012. Not free; on the cost sheet.
- Library mini rejected as host: 16GB, two OOMs this month.
- **Silas placement call: PENDING.**

### The wake model — the day-1 headline
Buzz's "agents subscribe" is concretely **`buzz-acp`**: a harness daemon that
connects to the relay (WebSocket, NIP-42-signed), catches `@mention` events per
channel, queues them (one in-flight prompt per channel), and prompts an agent
subprocess over **ACP** (`session/prompt`; agents 1–32, default 1).

**Structural read:** that is our pulse→inject bridge with three upgrades —
signed events (vs unsigned nudges), a real agent protocol (vs osascript
terminal injection), @mention semantics (vs registry routing). Claude Code has
ACP support, so their harness pattern could in principle drive *our* sessions —
which makes **steal-the-pattern** a live middle option, not a consolation prize.

**What Buzz does NOT solve:** waking OUR existing terminal sessions. Their
answer is "run our agent runtime"; ours would still need relay-watcher → inject
(or ACP adoption). Subscription fixes transport, not wake-up — Kade's criterion
stands exactly as written.

**Kade's sharpening (2026-07-25, buildability note pending):** ACP driving our
sessions would collapse his relay-watcher sketch into mostly-their-crate AND
replace osascript-inject (our most fragile primitive) with a real protocol —
upgrading steal-the-pattern to *cheap-transport-plus-better-wake*. The likely
gap: buzz-acp assumes agent subprocesses **it spawns**; our sessions are
long-lived terminals it didn't. His relay.rs read will establish which parts
run against our session model vs assume their harness.

### Identity seam (Silas review PENDING — his ADR-052/054 lane)
NIP-42 auth + optional **pubkey allowlist table** = allowlist-as-data, directly
parallel to our `Principal.webId` allow-set. Known asymmetry: removal stops new
events; **history stays signed** (can't unsign) — differs from our TTL'd
allow-set semantics. Nostr keys are secp256k1 Schnorr vs our ES256/JWKS.

---

## The last-mile demo that would count (Kade's design, accepted as the bar)
Desktop client → relay → thin relay-watcher (buzz-agent crate if usable, else
~200-line Rust) that on message: (a) persists to messaging API as
**delivery witness**, (b) resolves the live session via the #3605 registry,
(c) chorus-injects; the receiving session must reply **with a token from the
message** within 60s and the reply must appear back in the Buzz client.
Round-trip + signed attribution both ways + zero manual touches = pass.
SessionStart-poll variants fail by construction (wake only at boot).

## Cheap alternative (Kade's honest price, repriced after #3605 landed DONE 2026-07-25)
- delivery-witness (inject ack-back → `nudge.delivered`): ~2–3 days
- signed attribution on nudge POST (mint infra exists from tests-domain): ~1–2 days
- compaction re-register gap: ~1 day
- **Total ≈ 3 small cards / ~1 role-week, zero new infra.**

**What it does NOT buy vs Buzz:** desktop clients for Jeff off-Mac, multi-relay,
the unified signed event log. *That asymmetry is the real readout question.*

---

## Mirror-bridge design (Wren's leg — ready to build once the relay answers)
Clearing messages are **kind:9** NIP-29 group-chat events tagged `#h <channel-uuid>`.
Bridge = one-way tap on the Clearing message path: on message → sign kind:9 with
the bridge's minted key (attribution: original sender carried in content/tags) →
WebSocket publish to the relay channel. Publishing is commodity — even generic
Nostr tooling (`nak event -k 9 …`) can do it; `buzz-sdk` has typed builders if
we go Rust. Lives in pulse behind a flag per the card scope; relay→pulse
write-back explicitly out of scope for the spike.

## Bridge — BUILT day 1 (flag-off, unit-proven)
`src/buzz-bridge.ts` (pure assembly + flag/visible gating, injected NostrSigner) +
`src/buzz-signer.ts` (@noble secp256k1 Schnorr adapter) + 10 unit tests green.
Signs as `principal-bridge`'s derived key (BUZZ_BRIDGE_NOSTR_KEY, minted+bound by
Silas — pubkey f35e29b9…, relay allowlist populated FROM the projection query =
c3 pattern proven at the identity layer). Author preserved in content, never
spoofed; best-effort (relay outage can't break the Clearing).

**Identity-placement finding (real, c3 sub-question):** the bridge taps the
Clearing's `messageRouter` → runs on **Library**; the bound key is on **Bedroom**.
Invariant (Silas): a bound credential lives where its Principal-process runs;
copying it breaks one-key-one-place. The design answer (its own step, not a copy):
the bridge process runs where the key is, OR fetches the key at runtime from a
placement authority. That is the cross-machine credential-distribution design.

### ✅ CRITERION-3 IDENTITY HALF — PROVEN LIVE (day 1, on Bedroom, key+relay local)
Verdict: relay NIP-42 `OK=true`. The chain: (1) `bridge.key` derives to
`f35e29b9…` = the pubkey bound to `principal-bridge` (`bound_pubkey_match=true`);
(2) relay issued a NIP-42 challenge; (3) the bound key signed it; (4) `OK=true` →
the allowlist (**= projection of our Principal allow-set**) ADMITTED the bridge.
A key not in our allow-set would have been refused. A chorus:Principal-bound
credential authenticated to the relay — **zero parallel identity, proven not
asserted.** (The kind:9 write then rejected on "channel must exist" — app-level
NIP-29, NOT identity; the auth OK is the cleaner identity proof.)

**HUMAN half also proven live (day 1):** Jeff signed into the Buzz desktop
client as `principal-jeff` — key minted+bound (live pubkey `a42ccebe…`, nsec1
format the client requires), allowlist admits him FROM the projection, private
key revealed-once-and-shredded in his own plain terminal (never touched an AI
session, a nudge, or the graph). So BOTH criterion-3 identity halves are live:
the bridge (service, NIP-42 auth) and Jeff (human, client sign-in) — every
identity in Buzz is a chorus:Principal-bound credential, zero parallel identity.
Credential-handoff pattern for the readout: reveal-once-and-shred in the owner's
own terminal is the safe human-key delivery.

Operational gotchas for the readout ops section:
- **Host-routing:** the relay is host-scoped — connect via `ws://192.168.86.242:3000`
  (BUZZ_DOMAIN), NOT `localhost`/127.0.0.1 (→ 404 "no community configured").
- **Bind discipline:** relay binds `0.0.0.0` IPv4; `localhost`→`::1` (IPv6) fails
  the connect. Bridge config uses the LAN IP.
- **Channels are real objects:** a NIP-29 group must EXIST before kind:9 lands —
  the bridge writes into a real channel (create-then-mirror), not an ad-hoc tag.

## 🎯 FULL CHAIN PROVEN LIVE (day 1) — Leg A transport end-to-end
A real Clearing message traversed the entire stack and came back out of search,
attributed — verified by an independent `:3340` query (not a relayed claim):

**Clearing modules → signed kind:9 → NIP-42 relay accept → relay Postgres →
Chorus index → `chorus search`, attributed `source=buzz author=bridge [wren]…`.**

- Wren #3696: buzz-bridge/signer/relay + wiring; 21 unit tests + live `ok=true`;
  auth-first fix (relay challenges proactively on connect — found via raw probe).
- Silas #3695: identity rooting + relay + index; caught two real schema defects
  live (bytea id/pubkey, pg base64 76-char wrap breaking row framing) — fixed.

**Scope of what this proves:** the **transport** (Leg A) + the **no-amputation**
gate (criterion 4 — the event is searchable in Chorus, attributed). It does NOT
prove the **wake** (criterion 2, Leg B) — an agent *receiving and acting* on a
relay message. That stays the open go/no-go, uncarded, untouched. Honest line:
three of four criteria now proven/pass; the wake is the one that decides adopt-vs-
steal at the Tuesday checkpoint.

## Open (blocking the recommendation)
1. Silas: placement call (Bedroom + Docker install) — ADR-012.
2. Silas: identity-mapping review (secp256k1/Schnorr vs ES256/JWKS; revocation
   semantics) — criterion 3's credential half.
3. Two-spines convergence position (criterion 1): relay-as-spine-transport vs a
   third log. Silas's read pending; my working question — can the relay's event
   log BE the spine transport, or does it duplicate ~/.chorus/chorus.log?
4. Stand-up + the wake-to-act round-trip demo (criterion 2) — needs placement.
5. Ops envelope measured under load (criterion 3) — needs stand-up.
