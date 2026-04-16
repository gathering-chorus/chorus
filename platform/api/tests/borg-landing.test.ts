/**
 * Borg landing tests — #2099
 *
 * Borg front-end shaping surface: 9 observability/reflection pages migrated
 * from Gathering, served at 3340/borg/*. This suite covers the landing at
 * /borg/ — future suites cover per-page migrations.
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

const SURFACES = [
  { slug: 'assessment',        title: 'Borg Assessment' },
  { slug: 'instance-explorer', title: 'Instance Explorer' },
  { slug: 'patterns',          title: 'Interaction Patterns' },
  { slug: 'jeff',              title: 'Jeff Dashboard' },
  { slug: 'replay',            title: 'Session Replay' },
  { slug: 'quality',           title: 'Quality Service' },
  { slug: 'fitness',           title: 'Fitness Functions' },
  { slug: 'cost',              title: 'Cost Dashboard' },
  { slug: 'hooks',             title: 'Hooks Dashboard' },
];

describeIntegration('#2099: Borg landing at /borg/', () => {

  test('GET /borg/ returns 200', async () => {
    const res = await fetch(`${API}/borg/`);
    expect(res.status).toBe(200);
  }, 10_000);

  test('landing lists all 9 surface slugs', async () => {
    const res = await fetch(`${API}/borg/`);
    const html = await res.text();
    for (const s of SURFACES) {
      expect(html).toContain(`/borg/${s.slug}`);
    }
  }, 10_000);

  test('landing shows all 9 surface titles', async () => {
    const res = await fetch(`${API}/borg/`);
    const html = await res.text();
    for (const s of SURFACES) {
      expect(html).toContain(s.title);
    }
  }, 10_000);
});
