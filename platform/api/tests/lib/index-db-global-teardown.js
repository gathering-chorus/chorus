// #4152 — remove the per-run index.db copy made by index-db-global-setup.js.
const fs = require('fs');
module.exports = async function indexDbGlobalTeardown() {
  const dir = process.env.CHORUS_TEST_INDEX_DIR;
  if (dir && dir.includes('chorus-api-test-index-')) {
    fs.rmSync(dir, { recursive: true, force: true });
  }
};
