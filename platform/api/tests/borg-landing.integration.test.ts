// @test-type: integration — in-process TestApp harness (startTestApp); no live services.
/**
 * Borg landing tests — #2099
 *
 * Borg front-end shaping surface: 9 observability/reflection pages migrated
 * from Gathering, served at 3340/borg/*. This suite covers the landing at
 * /borg/ — future suites cover per-page migrations.
 *
 * Converted to in-process harness (#2173 AC4).
 */

import { startTestApp, type TestApp } from './lib/test-app';

// #3656: Quality Service reparented borg→loom — lives at /loom/quality/ now.
const SURFACES = [
  { slug: 'assessment',        title: 'Borg Assessment' },
  { slug: 'instance-explorer', title: 'Instance Explorer' },
  { slug: 'patterns',          title: 'Interaction Patterns' },
  { slug: 'jeff',              title: 'Jeff Dashboard' },
  { slug: 'replay',            title: 'Session Replay' },
  { slug: 'fitness',           title: 'Fitness Functions' },
  { slug: 'cost',              title: 'Cost Dashboard' },
  { slug: 'hooks',             title: 'Hooks Dashboard' },
];

describe('#2099: Borg landing at /borg/', () => {
  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });

  test('GET /borg/ returns 200', async () => {
    const res = await fetch(`${harness.baseUrl}/borg/`);
    expect(res.status).toBe(200);
  });

  test('landing lists all 8 surface slugs', async () => {
    const res = await fetch(`${harness.baseUrl}/borg/`);
    const html = await res.text();
    for (const s of SURFACES) {
      expect(html).toContain(`/borg/${s.slug}`);
    }
  });

  test('landing shows all 8 surface titles', async () => {
    const res = await fetch(`${harness.baseUrl}/borg/`);
    const html = await res.text();
    for (const s of SURFACES) {
      expect(html).toContain(s.title);
    }
  });

  test('landing points Quality Service at its loom home, not /borg/quality (#3656)', async () => {
    const res = await fetch(`${harness.baseUrl}/borg/`);
    const html = await res.text();
    expect(html).toContain('/loom/quality/');
    expect(html).not.toContain('/borg/quality');
  });
});
