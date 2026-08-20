// @test-type: unit
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

// #3904 — the live refusal shape. The old /^refused:/ anchor never matched
// "chorus_cards_move refused: ..." so contract-test refusals nudged ops on
// every nightly run (the 03:10 storms). These strings are copied VERBATIM
// from the 2026-08-16 03:10 spine events — the store is the fixture.
test('#3904: real refusal shape is suppressed (tool-name prefix)', () => {
  assert.equal(shouldNotifyOps(
    'chorus_cards_move refused: use-cards-done — status=Done must go through chorus_cards_done', '', false), false);
  assert.equal(shouldNotifyOps(
    'chorus_cards_set refused: no-status-changes — status is a transition, not a field.', '', false), false);
  assert.equal(shouldNotifyOps('Unknown tool: chorus_nonexistent_tool', '', false), false);
});

test('#3904 NEGATIVE PROOF: systemic failures still notify', () => {
  assert.equal(shouldNotifyOps('werk-deploy-fail — reason=work-fail exit=1', '', false), true);
  assert.equal(shouldNotifyOps('ECONNREFUSED localhost:3340', '', false), true);
  // "refused" as a bare word inside a systemic message must NOT suppress:
  assert.equal(shouldNotifyOps('spawn failed; connection refused by kernel', '', false), true);
});

// #3938 — Jeff's directive: routine word-cap bounces never broadcast into
// sessions. The refusal reaches its caller in the tool result; ops hears
// nothing. Spine event untouched (suppress the nudge, never the record).

test('#3938 word-cap bounce → no ops nudge', () => {
  assert.equal(shouldNotifyOps(
    'word-cap: 57 words, cap is 50. Rewrite shorter and resend — nothing was sent', '3872', false), false);
  assert.equal(shouldNotifyOps('word-cap: 104 words, cap is 100', '', false), false);
});

test('#3938 NEGATIVE: word-cap text MID-message still notifies (anchor is the guard)', () => {
  assert.equal(shouldNotifyOps(
    'werk-commit: commit failed: hook crashed while printing word-cap: 55 words, cap is 50', '3872', false), true);
});

test('#3938 NEGATIVE: real tool errors still notify — suppression is not a mute', () => {
  assert.equal(shouldNotifyOps('werk-merge-fail — reason=work-fail exit=1', '3911', false), true);
});
