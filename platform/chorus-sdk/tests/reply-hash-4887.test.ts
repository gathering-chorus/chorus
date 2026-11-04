// @test-type: unit — signal is fixture-data: the cross-language hash contract
// @domain: messages
// @card: #4887 (moved from Clearing's #3864 test when reply-delivery.ts retired)
// @owner: silas
//
// config/reply-hash-fixtures.json is the one join key: chorus-hooks' Rust
// content_hash (tests/reply_delivery_3864.rs) and this TS contentHash must agree
// on every case, or the gap watch cannot match a room row to its reply.emitted.
import * as fs from 'fs';
import * as path from 'path';
import { contentHash } from '../reply/hash';

const FIXTURES = path.join(__dirname, '../../../config/reply-hash-fixtures.json');

describe('#4887 reply hash — shared fixture parity', () => {
  const cases = JSON.parse(fs.readFileSync(FIXTURES, 'utf8')).cases as { name: string; text: string; hash: string }[];

  it('fixture has cases (a deleted contract fails loud)', () => {
    expect(cases.length).toBeGreaterThan(0);
  });

  it('TS hash agrees with the shared fixture on every case', () => {
    for (const c of cases) expect({ name: c.name, hash: contentHash(c.text) }).toEqual({ name: c.name, hash: c.hash });
  });

  // NEGATIVE PROOF (#3734): distinct replies hash distinctly.
  it('different texts hash differently', () => {
    expect(contentHash('reply one')).not.toBe(contentHash('reply two'));
  });

  // Same reply, different bytes (Rust keeps the header and joins with newline), ONE hash.
  it('header and join-style differences canonicalize to the same hash', () => {
    expect(contentHash('--- Silas | 2026-08-13 | #3864 | Werk v1.6.0 ---\nline one\nline two')).toBe(contentHash('line one line two'));
  });
});
