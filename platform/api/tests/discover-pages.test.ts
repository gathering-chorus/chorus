/**
 * Discover UI pages per domain — #2065
 *
 * Integration tests — hit live Chorus API at localhost:3340.
 * Tests verify that discover-pages scans views/public and maps to domains.
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('Discover pages (#2065)', () => {

  test('POST /api/athena/discover-pages returns page count > 0', async () => {
    const res = await fetch(`${API}/api/athena/discover-pages`, { method: 'POST' });
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.count).toBeGreaterThan(0);
  }, 30_000);

  test('discovered pages have route, path, and type fields', async () => {
    const res = await fetch(`${API}/api/athena/discover-pages`, { method: 'POST' });
    const body = await res.json();
    const entries = body.data?.entries || [];
    expect(entries.length).toBeGreaterThan(0);
    const page = entries[0];
    expect(page).toHaveProperty('route');
    expect(page).toHaveProperty('path');
    expect(page).toHaveProperty('pageType');
    expect(page).toHaveProperty('domainId');
  }, 30_000);

  test('paths include gathering/ project prefix', async () => {
    const res = await fetch(`${API}/api/athena/discover-pages`, { method: 'POST' });
    const body = await res.json();
    const entries = body.data?.entries || [];
    const withPrefix = entries.filter((e: any) => e.path.startsWith('gathering/'));
    expect(withPrefix.length).toBe(entries.length);
  }, 30_000);

  test('collection-music.ejs maps to music-domain with type collection', async () => {
    const res = await fetch(`${API}/api/athena/discover-pages`, { method: 'POST' });
    const body = await res.json();
    const entries = body.data?.entries || [];
    const musicCollection = entries.find((e: any) =>
      e.path.includes('collection-music') && e.domainId === 'music-domain'
    );
    expect(musicCollection).toBeDefined();
    expect(musicCollection.pageType).toBe('collection');
  }, 30_000);

  test('GET /api/athena/subdomains/:id/pages returns pages for populated domain', async () => {
    // First discover to populate
    await fetch(`${API}/api/athena/discover-pages`, { method: 'POST' });
    // Then query
    const res = await fetch(`${API}/api/athena/subdomains/music-domain/pages`);
    expect(res.status).toBe(200);
    const body = await res.json();
    const pages = body.data?.pages || [];
    expect(pages.length).toBeGreaterThan(0);
    expect(pages[0]).toHaveProperty('route');
  }, 30_000);
});
