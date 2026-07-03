// @test-type: unit — tmpdir fixture view/doc dirs; no gathering repo, no live services.
//
// #3606 — discover-pages-gathering.ts was 0% in the hermetic tier (its only
// exercise was the integration discover-pages test). The two scanners are
// pure directory classifiers — fixture dirs pin the rule table.
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { scanEjsViews, scanDocHtml } from '../src/discover-pages-gathering';

const ALIAS = { photos: 'photos-domain', music: 'music-domain', garden: 'garden-domain' };

let TMP: string;

beforeAll(() => {
  TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'discover-3606-'));
});
afterAll(() => fs.rmSync(TMP, { recursive: true, force: true }));

function mkfiles(dir: string, names: string[]): string {
  const abs = path.join(TMP, dir);
  fs.mkdirSync(abs, { recursive: true });
  for (const n of names) fs.writeFileSync(path.join(abs, n), '');
  return abs;
}

describe('scanEjsViews', () => {
  it('classifies collection / detail / admin / direct-alias views by the rule table', () => {
    const dir = mkfiles('views1', [
      'collection-photos.ejs',
      'music-detail.ejs',
      'admin-garden.ejs',
      'photos.ejs',
      'garden-map.ejs',
      'unrelated-thing.ejs',
      'notes.txt',
    ]);
    const entries = scanEjsViews(dir, ALIAS);
    const byRoute = Object.fromEntries(entries.map((e) => [e.route, e]));
    // collection-photos.ejs and photos.ejs both map to /photos — assert both
    // classifications exist rather than last-writer-wins in the route map.
    expect(entries.some((e) => e.route === '/photos' && e.pageType === 'collection')).toBe(true);
    expect(entries.some((e) => e.route === '/photos' && e.pageType === 'page')).toBe(true);
    expect(byRoute['/music/:slug']).toMatchObject({ pageType: 'detail', domainId: 'music-domain' });
    expect(byRoute['/admin/garden']).toMatchObject({ pageType: 'admin', domainId: 'garden-domain' });
    expect(byRoute['/garden-map']).toMatchObject({ pageType: 'page', domainId: 'garden-domain' }); // alias-prefix fallback
    // no alias → dropped; non-ejs → ignored
    expect(entries.some((e) => e.route.includes('unrelated'))).toBe(false);
    expect(entries.every((e) => e.path.startsWith('gathering/views/'))).toBe(true);
  });

  it('scans ontology-views subdir for alias-named views', () => {
    const dir = mkfiles('views2', []);
    mkfiles('views2/ontology-views', ['photos.ejs', 'nobody.ejs']);
    const entries = scanEjsViews(dir, ALIAS);
    expect(entries).toHaveLength(1);
    expect(entries[0]).toMatchObject({ route: '/ontology-views/photos', pageType: 'ontology', domainId: 'photos-domain' });
  });

  it('returns empty for a missing dir', () => {
    expect(scanEjsViews(path.join(TMP, 'nope'), ALIAS)).toEqual([]);
  });
});

describe('scanDocHtml', () => {
  it('classifies domain-* and *-service-design pages; drops unknown aliases', () => {
    const dir = mkfiles('docs1', [
      'domain-photos.html',
      'music-service-design.html',
      'domain-unknown.html',
      'random.html',
      'readme.md',
    ]);
    const entries = scanDocHtml(dir, ALIAS);
    const byRoute = Object.fromEntries(entries.map((e) => [e.route, e]));
    expect(byRoute['/gathering-docs/domain-photos.html']).toMatchObject({ pageType: 'doc', domainId: 'photos-domain' });
    expect(byRoute['/gathering-docs/music-service-design.html']).toMatchObject({ pageType: 'service-design', domainId: 'music-domain' });
    expect(entries).toHaveLength(2);
  });

  it('returns empty for a missing dir', () => {
    expect(scanDocHtml(path.join(TMP, 'nope'), ALIAS)).toEqual([]);
  });
});
