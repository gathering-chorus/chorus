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
const fs = require('fs');
const os = require('os');
const path = require('path');

const credFile = process.env.FLOW_CREDS_FILE || path.join(os.homedir(), '.chorus', 'flow-creds.env');
if (fs.existsSync(credFile)) {
  for (const line of fs.readFileSync(credFile, 'utf8').split('\n')) {
    const m = line.match(/^\s*(?:export\s+)?([A-Z0-9_]+)\s*=\s*(.*)$/);
    if (!m) continue;
    const key = m[1];
    const val = m[2].trim().replace(/^['"]|['"]$/g, '');
    if (process.env[key] === undefined) process.env[key] = val;
  }
}

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
