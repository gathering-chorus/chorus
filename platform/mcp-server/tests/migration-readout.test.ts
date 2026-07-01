// @test-type: unit
// #3594 — the V1→V2 migration readout: per-DOMAIN stage matrix (rebuilt from the
// per-kind counter, which answered the wrong question — Jeff 2026-07-01). Pure logic:
// raw graph/source facts → rows → fixed matrix text. The load-bearing invariant is
// HONESTY: owl-api-run / v1-code-retired are never faked (render as '—'), and `done`
// is never done while a stage is unproven.
import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { computeMigrationRows, renderMigrationReadout } from '../src/server';

test('computeMigrationRows: graph+source stages real; owl-api/v1-code not-instrumented; done honest-false', () => {
  const rows = computeMigrationRows({
    domains: ['search', 'code'], // unsorted input → compute sorts
    srcDomains: ['code'], // only code is declared in committed source
    subdomainStems: ['code'], // code still has a V1 twin; search does not
  });
  // deterministic sort
  assert.deepEqual(rows.map((r) => r.domain), ['code', 'search']);

  const code = rows.find((r) => r.domain === 'code')!;
  const search = rows.find((r) => r.domain === 'search')!;

  // graph-derivable: created + loaded always true for a domain in the graph
  assert.equal(code.created, true);
  assert.equal(code.loaded, true);
  // source-derivable owl-src
  assert.equal(code.owlSrc, true);
  assert.equal(search.owlSrc, false);
  // v1-retired: false when a V1 twin lingers, true when its name is gone
  assert.equal(code.v1Retired, false);
  assert.equal(search.v1Retired, true);

  // HONESTY invariant 1: the two uninstrumented stages are null on EVERY row, never faked
  assert.equal(rows.every((r) => r.owlApi === null && r.v1Code === null), true);
  // HONESTY invariant 2: done is never true while stages are unproven
  assert.equal(rows.every((r) => r.done === false), true);
});

test('renderMigrationReadout: per-domain matrix — honest cells, real counts, deterministic', () => {
  const rows = computeMigrationRows({ domains: ['code', 'search'], srcDomains: ['code'], subdomainStems: ['code'] });
  const out = renderMigrationReadout(rows);

  // header + column row for the 7 stages
  assert.match(out, /per-domain stage matrix/);
  assert.match(out, /domain +created owl-src loaded owl-api v1-retired v1-code done/);
  // both domains appear as rows
  assert.match(out, /\n {2}code /);
  assert.match(out, /\n {2}search /);
  // not-yet-instrumented rendered as — (never a ✓/·)
  assert.ok(out.includes('—'), 'uninstrumented columns render as —');
  // no data row claims done=✓ (every rendered row ends in the done cell '·', never '✓')
  const rowLines = out.split('\n').filter((l) => l.startsWith('  code ') || l.startsWith('  search '));
  assert.equal(rowLines.length, 2);
  assert.equal(rowLines.every((l) => l.trimEnd().endsWith('·')), true);
  // summary line reflects the REAL counts (created 2/2, owl-src 1/2, v1-retired 1/2)
  assert.match(out, /2 domains — created 2\/2 · owl-src 1\/2 · loaded 2\/2 · v1-retired 1\/2/);
  // legend names the uninstrumented stages explicitly
  assert.match(out, /not-yet-instrumented \(owl-api-run, v1-code-retired\)/);

  // determinism: identical input → identical output
  const render = (): string =>
    renderMigrationReadout(computeMigrationRows({ domains: ['x'], srcDomains: [], subdomainStems: [] }));
  assert.equal(render(), render());
});

test('empty graph degrades honestly: zero rows, 0/0 counts, no crash', () => {
  const out = renderMigrationReadout(computeMigrationRows({ domains: [], srcDomains: [], subdomainStems: [] }));
  assert.match(out, /0 domains — created 0\/0/);
});
