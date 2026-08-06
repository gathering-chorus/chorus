/**
 * #3757 — the cannot-drift proof for the shared model fold.
 *
 * AC: "a test asserts the two surfaces render from identical data for the same
 * domain — a divergence is a failure, not a skin difference."
 *
 * Renders BOTH altitudes for the same domain in a real browser:
 *   model view:  /domains/<d>                (full fold)
 *   ops view:    /athena/domain.html?d=<d>   (compact fold)
 * extracts the fold's DATA (class names, property rows, instance names, honest-
 * state notes — not styling) and asserts deep equality.
 *
 * NEGATIVE PROOF (no gate without one): after the equal-pass, the test mutates
 * one surface's rendered DOM (drops a schema row) and asserts the comparator
 * now REPORTS divergence. A comparator that cannot go red on a violated
 * invariant must not gate — this run proves it can, every time it runs.
 *
 * Run: NODE_PATH=../jeff-bridwell-personal-site/node_modules node platform/tests/test-model-fold-drift.cjs
 *   CHORUS_UI_BASE overrides the target (default :3340; the werk pipeline
 *   points it at the demo env).
 */

const { chromium } = require('playwright');

const BASE = process.env.CHORUS_UI_BASE || 'http://localhost:3340';
// principles: populated, shape-served, the #3749 reference domain — exercises
// schema rows AND instance cards. A domain with honest-state notes still
// compares equal (the notes are part of the extracted data).
const DOMAIN = process.env.MF_DOMAIN || 'principles';

const EXTRACT = () => {
  const root = document.querySelector('.mfold');
  if (!root) return null;
  const classes = [...root.querySelectorAll('h3')].map(h => h.textContent.trim());
  const rows = [...root.querySelectorAll('table tbody tr')].map(tr =>
    [...tr.querySelectorAll('td')].map(td => td.textContent.trim()).join('|'));
  const instances = [...root.querySelectorAll('a.mfcard .mfm')].map(m => m.textContent.trim());
  const notes = [...root.querySelectorAll('.mfnote strong, .mferr strong')].map(n => n.textContent.trim());
  return { classes, rows, instances, notes };
};

function diverges(a, b) {
  return JSON.stringify(a) !== JSON.stringify(b);
}

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage();
  const grab = async (url) => {
    await page.goto(url, { waitUntil: 'networkidle', timeout: 30000 });
    await page.waitForSelector('[data-mfold-ready]', { timeout: 20000 });
    const data = await page.evaluate(EXTRACT);
    if (!data) throw new Error('no .mfold rendered at ' + url);
    return data;
  };

  let failures = 0;
  try {
    const model = await grab(`${BASE}/domains/${DOMAIN}`);
    // grab ops LAST so the still-loaded page is the one we mutate below
    const ops = await grab(`${BASE}/athena/domain.html?d=${DOMAIN}`);

    if (diverges(model, ops)) {
      console.error(`DRIFT: the two altitudes render different data for "${DOMAIN}"`);
      console.error('model view:', JSON.stringify(model));
      console.error('ops view:  ', JSON.stringify(ops));
      failures++;
    } else {
      console.log(`equal: ${model.classes.length} classes, ${model.rows.length} schema rows, ` +
        `${model.instances.length} instances, ${model.notes.length} honest-state notes — identical on both altitudes`);
    }

    // ---- negative proof: a planted divergence MUST be detected ----
    const mutated = await page.evaluate((ex) => {
      const row = document.querySelector('.mfold table tbody tr');
      if (row) row.remove();
      else {
        // domain with no schema rows: drop an instance card instead
        const card = document.querySelector('.mfold a.mfcard');
        if (card) card.remove();
        else {
          // nothing structural to drop — mutate a note
          const n = document.querySelector('.mfold .mfnote strong, .mfold .mferr strong');
          if (n) n.textContent = 'MUTATED';
        }
      }
      return (0, eval)('(' + ex + ')')();
    }, EXTRACT.toString());
    if (!diverges(model, mutated)) {
      console.error('NEGATIVE PROOF FAILED: a planted divergence was NOT detected — the comparator cannot go red and must not gate.');
      failures++;
    } else {
      console.log('NEGATIVE PROOF OK: planted divergence detected — the comparator can fail.');
    }
  } catch (e) {
    console.error('test error:', e.message);
    failures++;
  } finally {
    await browser.close();
  }
  process.exit(failures ? 1 : 0);
})();
