// #3268 — the priorities readout grouper. Pure logic: board rows (owner/chunk
// labels) → role(hard-rank) → chunk → cards, with proving cross-cut, prune for
// Gathering, and HONEST untagged (AC4: never fabricate placement).
import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { groupPrioritiesReadout, renderPrioritiesReadout } from '../src/server';

test('groups by role hard-rank → chunk → cards', () => {
  const rows = [
    { idx: 100, title: 'werk demo fix', bucket: 'WIP', labels: 'owner:kade,chunk:werk-demo' },
    { idx: 101, title: 'model adr', bucket: 'Next', labels: 'owner:silas,chunk:model' },
    { idx: 102, title: 'loose wren card', bucket: 'Later', labels: 'owner:wren' },
    { idx: 103, title: 'gathering photo', bucket: 'Later', labels: 'owner:kade,sequence:gathering' },
    { idx: 104, title: 'proving loop', bucket: 'Later', labels: 'owner:silas,chunk:proving' },
  ];
  const r = groupPrioritiesReadout(rows);

  // AC1: roles in hard rank order
  assert.deepEqual(r.roles.map((x) => x.role), ['kade', 'silas', 'wren']);
  assert.deepEqual(r.roles.map((x) => x.rank), [1, 2, 3]);

  // AC2: cards land under their chunk (from the chunk attribute)
  assert.equal(r.roles[0].chunks[0].chunk, 'werk-demo');
  assert.equal(r.roles[0].chunks[0].cards[0].id, 100);
  assert.equal(r.roles[1].chunks[0].chunk, 'model');

  // AC3: proving cross-cut + prune (Gathering) separated, not double-placed
  assert.equal(r.proving.length, 1);
  assert.equal(r.proving[0].id, 104);
  assert.equal(r.prune.length, 1);
  assert.equal(r.prune[0].id, 103);
  // #104 is proving-only → NOT also under silas's chunks or untagged
  assert.equal(r.roles[1].chunks.length, 1);
  assert.equal(r.roles[1].untagged.length, 0);

  // AC4: a chorus card with no chunk is honestly UNTAGGED, not invented under a priority
  assert.equal(r.roles[2].untagged.length, 1);
  assert.equal(r.roles[2].untagged[0].id, 102);
  assert.equal(r.roles[2].chunks.length, 0);
});

test('AC4: degrades honestly when chunks are unset', () => {
  const r = groupPrioritiesReadout([
    { idx: 1, title: 'untagged a', bucket: 'Later', labels: 'owner:wren' },
    { idx: 2, title: 'untagged b', bucket: 'Next', labels: 'owner:kade' },
  ]);
  assert.equal(r.totals.chunked, 0);
  assert.equal(r.totals.untagged, 2);
});

test('a card with two chunks appears under each', () => {
  const r = groupPrioritiesReadout([
    { idx: 9, title: 'two', bucket: 'Later', labels: 'owner:wren,chunk:loom-authoring,chunk:memory' },
  ]);
  const wren = r.roles.find((x) => x.role === 'wren')!;
  assert.deepEqual(wren.chunks.map((c) => c.chunk).sort(), ['loom-authoring', 'memory']);
});

test('renderPrioritiesReadout: chunked-only report (chunk → - #id - title); no untagged, no prune', () => {
  const r = groupPrioritiesReadout([
    { idx: 100, title: 'werk demo fix', bucket: 'WIP', labels: 'owner:kade,chunk:werk-demo' },
    { idx: 101, title: 'model adr', bucket: 'Next', labels: 'owner:silas,chunk:model' },
    { idx: 102, title: 'loose wren card', bucket: 'Later', labels: 'owner:wren' },          // untagged
    { idx: 103, title: 'gathering photo', bucket: 'Later', labels: 'owner:kade,sequence:gathering' }, // prune
    { idx: 104, title: 'proving loop', bucket: 'Later', labels: 'owner:silas,chunk:proving' },
  ]);
  const out = renderPrioritiesReadout(r);
  // role headings in hard rank order
  assert.match(out, /1 KADE/);
  assert.match(out, /2 SILAS/);
  assert.match(out, /3 WREN/);
  assert.ok(out.indexOf('1 KADE') < out.indexOf('2 SILAS'));
  assert.ok(out.indexOf('2 SILAS') < out.indexOf('3 WREN'));
  // chunk heading + fixed card line: "   - #id - title"
  assert.match(out, /werk-demo\n   - #100 - werk demo fix/);
  assert.match(out, /model\n   - #101 - model adr/);
  // proving (chunk:proving) is included — it's a real chunk
  assert.match(out, /proving\n   - #104 - proving loop/);
  // NO untagged: the unchunked card #102 and the word "untagged" never appear
  assert.ok(!out.includes('#102'), 'untagged card must not appear');
  assert.ok(!out.includes('untagged'), 'no untagged section');
  // NO prune: the gathering card #103 and the word "prune" never appear
  assert.ok(!out.includes('#103'), 'prune/gathering card must not appear');
  assert.ok(!out.includes('prune'), 'no prune section');
  // determinism: same input → identical output
  assert.equal(renderPrioritiesReadout(groupPrioritiesReadout([
    { idx: 100, title: 'werk demo fix', bucket: 'WIP', labels: 'owner:kade,chunk:werk-demo' },
  ])), renderPrioritiesReadout(groupPrioritiesReadout([
    { idx: 100, title: 'werk demo fix', bucket: 'WIP', labels: 'owner:kade,chunk:werk-demo' },
  ])));
});

test('unassigned / jeff cards are not forced into the hard rank', () => {
  const r = groupPrioritiesReadout([
    { idx: 5, title: 'jeff card', bucket: 'Later', labels: 'owner:jeff' },
  ]);
  assert.equal(r.totals.untagged, 0);
  assert.equal(r.roles.every((x) => x.chunks.length === 0 && x.untagged.length === 0), true);
});
