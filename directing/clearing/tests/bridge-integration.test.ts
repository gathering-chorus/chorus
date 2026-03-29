/**
 * Bridge Integration Tests — #1674 AC #2
 *
 * Tests what Jeff SEES on the Bridge, not internal state.
 *
 * AC:
 * 1. Message attribution: each message shows correct role name
 * 2. Delivery: messages reach the correct stream panel
 * 3. Filtering: role-to-role coordination not in Jeff's feed
 * 4. All 3 sessions: wren, silas, kade sessions all visible and distinct
 */

import { MessageRouter } from '../src/router';

// Helper: ingest and return the classified message
function ingestAndGet(router: MessageRouter, msg: { from: string; text: string; ts?: string; type?: string }) {
  const fullMsg = { ts: new Date().toISOString(), ...msg };
  router.ingest(fullMsg);
  return router.getRecent(100, true).slice(-1)[0];
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. MESSAGE ATTRIBUTION — Jeff sees the correct role name on every message
// ═══════════════════════════════════════════════════════════════════════════

describe('AC #2.1: Message attribution — correct role name on every message', () => {
  let router: MessageRouter;
  beforeEach(() => { router = new MessageRouter(); });

  test('Wren message shows from=wren', () => {
    const msg = ingestAndGet(router, {
      from: 'wren',
      text: 'Card #1674 is ready for your review.',
      type: 'role-response',
    });
    expect(msg.from).toBe('wren');
  });

  test('Silas message shows from=silas', () => {
    const msg = ingestAndGet(router, {
      from: 'silas',
      text: 'Deploy complete — health check green.',
      type: 'role-response',
    });
    expect(msg.from).toBe('silas');
  });

  test('Kade message shows from=kade', () => {
    const msg = ingestAndGet(router, {
      from: 'kade',
      text: 'Tests passing — 3855 green.',
      type: 'role-response',
    });
    expect(msg.from).toBe('kade');
  });

  test('Jeff input shows from=jeff', () => {
    const msg = ingestAndGet(router, {
      from: 'jeff',
      text: 'what cards are with you?',
    });
    expect(msg.from).toBe('jeff');
  });

  test('attribution persists through getRecent', () => {
    ingestAndGet(router, { from: 'wren', text: 'First', type: 'role-response' });
    ingestAndGet(router, { from: 'silas', text: 'Second', type: 'role-response' });
    ingestAndGet(router, { from: 'kade', text: 'Third', type: 'role-response' });

    const recent = router.getRecent(10);
    expect(recent.map(m => m.from)).toEqual(['wren', 'silas', 'kade']);
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 2. DELIVERY — messages reach Jeff's visible stream
// ═══════════════════════════════════════════════════════════════════════════

describe('AC #2.2: Delivery — messages reach the correct stream', () => {
  let router: MessageRouter;
  beforeEach(() => { router = new MessageRouter(); });

  test('role-response is visible in Jeff stream', () => {
    const msg = ingestAndGet(router, {
      from: 'silas',
      text: 'iPhone backup complete — 55K photos extracted.',
      type: 'role-response',
    });
    expect(msg.visible).toBe(true);
    expect(msg.type).toBe('role-response');
  });

  test('demo notification is visible in Jeff stream', () => {
    const msg = ingestAndGet(router, {
      from: 'wren',
      text: '[demo] #1674 — TDD discipline. Ready for review.',
    });
    expect(msg.visible).toBe(true);
    expect(msg.type).toBe('demo-ready');
  });

  test('blocked message is visible in Jeff stream', () => {
    const msg = ingestAndGet(router, {
      from: 'kade',
      text: 'blocked on Fuseki — service down.',
    });
    expect(msg.visible).toBe(true);
    expect(msg.type).toBe('blocked');
  });

  test('accept request is visible in Jeff stream', () => {
    const msg = ingestAndGet(router, {
      from: 'silas',
      text: '#1671 ready for accept — all tests green.',
    });
    expect(msg.visible).toBe(true);
    expect(msg.type).toBe('accept-request');
  });

  test('PM thinking is visible in Jeff stream', () => {
    const msg = ingestAndGet(router, {
      from: 'wren',
      text: 'Considering whether to pull #1675 or wait for Silas...',
      type: 'pm-thinking',
    });
    expect(msg.visible).toBe(true);
    expect(msg.type).toBe('pm-thinking');
  });

  test('Jeff input is always visible', () => {
    const msg = ingestAndGet(router, {
      from: 'jeff',
      text: 'show me the board',
    });
    expect(msg.visible).toBe(true);
    expect(msg.type).toBe('jeff-input');
  });

  test('system error is visible in Jeff stream', () => {
    const msg = ingestAndGet(router, {
      from: 'silas',
      text: 'LaunchAgent crashed — restarting.',
      type: 'system-error',
    });
    expect(msg.visible).toBe(true);
    expect(msg.type).toBe('system-error');
  });

  test('decision request is visible in Jeff stream', () => {
    const msg = ingestAndGet(router, {
      from: 'wren',
      text: '[decision] Should we pause harvesting until ICD validation is wired?',
    });
    expect(msg.visible).toBe(true);
  });

  test('gemba observation is visible with eye emoji', () => {
    const msg = ingestAndGet(router, {
      from: 'kade',
      text: '[gemba] Silas is fighting macOS permissions on the screenshot tool.',
    });
    expect(msg.visible).toBe(true);
    expect(msg.text).toContain('\u{1F441}'); // 👁
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 3. FILTERING — role-to-role coordination hidden from Jeff's feed
// ═══════════════════════════════════════════════════════════════════════════

describe('AC #2.3: Filtering — role-to-role coordination hidden from Jeff', () => {
  let router: MessageRouter;
  beforeEach(() => { router = new MessageRouter(); });

  test('[nudge from role] messages are hidden', () => {
    const msg = ingestAndGet(router, {
      from: 'silas',
      text: '[nudge from silas | 2026-03-24 14:01 Boston] [feedback] #1670 looks solid.',
    });
    expect(msg.visible).toBe(false);
    expect(msg.type).toBe('role-to-role');
  });

  test('[feedback] between roles is hidden', () => {
    const msg = ingestAndGet(router, {
      from: 'wren',
      text: '[feedback] #1676 — brief landed, auto-generation works.',
    });
    expect(msg.visible).toBe(false);
  });

  test('[reply] between roles is hidden', () => {
    const msg = ingestAndGet(router, {
      from: 'silas',
      text: '[reply] #1670 — you\'re right. The hook never fired.',
    });
    expect(msg.visible).toBe(false);
  });

  test('[ack] between roles is hidden', () => {
    const msg = ingestAndGet(router, {
      from: 'kade',
      text: '[ack] blast radius check clean — kade is waiting.',
    });
    expect(msg.visible).toBe(false);
  });

  test('[direction] between roles is hidden', () => {
    const msg = ingestAndGet(router, {
      from: 'wren',
      text: '[direction] Silas — pull #1675 next, not #1673.',
    });
    expect(msg.visible).toBe(false);
  });

  test('[correction] between roles is hidden', () => {
    const msg = ingestAndGet(router, {
      from: 'wren',
      text: '[correction] AC #3 needs WIP detection, not just delivery.',
    });
    expect(msg.visible).toBe(false);
  });

  test('bare ack/acknowledged from role is hidden', () => {
    const msg1 = ingestAndGet(router, { from: 'kade', text: 'ack — blast radius check clean.' });
    expect(msg1.visible).toBe(false);

    const router2 = new MessageRouter();
    const msg2 = ingestAndGet(router2, { from: 'silas', text: 'acknowledged — pulling now.' });
    expect(msg2.visible).toBe(false);
  });

  test('nudge delivery echo (arrow prefix) is hidden', () => {
    const msg = ingestAndGet(router, {
      from: 'silas',
      text: '\u2192 kade [feedback] #1671 looks solid.',
    });
    expect(msg.visible).toBe(false);
  });

  test('system noise with XML tags is hidden', () => {
    const msg = ingestAndGet(router, {
      from: 'silas',
      text: '<system-reminder>PostToolUse hook output</system-reminder>',
    });
    expect(msg.visible).toBe(false);
  });

  test('file paths are hidden', () => {
    const msg = ingestAndGet(router, {
      from: 'kade',
      text: '/Users/jeffbridwell/CascadeProjects/engineer/src/app.ts',
    });
    expect(msg.visible).toBe(false);
  });

  test('tool metadata suffixes are stripped from visible messages', () => {
    const msg = ingestAndGet(router, {
      from: 'jeff',
      text: 'show me the board | tools: none | 0.0s',
    });
    expect(msg.visible).toBe(true);
    expect(msg.text).toBe('show me the board');
    expect(msg.text).not.toContain('tools:');
  });

  test('visible messages still show after filtering hidden ones', () => {
    // Interleave visible and hidden messages
    ingestAndGet(router, { from: 'silas', text: '[nudge from silas] hidden nudge' });
    ingestAndGet(router, { from: 'wren', text: 'Card shipped!', type: 'role-response' });
    ingestAndGet(router, { from: 'kade', text: '[ack] got it' });
    ingestAndGet(router, { from: 'silas', text: 'Deploy complete.', type: 'role-response' });

    const visible = router.getRecent(10, false);
    expect(visible).toHaveLength(2);
    expect(visible[0].text).toBe('Card shipped!');
    expect(visible[1].text).toBe('Deploy complete.');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 4. ALL 3 SESSIONS — wren, silas, kade all visible and distinct
// ═══════════════════════════════════════════════════════════════════════════

describe('AC #2.4: All 3 sessions visible and distinct', () => {
  let router: MessageRouter;
  beforeEach(() => { router = new MessageRouter(); });

  test('messages from all 3 roles are distinct in the stream', () => {
    ingestAndGet(router, { from: 'wren', text: 'Board review complete.', type: 'role-response' });
    ingestAndGet(router, { from: 'silas', text: 'Hook deployed.', type: 'role-response' });
    ingestAndGet(router, { from: 'kade', text: 'Tests green.', type: 'role-response' });

    const visible = router.getRecent(10);
    expect(visible).toHaveLength(3);

    const roles = visible.map(m => m.from);
    expect(roles).toContain('wren');
    expect(roles).toContain('silas');
    expect(roles).toContain('kade');
    // All distinct
    expect(new Set(roles).size).toBe(3);
  });

  test('interleaved conversation preserves role order', () => {
    ingestAndGet(router, { from: 'jeff', text: 'what are you all working on?' });
    ingestAndGet(router, { from: 'wren', text: 'I have #1674 in WIP.', type: 'role-response' });
    ingestAndGet(router, { from: 'kade', text: 'Building test suites.', type: 'role-response' });
    ingestAndGet(router, { from: 'silas', text: 'Filtering nudges on #1675.', type: 'role-response' });

    const visible = router.getRecent(10);
    expect(visible).toHaveLength(4);
    expect(visible.map(m => m.from)).toEqual(['jeff', 'wren', 'kade', 'silas']);
  });

  test('deduplication prevents same message appearing twice', () => {
    ingestAndGet(router, { from: 'wren', text: 'Card shipped!' });
    ingestAndGet(router, { from: 'wren', text: 'Card shipped!' }); // duplicate

    const all = router.getRecent(10, true);
    const matching = all.filter(m => m.text === 'Card shipped!');
    expect(matching).toHaveLength(1);
  });

  test('hidden count tracks role-to-role messages since last visible', () => {
    ingestAndGet(router, { from: 'wren', text: 'Visible message.', type: 'role-response' });
    ingestAndGet(router, { from: 'silas', text: '[ack] got it' });
    ingestAndGet(router, { from: 'kade', text: '[reply] confirmed' });

    expect(router.getHiddenCount()).toBe(2);
  });
});
