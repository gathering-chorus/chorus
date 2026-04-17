# Test-Run Alert Log — #2149

**Directive:** Jeff, 2026-04-17 09:48 — "document every alert that hits you while running the tests — the run creates a flood of nudges and not all of them are essential for testing."

**Purpose:** Capture every inbound signal (nudge, bridge post, system alert, inject payload) that lands on the Kade terminal *while a chorus test suite is running*. Categorize each. Noise entries feed #2131 (chorus-inject hermeticity) and related work; essential entries are real test-result signals and stay.

**Format:**
```
## Run <N> — <timestamp> — <suite>
<trigger: what I ran>

- [category] <source>@<time>: <content>
```

**Categories:**
- `essential` — signal directly produced by the test result (e.g., nightly backstop's owner-routed nudge after pass/fail aggregation)
- `noise-inject` — test fired a real chorus-inject payload into terminals instead of mocking
- `noise-bridge` — test wrote to the real Bridge instead of a test bridge
- `noise-nudge` — test fired a real nudge via `platform/scripts/nudge` instead of a test no-op
- `noise-spine` — test wrote real spine events (may be OK — spine is append-only — but note)
- `noise-system` — system reminder (compaction, memory, etc.) not related to tests
- `unrelated` — Wren/Silas/Jeff real traffic that happened during the run

## Runs

### Run 1 — 2026-04-17 ~09:55 — directing/products/cards (jest)
Trigger: `npx jest` (full suite after fixes)

- No inbound nudges, bridge writes, inject payloads, or alerts during this run.

Observation: the cards suite is **hermetic**. It mocks `fetch` and doesn't touch shared Chorus infra. Other suites (especially chorus-inject, bridge-integration) are the noise sources — confirmed by earlier bursts during gate:code runs.

### Run 2 — 2026-04-17 11:56–11:57 — directing/clearing (jest)
Trigger: `npx jest` (full clearing suite)

Test result: 2 failed, 1 skipped, 146 passed, 149 total.

Inbound during the ~67s run:
- [noise-nudge] wren@11:56: "Test nudge from wren to kade"
- [noise-nudge] silas@11:56: "Test nudge from silas to kade"
- [noise-bridge] silas@11:57: "Bridge integration tests are failing — attribution shows jeff instead of wren on PM thinking messages. Need to check session-tailer.ts line 238."
- [noise-bridge] wren@11:57: "What do you think about this approach? [REPLY EXPECTED — nudge wren back]"
- [noise-bridge] wren@11:57: "Brief in your inbox."
- [noise-chat] silas: "replied in chat silas-kade-1776441447"

Count: 6 noise signals, 0 essential.

Pattern:
- The "Test nudge from X to kade" payloads come from bridge-integration or similar suites that literally call `platform/scripts/nudge` with role-name test inputs.
- The `session-tailer.ts line 238` message is a test-case assertion string being used as the nudge body — the test POSTs to Bridge with that as the `text` field and Bridge relays it. No hermeticity barrier.
- `[REPLY EXPECTED]` tag is baked into the test fixture; there's no actual expectation of a reply. Triggered my attention-contract logic until pattern-matched.
- The chat-reply notification is from a chat session the test created and replied in.

Implications for hermeticity cards:
- `nudge` (shell script) needs `CHORUS_TEST_MODE=1` honored — when set, write to a log instead of delivering via osascript/bridge-POST.
- Bridge POST handler could filter/route test messages by a header/sender prefix into a test channel.
- Chat creation APIs in tests should use a dedicated test-channel-namespace not read by real roles.

Writing this now rather than trying to remediate inline — the fixes belong in #2131 expansion.

### Run 3 — 2026-04-17 11:58 — directing/clearing re-run (jest, filtering to fail/skip)
Trigger: `npx jest` (re-run to enumerate failures)

Inbound during re-run:
- [essential] silas@11:58: "[#2119 cleanup] Your roles/kade/.claude/hooks/infra-guardrails.sh still has live docker deny/ask blocks..." — REAL signal, unrelated to test run, addressed separately
- [noise-nudge] wren@11:58: "Test nudge from wren to kade"
- [noise-nudge] silas@11:58: "Test nudge from silas to kade"
- [noise-bridge] silas@11:58: "Bridge integration tests are failing — attribution..."
- [noise-bridge] wren@11:58: "What do you think about this approach?"
- [noise-bridge] wren@11:58: "Brief in your inbox."
- [noise-chat] silas: "replied in chat silas-kade-1776441538"

Count: 6 noise, 1 essential (coincidentally landed mid-run).

Pattern confirmed: the same 6-payload burst reproduces every clearing-suite run. Fully scripted, fully noisy. Hermeticity fix would eliminate 6 noise signals per nightly.

### Run 4 — 2026-04-17 ~12:30 — full nightly-suites.sh --run-all sweep
Trigger: `HERMETIC_TEST_MODE=1 bash platform/scripts/nightly-suites.sh --run-all`

Inbound during the ~10 min sweep (while cargo chorus-inject was running):
- [noise-inject] `[cargo-test] AC4 wren inject[cargo-test] AC5 nudge e2e[cargo-test] AC4 silas inject[cargo-test] AC4 kade inject` — concatenated payload

Count: 1 concatenated payload, 4 distinct inject test cases.

Pattern: chorus-inject's Rust integration tests fire real injects into each role's terminal via the chorus-hook-shim binary. The tests run under `cargo test --release` and the HERMETIC_TEST_MODE env var is set at the parent shell level but the Rust tests don't check it. They call the inject pipeline directly.

Fix path: either (a) add a runtime check in chorus-hook-shim's inject command that no-ops when HERMETIC_TEST_MODE=1 (Silas's kill-switch idea from our 12:11 chat), or (b) gate the Rust tests themselves with `#[cfg_attr(...)]` or a runtime skip.

