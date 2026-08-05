/**
 * Chorus UI Sitemap Scraper + Site Coherence Check — #3758
 *
 * ONE inventory, TWO consumers (so review and regression can never drift):
 *   map mode (default): screenshot + narrative per surface → review report
 *     (the Gathering site-map treatment, 2026-02-21 precedent, ~75 pages)
 *   --check mode: no screenshots — every surface must respond 200 and not be
 *     an empty husk. The JX coherence check, nightly-runnable, fails LOUDLY
 *     with the page named (Jeff 2026-08-05: "some sort of site coherence
 *     check to make sure that all pages respond and are tested").
 *
 * Playwright rides the gathering repo's node_modules:
 *   cd $CHORUS_ROOT && npx --prefix ../jeff-bridwell-personal-site ts-node \
 *     platform/scripts/scrape-chorus-sitemap.ts [--check]
 *
 * Output (map mode):
 *   designing/docs/chorus-ui-sitemap/YYYY-MM-DD/{<name>.png, report.html}
 */

const { chromium } = require('playwright');
const fs = require('fs');
const path = require('path');

const BASE = process.env.CHORUS_UI_BASE || 'http://localhost:3340';
const CHECK_MODE = process.argv.includes('--check');


// The inventory — journey-ordered; dispositions live in
// designing/docs/chorus-ui-inventory-tobe.html, narratives here are review commentary.
const SURFACES = [
  // — Journey rungs —
  { name: 'root', url: '/', section: 'Journey rungs', narrative: 'The entrance. Today links only three legacy model pages — the #3761 rebuild makes the value stream the front door.' },
  { name: 'value-stream', url: '/athena/value-stream.html', section: 'Journey rungs', narrative: 'The spine: Shaping→…→Reflecting. Jeff: "has potential" — becomes the organizing frame of the whole journey.' },
  { name: 'products', url: '/athena/products.html', section: 'Journey rungs', narrative: 'The product rung. Each value-stream step opens into its products.' },
  { name: 'product', url: '/athena/product.html', section: 'Journey rungs', narrative: 'Single-product view; likely merges with product-view into one page.' },
  { name: 'domains-model', url: '/domains/principles', section: 'Journey rungs', narrative: 'The model view of a domain (#3724/#3749): schema → OWL → instances, Jeff\'s reading order. Principles as the reference case.' },
  { name: 'domains-ops', url: '/athena/domains.html', section: 'Journey rungs', narrative: 'The ops listing — Jeff: "i like the ux and simplicity of this." The aesthetic bar for the integration.' },
  { name: 'erd', url: '/athena/model.html', section: 'Journey rungs', narrative: 'The ERD (#3739) — domains and their lines, drill to class diagrams. The "what it\'s all made of" instrument.' },
  { name: 'tree', url: '/athena/tree.html', section: 'Journey rungs', narrative: 'Containment tree — a descent instrument that needs the shared nav.' },
  { name: 'chorus-drawing', url: '/chorus', section: 'Journey rungs', narrative: 'Jeff\'s drawing — the authoritative decomposition, one click from the entrance.' },
  { name: 'instance-page', url: '/domains/principles/hemenway-observe', section: 'Journey rungs', narrative: 'The bottom of the descent: one instance, full entity render (post-#3749 — was three dashes).' },

  // — Loom —
  { name: 'loom-hub', url: '/loom', section: 'Loom', narrative: 'The loom product page — values/principles/policies/decisions hub.' },
  { name: 'loom-principles', url: '/loom/principles.html', section: 'Loom', narrative: 'Fed by the generated surface (#3749); gains Values (#3759) and the 28 recursive principles (#3760).' },
  { name: 'loom-policies', url: '/loom/policies.html', section: 'Loom', narrative: 'Policy leg #3755 repoints this to the generated surface.' },
  { name: 'loom-decisions', url: '/loom/decisions.html', section: 'Loom', narrative: 'Decisions — same repoint treatment later.' },

  // — Working surfaces —
  { name: 'flow', url: '/flow', section: 'Working surfaces', narrative: 'Werk flow — board/pipeline state.' },
  { name: 'werk', url: '/werk', section: 'Working surfaces', narrative: 'Werk product page.' },
  { name: 'borg-assessment', url: '/borg-assessment', section: 'Working surfaces', narrative: 'Borg product page.' },
  { name: 'harvest-manifests', url: '/harvest-manifests', section: 'Working surfaces', narrative: 'Convergence/harvest manifests.' },
  { name: 'icd', url: '/harvesting/icd', section: 'Working surfaces', narrative: 'ICD — harvest contracts.' },
  { name: 'operations', url: '/operations.html', section: 'Working surfaces', narrative: 'REVIEW: overlaps the future services rung.' },

  // — Legacy model explorers (the root page's three links) —
  { name: 'chorus-data-model', url: '/chorus-data-model.html', section: 'Legacy model explorers', narrative: 'REVIEW: pre-#3739 model view; the ERD + /domains likely supersede — judged after integration.' },
  { name: 'chorus-er-diagram', url: '/chorus-er-diagram.html', section: 'Legacy model explorers', narrative: 'REVIEW: hand-drawn ER predecessor of the generated ERD.' },
  { name: 'chorus-instance-explorer', url: '/chorus-instance-explorer.html', section: 'Legacy model explorers', narrative: 'REVIEW: instance browser; /domains/<d>/<i> covers the journey case.' },
  { name: 'model-data-hub', url: '/model-data-hub.html', section: 'Legacy model explorers', narrative: 'REVIEW: one of three model-data variants — the competing-surfaces family.' },
  { name: 'chorus-model-data', url: '/chorus-model-data', section: 'Legacy model explorers', narrative: 'REVIEW: model-data variant two.' },
  { name: 'model-data', url: '/model-data', section: 'Legacy model explorers', narrative: 'REVIEW: model-data variant three.' },

  // — Reflecting instruments —
  { name: 'attention-analytics', url: '/attention-analytics.html', section: 'Reflecting instruments', narrative: 'Attention/touch analytics — belongs under the Reflecting step.' },
  { name: 'voice-analytics', url: '/voice-analytics.html', section: 'Reflecting instruments', narrative: 'Jeff\'s voice/tone analytics.' },
  { name: 'reprompt-analytics', url: '/reprompt-analytics.html', section: 'Reflecting instruments', narrative: 'Re-prompt (friction) analytics.' },
  { name: 'gemba-analysis', url: '/gemba-analysis.html', section: 'Reflecting instruments', narrative: 'Gemba observation analysis.' },
  { name: 'frustration', url: '/frustration.html', section: 'Reflecting instruments', narrative: 'Frustration telemetry.' },
  { name: 'doc-catalog', url: '/doc-catalog.html', section: 'Reflecting instruments', narrative: 'Doc catalog — the tagged design-doc index.' },

  // — Strategy & research one-offs —
  { name: 'wardley-map', url: '/wardley-map.html', section: 'Strategy & research', narrative: 'REVIEW: strategy one-off.' },
  { name: 'gathering-chorus', url: '/gathering-chorus.html', section: 'Strategy & research', narrative: 'REVIEW: the two-product relationship page.' },
  { name: 'business-plan', url: '/business-plan.html', section: 'Strategy & research', narrative: 'REVIEW: strategy doc.' },
  { name: 'practice-spine', url: '/practice-spine.html', section: 'Strategy & research', narrative: 'FOLD: Jeff\'s home spine — personal rhythm; placement is Jeff\'s call.' },
  { name: 'ui-system', url: '/ui-system.html', section: 'Strategy & research', narrative: 'May seed the shared design system alongside the flow mock.' },

  // — Design references —
  { name: 'athena-flow-mock', url: '/designing/docs/athena-flow-mock/index.html', section: 'Design references', narrative: 'THE design-language North Star (#3378): crafted header, eyebrow/title/lede/chips, value-stream spine.' },
  { name: 'ui-inventory-tobe', url: '/designing/docs/chorus-ui-inventory-tobe.html', section: 'Design references', narrative: 'This review\'s companion: the to-be tree + dispositions (#3758).' },
];


