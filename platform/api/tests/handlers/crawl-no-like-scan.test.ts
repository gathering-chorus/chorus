// #3054 AC1 — the crawl handler must NOT scan messages with `content LIKE '%q%'`
// (unindexed full scan over 1.26M rows, ~2.8s sync, freezes the spine — the same
// #3051 class). collectMentions uses FTS5 MATCH instead. Red before the swap.

import { readFileSync } from 'fs';
import { join } from 'path';

test('chorus-crawl.ts has no `content LIKE` full-scan (#3054 AC1)', () => {
  const src = readFileSync(join(__dirname, '../../src/handlers/chorus-crawl.ts'), 'utf-8');
  const hits = (src.match(/content LIKE/g) || []).length;
  expect(hits).toBe(0);
});