Approach chosen: (a) — single-point defense in the shim, covers all current and future callers. Silas committed to filing the kill-switch card after our chat.

Sweep results:
- 8 npm suites: 6 pass, 1 fail (chorus-sdk 5/35), 1 app-scope fail (out of scope for #2149)
- 2 cargo: 1 pass (chorus-inject — counted as pass because chorus-inject's own test runner reports 2 of 2 suites ok after my sweep, but the nudges fired per-run — separate concern), 1 fail (chorus-hooks — stale reading, fix committed in session)
- 13 shell: 8 pass, 4 fail, 1 status-miscount (test-skip-gates — reported pass but summary says 2 fail; investigate)

### Run 5 — 2026-04-17 ~12:30–12:45 — chorus-wide sweeps during audit
Silas sent detailed AX data from his session: **65+ nudges over ~45 min**, including mid-sentence cuts into Jeff's typing ("hospitals are a good example of this issue — lots of equipment [nudge...]"). Full inventory (from Silas):

- [noise-nudge] structural-test variants with quotes: ~12 + ~5 + ~4 (jest clearing-ui family — NOT yet gated)
- [noise-bridge] "this doesn't break anymore": ~8
- [noise-bridge] "post-build accessibility test": ~7 (this is my own test name — cargo-side tests fired real injects before the chorus-inject gate landed)
- [noise-bridge] "structural-test" bare: ~3
- [noise-nudge] `[nudge from kade ... Test nudge from kade to silas]`: ~6
- [noise-nudge] `[nudge from kade ... Exchange test]`: ~5
- [noise-nudge] `[nudge from kade ... post-build accessibility test]`: ~6 (nudge-wrapped)
- [noise-nudge] `[nudge from wren ... Test nudge from wren to silas]`: ~4
- [noise-nudge] `[nudge from wren ... Check the deploy]`: ~3
- [noise-inject] `[cargo-test] AC4 silas inject`: 2 (chorus-inject, pre-gate run)

Fix coverage after my gates so far:
- nudge-integration.test.ts (HERMETIC): kills "Test nudge from X", "What do you think", "Brief in your inbox", "session-tailer line 238", "Exchange test" — ✓ addressed
- clearing-ui.test.ts AC3 (HERMETIC): kills chat.sh test-chat family — ✓ addressed
- chorus-inject Rust (HERMETIC): kills "[cargo-test] AC4 X inject" — ✓ addressed

Still uncovered (will investigate):
- "structural-test" + quoted variants — appears clearing-ui or bridge-integration have additional describes beyond AC3
- "this doesn't break anymore" — unknown test, need to find
- "Check the deploy" — appears in nudge-integration but also somewhere else
- "post-build accessibility test" — my own cargo tests (post_build_accessibility.rs) that did real injects before gate; will gate now

### Run 6 — (final green-or-known-scope sweep pending)





