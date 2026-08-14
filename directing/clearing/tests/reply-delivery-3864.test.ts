// @test-type: unit — signal is fixture-data: pure hasher + emit shaping, no io
//
// #3864 — reply-delivery correlation, Clearing leg. The content hash is the
// cross-language join key with chorus-hooks' Rust hasher (reply.emitted at
// the Stop hook). config/reply-hash-fixtures.json is the contract: every
// case must produce the identical hash in both languages, or emitted/rendered
// pairs never join and every reply reads as a delivery gap. A missing fixture
// is a loud failure, never a skip (#3734).
import * as fs from 'fs';
import * as path from 'path';
import { contentHash, renderedEvent } from '../src/reply-delivery';

const FIXTURES = path.join(__dirname, '../../../config/reply-hash-fixtures.json');

describe('#3864 reply hash — shared fixture parity', () => {
  it('fixture exists and has cases (a deleted contract fails loud)', () => {
    const raw = fs.readFileSync(FIXTURES, 'utf8'); // throws loud if missing
    const cases = JSON.parse(raw).cases;
    expect(Array.isArray(cases)).toBe(true);
    expect(cases.length).toBeGreaterThan(0);
  });

  it('TS hash agrees with the shared fixture on every case', () => {
    const cases = JSON.parse(fs.readFileSync(FIXTURES, 'utf8')).cases as
      { name: string; text: string; hash: string }[];
    for (const c of cases) {
      expect({ name: c.name, hash: contentHash(c.text) })
        .toEqual({ name: c.name, hash: c.hash });
    }
  });

  // NEGATIVE PROOF (#3734): distinct replies must hash distinctly, or the
  // join silently correlates the wrong pair and hides a real gap.
  it('different texts hash differently', () => {
    expect(contentHash('reply one')).not.toBe(contentHash('reply two'));
  });

  // The whole reason canonicalization exists: the Rust extractor keeps the
  // header and joins blocks with newline; this tailer strips it and joins
  // with spaces. Same reply, different bytes, ONE hash.
  it('header and join-style differences canonicalize to the same hash', () => {
    const rustView = '--- Silas | 2026-08-13 | #3864 | Werk v1.6.0 ---\nline one\nline two';
    const tailerView = 'line one line two';
    expect(contentHash(rustView)).toBe(contentHash(tailerView));
  });
});

describe('#3864 reply.rendered event shape', () => {
  it('carries the join key, role, surface and hash of the SAME text', () => {
    const ev = renderedEvent('silas', '  hello Jeff  ');
    expect(ev.event).toBe('reply.rendered');
    expect(ev.role).toBe('silas');
    expect(ev.surface).toBe('clearing');
    expect(ev.hash).toBe(contentHash('hello Jeff')); // trim-equivalent
  });
});
