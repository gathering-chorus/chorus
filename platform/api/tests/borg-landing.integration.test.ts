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

// #3656: Quality Service reparented borg→werk — lives at /werk/quality/ now.
const SURFACES = [
  { slug: 'assessment',        title: 'Borg Assessment' },
  // #4031 removed the instance-explorer tile from /borg/ on purpose: it was a
  // twin of The Model row's Instance Explorer. The negative test below holds it out.
  { slug: 'patterns',          title: 'Interaction Patterns' },
  { slug: 'jeff',              title: 'Jeff Dashboard' },
  { slug: 'replay',            title: 'Session Replay' },
  { slug: 'fitness',           title: 'Fitness Functions' },
  { slug: 'cost',              title: 'Cost Dashboard' },
  { slug: 'hooks',             title: 'Hooks Dashboard' },
];

describe('#4031: the removed twin stays removed', () => {
  let harness: TestApp;
  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });

  test('landing does NOT list instance-explorer (nightly red 2026-09-02 was this test expecting it back)', async () => {
    const html = await (await fetch(`${harness.baseUrl}/borg/`)).text();
    expect(html.includes('/borg/instance-explorer')).toBe(false);
  });
});

describe('#2099: Borg landing at /borg/', () => {
  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });

  test('GET /borg/ returns 200', async () => {
    const res = await fetch(`${harness.baseUrl}/borg/`);
    expect(res.status).toBe(200);
  });

  test('landing lists all 7 surface slugs', async () => {
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

  test('landing links neither /borg/quality nor /werk/quality — Quality is reached from /werk (#3656, #4031)', async () => {
    const res = await fetch(`${harness.baseUrl}/borg/`);
    const html = await res.text();
    // #4031 — the landing no longer links out to Werk at all; Quality lives on
    // /werk and is reached from there. The rule that survives is: no /borg/quality/.
    expect(html).not.toContain('/werk/quality/');
    expect(html).not.toContain('/borg/quality');
  });
});
