/**
 * Browser flows for Chorus (#3798).
 *
 * These run against the LIVE system on purpose. There is one environment and it
 * is production, so a flow that proves the door works has to open the real door.
 *
 * Credentials: never hard-coded, never committed. The runner reads them from
 * ~/.chorus/flow-creds.env (chmod 600, outside the repo) if it exists, so the
 * signed-in flows RUN instead of skipping. Env vars already set win over the file.
 */
// #4915 — one loader for the flow creds, shared with scripts outside this runner
require('./proving/flows/_auth.cjs').loadFlowCreds();

module.exports = {
  testDir: './proving/flows',
  testMatch: '**/*.spec.cjs',
  timeout: 60_000,
  expect: { timeout: 15_000 },
  // Serial: these drive one real system with one real identity. Parallel runs
  // would sign the same account in and out from under each other.
  workers: 1,
  fullyParallel: false,
  reporter: [['list']],
  use: {
    baseURL: process.env.FLOW_BASE || 'https://lightlifeurbangardens.com',
    ignoreHTTPSErrors: false,
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure',
  },
  projects: [{ name: 'chromium', use: { browserName: 'chromium' } }],
};