async function main() {
  const date = process.env.SCRAPE_DATE || new Date().toISOString().slice(0, 10);
  const outDir = path.join(__dirname, '..', '..', 'designing', 'docs', 'chorus-ui-sitemap', date);
  if (!CHECK_MODE) fs.mkdirSync(outDir, { recursive: true });

  const browser = await chromium.launch();
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  const page = await ctx.newPage();

  const results = [];
  for (const s of SURFACES) {
    let status = 'ok', husk = false, shot = null;
    try {
      const resp = await page.goto(BASE + s.url, { waitUntil: 'networkidle', timeout: 20000 });
      await page.waitForTimeout(800); // client-rendered pages settle
      if (!resp || !resp.ok()) status = `http ${resp ? resp.status() : '???'}`;
      // Husk check (#3758 coherence): a page that responds 200 but renders
      // almost nothing is the front-end two-states bug — "responds" and
      // "works" must be distinguishable.
      const textLen = await page.evaluate(() => (document.body?.innerText || '').trim().length);
      husk = textLen < 80;
      if (!CHECK_MODE) {
        shot = `${s.name}.png`;
        await page.screenshot({ path: path.join(outDir, shot), fullPage: false });
      }
    } catch (e) {
      status = `FAILED: ${e instanceof Error ? e.message.split('\n')[0] : String(e)}`;
    }
    results.push({ s, status, husk, shot });
  }
  await browser.close();

  const bad = results.filter((r) => r.status !== 'ok' || r.husk);
  if (CHECK_MODE) {
    for (const r of results) {
      const verdict = r.status !== 'ok' ? r.status : r.husk ? 'HUSK (renders <80 chars)' : 'ok';
      console.log(`${verdict === 'ok' ? 'ok  ' : 'FAIL'}  ${r.s.name}  ${r.s.url}  ${verdict !== 'ok' ? '— ' + verdict : ''}`);
    }
    console.log(`\nsite-coherence: ${results.length - bad.length}/${results.length} pass${bad.length ? ` — ${bad.length} FAILED` : ''}`);
    process.exitCode = bad.length ? 1 : 0;
    return;
  }

  const sections = [...new Set(SURFACES.map((s) => s.section))];
  const card = (r) => `
    <div class="card ${r.status === 'ok' && !r.husk ? '' : 'bad'}">
      <h3>${r.s.name} <span class="st">${r.status}${r.husk ? ' · HUSK' : ''}</span></h3>
      <p class="u"><a href="${BASE}${r.s.url}">${r.s.url}</a></p>
      ${r.shot ? `<a href="${r.shot}"><img src="${r.shot}" loading="lazy"></a>` : '<p class="miss">no screenshot</p>'}
      <p class="narr">${r.s.narrative}</p>
    </div>`;
  const report = `<!DOCTYPE html><html><head><meta charset="utf-8"><title>Chorus UI Site Map — ${date}</title>
<style>
 body{font-family:-apple-system,sans-serif;background:#fafafa;color:#1a1a1a;padding:32px;max-width:1280px;margin:0 auto}
 h1{font-weight:300}.meta{color:#667;font-size:.85rem;margin-bottom:20px}
 h2{margin:28px 0 12px;font-size:1.05rem}
 .grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(380px,1fr));gap:16px}
 .card{background:#fff;border:1px solid #e0e0e0;border-radius:6px;padding:12px}
 .card.bad{border-color:#a8226b}
 .card h3{font-size:.95rem;margin-bottom:2px}.st{font-weight:400;font-size:.7rem;color:#667}
 .card.bad .st{color:#a8226b;font-weight:700}
 .u a{font-family:ui-monospace,monospace;font-size:.75rem;color:#0d6a89;text-decoration:none}
 img{width:100%;border:1px solid #eee;border-radius:4px;margin:8px 0}
 .narr{font-size:.82rem;color:#444;line-height:1.5}.miss{color:#a8226b;font-size:.8rem}
</style></head><body>
<h1>Chorus UI — Site Map &amp; Review</h1>
<p class="meta">#3758 · generated ${date} · ${SURFACES.length} surfaces · ${results.length - bad.length} clean, ${bad.length} flagged · companion: <a href="/designing/docs/chorus-ui-inventory-tobe.html">to-be tree</a> · precedent: the Gathering site map (2026-02-21)</p>
${sections.map((sec) => `<h2>${sec}</h2><div class="grid">${results.filter((r) => r.s.section === sec).map(card).join('')}</div>`).join('')}
</body></html>`;
  fs.writeFileSync(path.join(outDir, 'report.html'), report);
  console.log(`chorus-ui-sitemap: ${results.length - bad.length} clean, ${bad.length} flagged → ${outDir}/report.html`);
}

main();
