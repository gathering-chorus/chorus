// #3335 — shouldNotifyOps: which mcp.tool.error events nudge ops (silas) vs stay
// spine-only. The synthetic-trace guard (Pattern 1) + the anchored-regex fix (Pattern 9).
// Pure predicate, so no HTTP/fetch — just the decision. node:test/tsx (the mcp-server runner).

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { shouldNotifyOps } from '../src/server';

// --- Pattern 1: synthetic/test traffic must NOT nudge ops (suppress nudge, keep event) ---

test('CHORUS_SYNTHETIC caller → no ops nudge (test traffic, #3329 class)', () => {
  assert.equal(shouldNotifyOps('something exploded', '', true), false);
});

test('synthetic card-id sentinel → no ops nudge', () => {
  assert.equal(shouldNotifyOps('real systemic failure', '99999', false), false);
  assert.equal(shouldNotifyOps('real systemic failure', '99998', false), false);
});

// --- a genuine systemic failure from a real card DOES nudge ops ---

test('real systemic error, real card, not synthetic → nudges ops', () => {
  assert.equal(shouldNotifyOps('something exploded', '3335', false), true);
});

// --- #3022: caller-side validation/refusals stay suppressed (their own bad call) ---

test('caller validation/refusal at message start → suppressed', () => {
  assert.equal(shouldNotifyOps('Invalid arguments: card_id required', '3335', false), false);
  assert.equal(shouldNotifyOps('refused: wrong-status', '3335', false), false);
  assert.equal(shouldNotifyOps('expected one of kade|wren|silas', '3335', false), false);
});

// --- Pattern 9: the anchored-regex fix — a REAL error merely CONTAINING those words
//     mid-message must NOT be over-suppressed ---

test('Pattern 9: a real error containing "refused:"/"expected one of" mid-message is NOT suppressed', () => {
  assert.equal(shouldNotifyOps('fuseki connection refused: ECONNREFUSED', '3335', false), true);
  assert.equal(shouldNotifyOps('upstream said expected one of the shards to respond', '3335', false), true);
});
