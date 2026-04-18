/**
 * Performance budget tests — #1777
 *
 * Validates real-world response times against measured baselines. Stays
 * RUN_INTEGRATION-gated (not converted to the in-process harness in #2173
 * AC4) because perf is environmental: the numbers only mean something
 * against the live chorus-api service under real Fuseki/SQLite load, not
 * against an in-process jest worker. Opt in with:
 *
 *   RUN_INTEGRATION=true CHORUS_API=http://localhost:3340 npx jest perf-budget
 *
 * Complements Silas's nightly perf-baseline-chorus.sh (#1914) which runs
 * from a LaunchAgent.
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';
const APP = process.env.GATHERING_APP || 'http://localhost:3000';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

const SD = 'chorus-domain';

const ATHENA_TOP_LEVEL: Array<[string, string, number]> = [
  ['athena health', `${API}/api/athena/health`, 100],
  ['athena products', `${API}/api/athena/products`, 500],
  ['athena subproducts', `${API}/api/athena/subproducts`, 100],
  ['athena subdomains', `${API}/api/athena/subdomains`, 200],
  ['athena steps', `${API}/api/athena/steps`, 100],
  ['athena owners', `${API}/api/athena/owners`, 100],
  ['athena machines', `${API}/api/athena/machines`, 100],
  ['athena validate', `${API}/api/athena/validate`, 200],
];

const ATHENA_SUBDOMAIN: Array<[string, string, number]> = [
  ['subdomain detail', `${API}/api/athena/subdomains/${SD}`, 100],
  ['completeness', `${API}/api/athena/subdomains/${SD}/completeness`, 100],
  ['blast radius', `${API}/api/athena/subdomains/${SD}/blast-radius`, 200],
  ['cards', `${API}/api/athena/subdomains/${SD}/cards`, 1000],
  ['alerts', `${API}/api/athena/subdomains/${SD}/alerts`, 100],
  ['code', `${API}/api/athena/subdomains/${SD}/code`, 100],
  ['coverage', `${API}/api/athena/subdomains/${SD}/coverage`, 100],
  ['test coverage', `${API}/api/athena/subdomains/${SD}/test-coverage`, 100],
  ['pages', `${API}/api/athena/subdomains/${SD}/pages`, 100],
  ['services', `${API}/api/athena/subdomains/${SD}/services`, 100],
  ['actors', `${API}/api/athena/subdomains/${SD}/actors`, 100],
  ['scenarios', `${API}/api/athena/subdomains/${SD}/scenarios`, 100],
  ['contract', `${API}/api/athena/subdomains/${SD}/contract`, 100],
  ['integrations', `${API}/api/athena/subdomains/${SD}/integrations`, 100],
  ['persistence', `${API}/api/athena/subdomains/${SD}/persistence`, 100],
];

const DOMAIN_DETAIL: Array<[string, string, number]> = [
  ['domain detail (chorus)', `${API}/api/chorus/domain/chorus`, 1000],
  ['domain detail (seeds)', `${API}/api/chorus/domain/seeds`, 1000],
  ['domain detail (music)', `${API}/api/chorus/domain/music`, 1000],
  ['domain detail (photos)', `${API}/api/chorus/domain/photos`, 1000],
];

const CHORUS_API: Array<[string, string, number]> = [
  ['chorus health', `${API}/api/chorus/health`, 100],
  ['chorus search', `${API}/api/chorus/search?q=test&limit=5`, 1500],
  ['chorus rcas', `${API}/api/chorus/rcas`, 200],
];

const PAGE_BUDGETS: Array<[string, string, number]> = [
  ['app root', `${APP}/`, 500],
  ['domain detail page', `${APP}/gathering-docs/domain-detail.html?id=${SD}`, 500],
  ['werk page', `${APP}/gathering-docs/werk.html`, 500],
  ['cards by domain', `${APP}/gathering-docs/cards-by-domain.html`, 500],
  ['flow page', `${APP}/gathering-docs/flow.html`, 500],
];

async function measureEndpoint(url: string, thresholdMs: number) {
  const start = Date.now();
  let res: Response;
  try {
    res = await fetch(url, { signal: AbortSignal.timeout(thresholdMs * 3) });
  } catch (e) {
    return { url, ms: Date.now() - start, status: 0, pass: false, error: (e as Error).message };
  }
  const ms = Date.now() - start;
  const pass = ms <= thresholdMs;
  return { url, ms, status: res.status, pass, threshold: thresholdMs };
}

function budgetSuite(suiteName: string, budgets: Array<[string, string, number]>) {
  describeIntegration(suiteName, () => {
    for (const [name, url, threshold] of budgets) {
      test(`${name} responds within ${threshold}ms`, async () => {
        const result = await measureEndpoint(url, threshold);
        console.log(`  ${name}: ${result.ms}ms / ${threshold}ms budget (${result.pass ? 'PASS' : 'FAIL'})`);
        expect(result.ms).toBeLessThanOrEqual(threshold);
        expect(result.status).toBeGreaterThanOrEqual(200);
        expect(result.status).toBeLessThan(500);
      }, threshold * 3 + 5000);
    }
  });
}

budgetSuite('Athena top-level endpoints', ATHENA_TOP_LEVEL);
budgetSuite('Athena subdomain + facets', ATHENA_SUBDOMAIN);
budgetSuite('Chorus domain detail', DOMAIN_DETAIL);
budgetSuite('Chorus API', CHORUS_API);
budgetSuite('Gathering app pages', PAGE_BUDGETS);
