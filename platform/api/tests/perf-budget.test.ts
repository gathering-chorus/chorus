/**
 * Performance budget tests — #1777
 *
 * Integration tests — hit live Chorus API at localhost:3340 and app at localhost:3000.
 * Validates response times stay within budget. Thresholds based on measured
 * baselines (2026-04-16) with margin for load variance.
 *
 * Jeff's signal: "Athena page especially seems sluggish" — domain detail at
 * 450-520ms and subdomain cards at 600ms are the bottlenecks. Most Athena
 * endpoints are 20-40ms. Full Athena experience covered here.
 *
 * Complements Silas's nightly perf-baseline-chorus.sh (#1914) which runs from
 * a LaunchAgent. This test runs in the gate:code suite so regressions are caught
 * at demo time, not overnight.
 */

import { startTestApp, type TestApp } from './lib/test-app';

const APP = process.env.GATHERING_APP || 'http://localhost:3000';

const SD = 'chorus-domain'; // representative subdomain for facet tests

// --- Athena top-level endpoints (20-30ms measured) ---
const ATHENA_TOP_LEVEL = [
  ['athena health', `${harness.baseUrl}/api/athena/health`, 100],
  ['athena products', `${harness.baseUrl}/api/athena/products`, 500],
  ['athena subproducts', `${harness.baseUrl}/api/athena/subproducts`, 100],
  ['athena subdomains', `${harness.baseUrl}/api/athena/subdomains`, 200],
  ['athena steps', `${harness.baseUrl}/api/athena/steps`, 100],
  ['athena owners', `${harness.baseUrl}/api/athena/owners`, 100],
  ['athena machines', `${harness.baseUrl}/api/athena/machines`, 100],
  ['athena validate', `${harness.baseUrl}/api/athena/validate`, 200],
];

// --- Athena subdomain detail + facets (20-40ms measured, except cards at 600ms) ---
const ATHENA_SUBDOMAIN = [
  ['subdomain detail', `${harness.baseUrl}/api/athena/subdomains/${SD}`, 100],
  ['completeness', `${harness.baseUrl}/api/athena/subdomains/${SD}/completeness`, 100],
  ['blast radius', `${harness.baseUrl}/api/athena/subdomains/${SD}/blast-radius`, 200],
  ['cards', `${harness.baseUrl}/api/athena/subdomains/${SD}/cards`, 1000],
  ['alerts', `${harness.baseUrl}/api/athena/subdomains/${SD}/alerts`, 100],
  ['code', `${harness.baseUrl}/api/athena/subdomains/${SD}/code`, 100],
  ['coverage', `${harness.baseUrl}/api/athena/subdomains/${SD}/coverage`, 100],
  ['test coverage', `${harness.baseUrl}/api/athena/subdomains/${SD}/test-coverage`, 100],
  ['pages', `${harness.baseUrl}/api/athena/subdomains/${SD}/pages`, 100],
  ['services', `${harness.baseUrl}/api/athena/subdomains/${SD}/services`, 100],
  ['actors', `${harness.baseUrl}/api/athena/subdomains/${SD}/actors`, 100],
  ['scenarios', `${harness.baseUrl}/api/athena/subdomains/${SD}/scenarios`, 100],
  ['contract', `${harness.baseUrl}/api/athena/subdomains/${SD}/contract`, 100],
  ['integrations', `${harness.baseUrl}/api/athena/subdomains/${SD}/integrations`, 100],
  ['persistence', `${harness.baseUrl}/api/athena/subdomains/${SD}/persistence`, 100],
];

// --- Chorus domain detail (450-520ms measured — the sluggish ones) ---
const DOMAIN_DETAIL = [
  ['domain detail (chorus)', `${harness.baseUrl}/api/chorus/domain/chorus`, 1000],
  ['domain detail (seeds)', `${harness.baseUrl}/api/chorus/domain/seeds`, 1000],
  ['domain detail (music)', `${harness.baseUrl}/api/chorus/domain/music`, 1000],
  ['domain detail (photos)', `${harness.baseUrl}/api/chorus/domain/photos`, 1000],
];

// --- Other Chorus API endpoints ---
const CHORUS_API = [
  ['chorus health', `${harness.baseUrl}/api/chorus/health`, 100],
  ['chorus search', `${harness.baseUrl}/api/chorus/search?q=test&limit=5`, 1500],
  ['chorus rcas', `${harness.baseUrl}/api/chorus/rcas`, 200],
];

// --- Page loads (static HTML shells, all <20ms) ---
const PAGE_BUDGETS = [
  ['app root', `${APP}/`, 500],
  ['domain detail page', `${APP}/gathering-docs/domain-detail.html?id=${SD}`, 500],
  ['werk page', `${APP}/gathering-docs/werk.html`, 500],
  ['cards by domain', `${APP}/gathering-docs/cards-by-domain.html`, 500],
  ['flow page', `${APP}/gathering-docs/flow.html`, 500],
];

async function measureEndpoint(url, thresholdMs) {
  const start = Date.now();
  let res;
  try {
    res = await fetch(url, { signal: AbortSignal.timeout(thresholdMs * 3) });
  } catch (e) {
    return { url, ms: Date.now() - start, status: 0, pass: false, error: e.message };
  }
  const ms = Date.now() - start;
  const pass = ms <= thresholdMs;
  return { url, ms, status: res.status, pass, threshold: thresholdMs };
}

function budgetSuite(suiteName, budgets) {
  describe(suiteName, () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
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

// Warmup: first fetch in a Jest process pays Node DNS/connection setup cost
beforeAll(async () => {
  await fetch(`${harness.baseUrl}/api/chorus/health`).catch(() => {});
}, 10_000);

budgetSuite('Athena top-level (#1777)', ATHENA_TOP_LEVEL);
budgetSuite('Athena subdomain facets (#1777)', ATHENA_SUBDOMAIN);
budgetSuite('Domain detail (#1777)', DOMAIN_DETAIL);
budgetSuite('Chorus API (#1777)', CHORUS_API);
budgetSuite('Page loads (#1777)', PAGE_BUDGETS);
