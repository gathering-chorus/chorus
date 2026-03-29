#!/usr/bin/env node
/**
 * Router classification tests for #1675 — role-to-role nudge filtering
 * Tests what Jeff SEES, not internal state.
 *
 * AC:
 * 1. Nudges between roles show sender role+emoji, not Jeff's identity
 * 2. Only Jeff-targeted nudges appear in Jeff's stream
 * 3. Role-to-role coordination visually distinct from Jeff's conversation
 */

// Import the compiled router
import { MessageRouter } from './dist/router.js';

let pass = 0;
let fail = 0;

function check(name, result) {
  if (result) { console.log(`  PASS: ${name}`); pass++; }
  else { console.log(`  FAIL: ${name}`); fail++; }
}

function ingestAndGet(router, msg) {
  router.ingest(msg);
  // Get all messages including hidden
  return router.getRecent(100, true).slice(-1)[0];
}

console.log('=== Router Classification Tests (#1675) ===\n');

// --- AC 1: Role-to-role nudges show sender identity ---
console.log('--- AC 1: Role-to-role nudges show sender, not Jeff ---');
{
  const r = new MessageRouter();

  // Silas nudges Kade — should show "silas" as sender, not "jeff"
  const msg = ingestAndGet(r, {
    from: 'silas',
    text: '[nudge from silas | 2026-03-24 14:01 Boston] [feedback] #1670 looks solid.',
    ts: new Date().toISOString(),
  });
  check('Role-to-role nudge preserves sender identity (from=silas)', msg.from === 'silas');
  check('Role-to-role nudge is hidden from Jeff stream', msg.visible === false);
}
{
  const r = new MessageRouter();

  // Wren nudges Silas with feedback
  const msg = ingestAndGet(r, {
    from: 'wren',
    text: '[feedback] #1676 — brief landed, auto-generation works.',
    ts: new Date().toISOString(),
  });
  check('[feedback] between roles is hidden', msg.visible === false);
  check('[feedback] preserves sender (from=wren)', msg.from === 'wren');
}
{
  const r = new MessageRouter();

  // Kade acks a nudge
  const msg = ingestAndGet(r, {
    from: 'kade',
    text: 'ack — blast radius check clean.',
    ts: new Date().toISOString(),
  });
  check('Role ack is hidden from Jeff stream', msg.visible === false);
}

// --- AC 2: Only Jeff-targeted nudges in Jeff's stream ---
console.log('\n--- AC 2: Jeff-targeted nudges visible, role-to-role hidden ---');
{
  const r = new MessageRouter();

  // Role nudges Jeff directly (via Bridge /api/message)
  const msg = ingestAndGet(r, {
    from: 'silas',
    text: 'iPhone backup complete — 55K photos extracted.',
    ts: new Date().toISOString(),
    type: 'role-response',
  });
  check('Role response to Jeff is visible', msg.visible === true);
}
{
  const r = new MessageRouter();

  // Demo nudge — always visible (Jeff needs to see demos)
  const msg = ingestAndGet(r, {
    from: 'silas',
    text: '[demo] #1671 — accept-gate hook. Ready for review.',
    ts: new Date().toISOString(),
  });
  check('[demo] nudge is visible to Jeff', msg.visible === true);
  check('[demo] type is demo-ready', msg.type === 'demo-ready');
}
{
  const r = new MessageRouter();

  // Blocked — always visible
  const msg = ingestAndGet(r, {
    from: 'kade',
    text: 'blocked on Fuseki — service down.',
    ts: new Date().toISOString(),
  });
  check('Blocked message visible to Jeff', msg.visible === true);
}
{
  const r = new MessageRouter();

  // Role-to-role [reply] — hidden
  const msg = ingestAndGet(r, {
    from: 'silas',
    text: '[reply] #1670 — you\'re right. The hook never fired.',
    ts: new Date().toISOString(),
  });
  check('[reply] between roles is hidden', msg.visible === false);
}
{
  const r = new MessageRouter();

  // Role-to-role [ack] — hidden
  const msg = ingestAndGet(r, {
    from: 'kade',
    text: '[ack] blast radius check clean — kade is waiting.',
    ts: new Date().toISOString(),
  });
  check('[ack] between roles is hidden', msg.visible === false);
}

// --- AC 3: Role-to-role visually distinct (type classification) ---
console.log('\n--- AC 3: Role-to-role typed as role-to-role ---');
{
  const r = new MessageRouter();

  const nudge = ingestAndGet(r, {
    from: 'wren',
    text: '[nudge from wren | 2026-03-24 14:12 Boston] [feedback] #1676 — agreed.',
    ts: new Date().toISOString(),
  });
  check('Nudge classified as role-to-role type', nudge.type === 'role-to-role');

  const response = ingestAndGet(r, {
    from: 'silas',
    text: 'Accepted #1671 — committed and pushed.',
    ts: new Date().toISOString(),
    type: 'role-response',
  });
  check('Jeff-facing response classified as role-response', response.type === 'role-response');
}

// --- Edge cases ---
console.log('\n--- Edge Cases ---');
{
  const r = new MessageRouter();

  // Jeff's own input — always visible
  const msg = ingestAndGet(r, {
    from: 'jeff',
    text: 'what cards are with you?',
    ts: new Date().toISOString(),
  });
  check('Jeff input is always visible', msg.visible === true);
  check('Jeff input typed as jeff-input', msg.type === 'jeff-input');
}
{
  const r = new MessageRouter();

  // Nudge delivery echo "→ target message" — hidden (system noise)
  const msg = ingestAndGet(r, {
    from: 'silas',
    text: '→ kade [feedback] #1671 looks solid.',
    ts: new Date().toISOString(),
  });
  check('Nudge delivery echo (→) is hidden', msg.visible === false);
}
{
  const r = new MessageRouter();

  // PM thinking — always visible
  const msg = ingestAndGet(r, {
    from: 'wren',
    text: 'Considering whether to pull #1675 or wait for Silas...',
    ts: new Date().toISOString(),
    type: 'pm-thinking',
  });
  check('PM thinking is visible', msg.visible === true);
}

// --- Summary ---
console.log(`\n=== Results: ${pass} pass, ${fail} fail ===`);
process.exit(fail > 0 ? 1 : 0);
