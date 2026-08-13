// @test-type: unit — signal is fixture-data: config/word-cap-fixtures.json + injected fetch/clock, no live store
// #3818 — the word cap. Jeff, 2026-08-11: "seriously contemplating a hard cap of
// 100 words on all agent responses" — "including chats between u."
//
// Two surfaces enforce it in two languages: this send-path gate (TS) and Kade's
// Stop-hook leg (Rust). They cannot share a module, so they share a FIXTURE:
// config/word-cap-fixtures.json. Both run it. Two counters that disagree would
// bounce Jeff on one surface and not the other, which reads as a broken cap.
import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import * as fs from 'fs';
import * as path from 'path';
import {
  countWords, resolveCap, checkCap, FAIL_OPEN_CAP, CAP_TTL_MS, __clearCapCache,
} from '../src/word-cap';

const FIXTURE = JSON.parse(
  fs.readFileSync(path.join(__dirname, '../../../config/word-cap-fixtures.json'), 'utf8'),
) as { cases: Array<{ name: string; text: string; expect: number }> };

test('the shared fixture has cases — an emptied fixture would pass every case below vacuously', () => {
  assert.ok(FIXTURE.cases.length > 10, `only ${FIXTURE.cases.length} cases`);
});

for (const c of FIXTURE.cases) {
  test(`countWords: ${c.name}`, () => {
    assert.equal(countWords(c.text), c.expect);
  });
}

test('checkCap passes at exactly the cap — the boundary is inclusive', () => {
  assert.equal(checkCap('one two three', 3).ok, true);
});

// NEGATIVE PROOF (#3734): the gate must refuse an actual violation.
test('checkCap REJECTS one word over and names the count', () => {
  const v = checkCap('one two three four', 3);
  assert.equal(v.ok, false);
  if (v.ok) return;
  assert.equal(v.words, 4);
  assert.equal(v.cap, 3);
  assert.match(v.message, /4 words/);
});

test('the rejection says nothing was sent and nothing truncated', () => {
  const v = checkCap('a b c d', 1);
  if (v.ok) throw new Error('expected rejection');
  assert.match(v.message, /nothing was sent/);
  assert.match(v.message, /nothing was truncated/);
});

test('a long fenced receipt does not push a short message over the cap', () => {
  const fence = '```\n' + 'x '.repeat(500) + '\n```';
  assert.equal(checkCap(`short note\n${fence}`, 10).ok, true);
});

const okResp = (value: unknown) => ({ ok: true, json: async () => ({ value }) });

test('resolveCap returns the configured value', async () => {
  __clearCapCache();
  assert.equal(await resolveCap('wren', { fetchImpl: async () => okResp(42), now: () => 0 }), 42);
});

// NEGATIVE PROOFS for Kade's catch: the graph is now on the send path, so a
// dead store must never mute the team (the #3218 lockout class).
test('a thrown fetch fails OPEN, not closed', async () => {
  __clearCapCache();
  const cap = await resolveCap('wren', {
    fetchImpl: async () => { throw new Error('ECONNREFUSED'); }, now: () => 0,
  });
  assert.equal(cap, FAIL_OPEN_CAP);
});

test('a non-ok response fails open', async () => {
  __clearCapCache();
  const cap = await resolveCap('wren', {
    fetchImpl: async () => ({ ok: false, json: async () => ({}) }), now: () => 0,
  });
  assert.equal(cap, FAIL_OPEN_CAP);
});

test('a missing property fails open', async () => {
  __clearCapCache();
  assert.equal(await resolveCap('wren', { fetchImpl: async () => okResp(undefined), now: () => 0 }), FAIL_OPEN_CAP);
});

test('a cap of 0 is refused — a fat-fingered value must not mute the team', async () => {
  __clearCapCache();
  assert.equal(await resolveCap('wren', { fetchImpl: async () => okResp(0), now: () => 0 }), FAIL_OPEN_CAP);
});

test('caches within the TTL — the store is not on the hot path of every message', async () => {
  __clearCapCache();
  let calls = 0;
  const deps = { fetchImpl: async () => { calls++; return okResp(7); }, now: () => 0 };
  await resolveCap('wren', deps);
  await resolveCap('wren', deps);
  assert.equal(calls, 1);
});

test('re-reads after the TTL — so editing the property actually takes effect', async () => {
  __clearCapCache();
  let calls = 0;
  let t = 0;
  const deps = { fetchImpl: async () => { calls++; return okResp(7); }, now: () => t };
  await resolveCap('wren', deps);
  t = CAP_TTL_MS + 1;
  await resolveCap('wren', deps);
  assert.equal(calls, 2);
});

test('caches per role — one role\'s cap never answers for another', async () => {
  __clearCapCache();
  assert.equal(await resolveCap('wren', { fetchImpl: async () => okResp(10), now: () => 0 }), 10);
  assert.equal(await resolveCap('kade', { fetchImpl: async () => okResp(20), now: () => 0 }), 20);
});
