// @test-type: integration
// @domain: domains
/**
 * #4489 — athena-domain is retired into domains (Jeff 2026-10-10). The Athena
 * pages under public/athena/ now belong to the domains domain.
 */
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { scanAthenaHtml } from '../src/discover-pages-athena';

function dirWith(files: string[]): string {
  const d = fs.mkdtempSync(path.join(os.tmpdir(), 'athena-pages-4489-'));
  for (const f of files) fs.writeFileSync(path.join(d, f), '<html></html>');
  return d;
}

describe('#4489 — Athena pages belong to the domains domain', () => {
  it('tags each Athena page with domains', () => {
    const entries = scanAthenaHtml(dirWith(['domain-detail.html', 'notes.txt']), new Set(['domains']));
    expect(entries).toEqual([{
      route: '/athena/domain-detail.html',
      path: 'chorus/platform/api/public/athena/domain-detail.html',
      pageType: 'athena',
      domainId: 'domains',
    }]);
  });

  // NEGATIVE PROOF: the retired name no longer admits the pages
  it('NEGATIVE PROOF — a valid set with only the retired athena-domain tags nothing', () => {
    expect(scanAthenaHtml(dirWith(['domain-detail.html']), new Set(['athena-domain']))).toEqual([]);
  });

  it('a missing folder tags nothing', () => {
    expect(scanAthenaHtml('/nonexistent/athena', new Set(['domains']))).toEqual([]);
  });
});
