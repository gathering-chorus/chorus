// @test-type: e2e:ui — playwright browser flow (clearing-lifecycle), live surface
/**
 * #3857 — MOVED from jeff-bridwell-personal-site/e2e/tests/chorus-clearing.spec.ts.
 *
 * A Chorus product tested by the gathering suite, which Chorus never runs. The
 * tests domain showed 100 tests, all pyramidLayer:unit, zero clearing — true of
 * this repo, and the reason none of Jeff's 2026-08-13 defects were caught.
 *
 * NOTE what this file is and is not: it drives the chat API (request), not the
 * browser. It proves the lifecycle, not what Jeff sees. The UI behaviours he
 * found by hand live in clearing-ui.spec.cjs alongside it.
 */
/**
 * Chorus Clearing E2E Tests — #2291
 *
 * End-to-end validation of the Clearing chat session lifecycle:
 *   Start session → roles join → send messages → decisions captured → end session
 *
 * Clearing chat mode runs through the Bridge API on localhost:3470.
 * Endpoints: POST /api/chat/start, /api/chat/message, /api/chat/end
 */
const { test: base, expect } = require('@playwright/test');
const { ownClearing } = require('./lib/own-clearing.cjs');

// #4417 — no prod default and no live credential. This spec POSTed chat
// sessions and DECISION messages to Jeff's live Clearing on :3470 with the
// machine's bridge token. It now drives its own Clearing (lib/own-clearing.cjs)
// unless CLEARING_URL names a variant; the live room is refused.
const JEFF_WEBID = 'https://pods.example/jeff/profile/card#me';
const CLEARING_TARGET = ownClearing(base, { signedInAs: JEFF_WEBID });
const BRIDGE_URL = CLEARING_TARGET.url;
const AUTH = CLEARING_TARGET.auth;

base.describe('Clearing: chat session lifecycle', () => {
  base('start session → send message → end session completes full lifecycle', async ({ request }) => {
    let sessionActive = false;

    await base.step('Start a clearing session', async () => {
      const response = await request.post(`${BRIDGE_URL}/api/chat/start`, {
        data: { initiator: 'jeff' },
      });
      // Accept 200 (started) or 409 (already active)
      expect([200, 409]).toContain(response.status());
      sessionActive = true;
    });

    await base.step('Send a message in the clearing', async () => {
      const response = await request.post(`${BRIDGE_URL}/api/chat/message`, {
        data: { from: 'jeff', text: `DECISION: E2E test decision ${Date.now()}` },
      });
      expect(response.status()).toBe(200);
    });

    await base.step('Send a role response', async () => {
      const response = await request.post(`${BRIDGE_URL}/api/chat/message`, {
        data: { from: 'kade', text: 'Acknowledged — e2e test response' },
      });
      expect(response.status()).toBe(200);
    });

    await base.step('End the clearing session', async () => {
      if (sessionActive) {
        const response = await request.post(`${BRIDGE_URL}/api/chat/end`);
        expect(response.status()).toBe(200);
      }
    });
  });

  base('messages sent during clearing appear in message list', async ({ request }) => {
    const uniqueText = `e2e clearing decision ${Date.now()}`;

    await base.step('Post clearing message via main message API', async () => {
      const response = await request.post(`${BRIDGE_URL}/api/message`, {
        headers: AUTH,
        data: { from: 'wren', text: uniqueText },
      });
      expect(response.status()).toBe(200);
    });

    await base.step('Verify message appears in list', async () => {
      const response = await request.get(`${BRIDGE_URL}/api/messages`);
      expect(response.status()).toBe(200);
      const messages = await response.json();
      const found = messages.some((m) => m.text === uniqueText);
      expect(found).toBe(true);
    });
  });

  base('NEGATIVE PROOF: the machine credential cannot record a decision as Jeff', async ({ request }) => {
    const r = await request.post(`${BRIDGE_URL}/api/message`, { headers: AUTH, data: { from: 'jeff', text: `DECISION: spoof ${Date.now()}` } });
    expect(r.status()).toBe(403);
  });

  base('DECISION-prefixed messages are captured with correct attribution', async ({ request }) => {
    const decisionText = `DECISION: Test decision from clearing ${Date.now()}`;

    await base.step('Send decision message', async () => {
      // #4417 — a decision in Jeff's name comes from Jeff's signed-in session;
      // the machine credential is refused when it claims a person.
      const response = await request.post(`${BRIDGE_URL}/api/message`, {
        headers: { cookie: `clearing_session=${CLEARING_TARGET.session()}` },
        data: { from: 'jeff', text: decisionText },
      });
      expect(response.status()).toBe(200);
    });

    await base.step('Decision appears in messages with correct sender', async () => {
      const response = await request.get(`${BRIDGE_URL}/api/messages`);
      const messages = await response.json();
      const decision = messages.find((m) => m.text === decisionText);
      expect(decision).toBeDefined();
      expect(decision.from).toBe('jeff');
    });
  });
});
