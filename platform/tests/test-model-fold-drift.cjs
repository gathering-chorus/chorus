/**
 * #3757 — the cannot-drift proof for the shared model fold.
 *
 * AC: "a test asserts the two surfaces render from identical data for the same
 * domain — a divergence is a failure, not a skin difference."
 *
 * #5041 — the ops-view half is RETIRED. Jeff, 2026-11-25: "we dont want 2-3 links
 * to same data"; the ops view (/athena/domain.html?d=<d>) no longer embeds the
 * fold, so there is one surface and nothing left to drift from. That the ops view
 * holds exactly one link into the model view, and no fold of its own, is held as
 * drawn by proving/flows/athena-domain-links-5034.spec.cjs ("no two links share a
 * destination; one link to the model view…"); that the model view's cards are the
 * rows the door holds is held by proving/flows/athena-domain-true-5034.spec.cjs.
 *
 * What this file still proves, in a real browser:
 *   model view:  /domains/<d>  is reached by its URL (no redirect) and draws the
 *   fold with its DATA (class names, property rows, instance names, honest-state
 *   notes — not styling): at least one class.
 *
 * NEGATIVE PROOF (no gate without one): the test then mutates the rendered DOM
 * (drops a schema row) and asserts the comparator REPORTS the difference from
 * what was first read. A comparator that cannot go red must not gate — this run
 * proves it can, every time it runs.
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
    // #3768 — a redirect here makes the comparison VACUOUS: the #3724 301 sent
    // the ops URL to the model page for three days and this test "passed" by
    // comparing the model page with itself. The URL surviving is part of the
    // invariant; a bounce is a loud failure, never a silent self-comparison.
    const finalPath = new URL(page.url()).pathname;
    const wantPath = new URL(url).pathname;
    if (finalPath !== wantPath) {
      throw new Error(`redirected: ${url} landed on ${page.url()} — the two surfaces cannot be compared (and one is unreachable by URL)`);
    }
    await page.waitForSelector('[data-mfold-ready]', { timeout: 20000 });
    const data = await page.evaluate(EXTRACT);
    if (!data) throw new Error('no .mfold rendered at ' + url);
    return data;
  };

  let failures = 0;
  try {
    // #5041 — one surface: the model view. (The ops view was grabbed here too and compared.)
    const model = await grab(`${BASE}/domains/${DOMAIN}`);

    if (!model.classes.length) {
      console.error(`EMPTY: the model view drew the fold for "${DOMAIN}" with no class in it`);
      console.error('model view:', JSON.stringify(model));
      failures++;
    } else {
      console.log(`drawn: ${model.classes.length} classes, ${model.rows.length} schema rows, ` +
        `${model.instances.length} instances, ${model.notes.length} honest-state notes — on the model view`);
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
