#!/usr/bin/env node
/**
 * site-walkthrough.mjs — Headless Chrome walkthrough with desktop + mobile captures
 *
 * Usage:
 *   npx puppeteer node scripts/site-walkthrough.mjs [--desktop|--mobile|--both] [output-dir]
 *   node scripts/site-walkthrough.mjs [--desktop|--mobile|--both] [output-dir]
 *
 * Reads page inventory from style-manifest.json, logs into the app via SOLID,
 * then captures every page at desktop (1440x900) and/or mobile (393x852).
 * Builds an interactive HTML catalog with side-by-side comparison.
 *
 * Zero focus-stealing — runs entirely headless.
 */

import puppeteer from 'puppeteer';
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'fs';
import { join, resolve } from 'path';

const BASE_URL = process.env.GATHERING_URL || 'http://localhost:3000';
const MANIFEST_PATH = resolve(
  process.env.HOME,
  'CascadeProjects/jeff-bridwell-personal-site/data/style-manifest.json'
);

const DESKTOP = { width: 1440, height: 900, label: 'desktop' };
const MOBILE = { width: 393, height: 852, label: 'mobile', isMobile: true, hasTouch: true };

// Parse CLI args
let mode = 'both';
let outDir = '';
for (const arg of process.argv.slice(2)) {
  if (arg === '--desktop') mode = 'desktop';
  else if (arg === '--mobile') mode = 'mobile';
  else if (arg === '--both') mode = 'both';
  else if (!arg.startsWith('-')) outDir = arg;
}

if (!outDir) {
  const ts = new Date().toISOString().replace(/[T:]/g, '-').slice(0, 15).replace(/-/g, '');
  // Format: YYYYMMDD-HHMMSS
  const now = new Date();
  const pad = (n) => String(n).padStart(2, '0');
  const stamp = `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}-${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;
  outDir = `/tmp/site-walkthrough/${stamp}`;
}
mkdirSync(outDir, { recursive: true });

// Load page manifest
if (!existsSync(MANIFEST_PATH)) {
  console.error(`ERROR: style-manifest.json not found at ${MANIFEST_PATH}`);
  process.exit(1);
}
const manifest = JSON.parse(readFileSync(MANIFEST_PATH, 'utf-8'));

// Flatten pages with sequence numbers
const pages = [];
let seq = 0;
for (const spoke of manifest.spokes) {
  for (const page of spoke.pages) {
    seq++;
    pages.push({
      seq: String(seq).padStart(2, '0'),
      route: page.route,
      label: page.label,
      section: spoke.name,
      theme: page.theme || 'light',
      nav: page.nav || 'main',
      tooling: page.tooling || false,
    });
  }
}

console.log(`Site walkthrough (${mode}): ${pages.length} pages → ${outDir}`);
console.log(`Source: style-manifest.json v${manifest.version}`);

async function login(page) {
  // Navigate to a protected page to trigger auth redirect
  await page.goto(`${BASE_URL}/music`, { waitUntil: 'networkidle0', timeout: 15000 });

  // Check if we're on the login page
  const url = page.url();
  if (!url.includes('/login')) {
    console.log('  Already authenticated');
    return true;
  }

  // Select local provider (localhost:3001) — should be checked by default
  // Submit the form
  try {
    await page.evaluate(() => {
      const form = document.querySelector('form[action="/login"]');
      if (form) {
        // Ensure local provider is selected
        const localRadio = form.querySelector('input[value="http://localhost:3001"]');
        if (localRadio) localRadio.checked = true;
        form.submit();
      }
    });

    // Wait for redirect back to the app
    await page.waitForNavigation({ waitUntil: 'networkidle0', timeout: 15000 });
    console.log('  Login successful');
    return true;
  } catch (err) {
    console.error(`  Login failed: ${err.message}`);
    return false;
  }
}

async function capturePage(page, url, filename, viewport) {
  await page.setViewport(viewport);

  try {
    await page.goto(url, { waitUntil: 'networkidle0', timeout: 15000 });
  } catch (err) {
    // networkidle0 can timeout on pages with persistent connections — try domcontentloaded
    try {
      await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 10000 });
      await new Promise(r => setTimeout(r, 2000)); // settle time for JS
    } catch (err2) {
      return false;
    }
  }

  // Extra settle time for D3, dynamic content
  await new Promise(r => setTimeout(r, 1500));

  const filepath = join(outDir, filename);
  await page.screenshot({ path: filepath, fullPage: false });
  return true;
}

function slugify(route) {
  if (route === '/') return 'home';
  return route.replace(/^\//, '').replace(/\//g, '-').replace(/\.html$/, '');
}

async function run() {
  const browser = await puppeteer.launch({
    headless: 'new',
    args: ['--no-sandbox', '--disable-setuid-sandbox', '--disable-gpu'],
  });

  const page = await browser.newPage();

  // Authenticate
  console.log('Authenticating...');
  const loggedIn = await login(page);
  if (!loggedIn) {
    console.error('Failed to authenticate — capturing public pages only');
  }

  const results = [];
  let captured = 0;
  let failed = 0;

  const viewports = [];
  if (mode === 'desktop' || mode === 'both') viewports.push(DESKTOP);
  if (mode === 'mobile' || mode === 'both') viewports.push(MOBILE);

  for (const p of pages) {
    const url = `${BASE_URL}${p.route}`;
    const slug = slugify(p.route);

    for (const vp of viewports) {
      const suffix = vp.label === 'mobile' ? '-mobile' : '';
      const filename = `${p.seq}-${slug}${suffix}.png`;

      const vpLabel = vp.label.padEnd(7);
      process.stdout.write(`  [${p.seq}/${String(pages.length).padStart(2, '0')}] ${p.label.padEnd(30)} ${vpLabel} `);

      const ok = await capturePage(page, url, filename, vp);
      if (ok) {
        captured++;
        console.log('OK');
        results.push({
          seq: parseInt(p.seq),
          slug: p.route,
          label: p.label,
          section: p.section,
          theme: p.theme,
          nav: p.nav,
          file: filename,
          viewport: vp.label,
        });
      } else {
        failed++;
        console.log('FAIL');
      }
    }
  }

  await browser.close();

  // Write manifest
  writeFileSync(join(outDir, 'manifest.json'), JSON.stringify(results, null, 2));

  // Generate HTML catalog
  generateCatalog(results, outDir);

  console.log('');
  console.log(`Done: ${captured} captured, ${failed} failed (mode: ${mode})`);
  console.log(`Catalog: ${outDir}/index.html`);
  console.log(`Open:    open ${outDir}/index.html`);
}

function generateCatalog(results, outDir) {
  // Group by page (seq+slug) to pair desktop/mobile
  const groups = new Map();
  for (const r of results) {
    const key = `${r.seq}|${r.slug}`;
    if (!groups.has(key)) {
      groups.set(key, {
        seq: r.seq, slug: r.slug, label: r.label,
        section: r.section, theme: r.theme, nav: r.nav,
        desktop: null, mobile: null,
      });
    }
    groups.get(key)[r.viewport] = r.file;
  }

  const grouped = [...groups.values()];
  const hasMobile = grouped.some(p => p.mobile);
  const hasDesktop = grouped.some(p => p.desktop);
  const ts = new Date().toLocaleString('en-US', { timeZone: 'America/New_York', dateStyle: 'short', timeStyle: 'short' });

  let html = `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<title>Gathering — Site Walkthrough</title>
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { background: #1a1a2e; color: #e0e0e0; font-family: -apple-system, BlinkMacSystemFont, sans-serif; padding: 2rem; padding-right: 220px; }
  h1 { color: #c4a35a; margin-bottom: 0.5rem; }
  .meta { color: #888; margin-bottom: 2rem; font-size: 0.9rem; }
  .section-header { color: #c4a35a; font-size: 1.3rem; margin: 2rem 0 1rem; border-bottom: 1px solid #333; padding-bottom: 0.5rem; }
  .page-row { display: flex; gap: 1.5rem; margin-bottom: 2rem; align-items: flex-start; }
  .card { background: #16213e; border-radius: 8px; overflow: hidden; border: 1px solid #333; transition: border-color 0.2s; }
  .card:hover { border-color: #c4a35a; }
  .card img { width: 100%; display: block; cursor: pointer; }
  .card-desktop { flex: 3; }
  .card-mobile { flex: 1; max-width: 250px; }
  .card-info { padding: 0.75rem 1rem; display: flex; flex-wrap: wrap; align-items: baseline; gap: 0.25rem 0.5rem; }
  .card-info .label { font-weight: 600; }
  .card-info .slug { color: #888; font-size: 0.85rem; font-family: monospace; }
  .card-info .seq { color: #c4a35a; font-size: 0.8rem; }
  .card-info .badge { font-size: 0.7rem; padding: 1px 6px; border-radius: 3px; }
  .card-info .vp { font-size: 0.7rem; padding: 1px 6px; border-radius: 3px; background: #2a2a3e; color: #aaa; }
  .badge-dark { background: #333; color: #ccc; }
  .badge-doc { background: #2a3a5a; color: #8ab; }
  .nav { position: fixed; top: 1rem; right: 1rem; background: #16213e; border: 1px solid #333; border-radius: 8px; padding: 1rem; max-height: 90vh; overflow-y: auto; font-size: 0.85rem; width: 200px; z-index: 500; }
  .nav a { color: #aaa; text-decoration: none; display: block; padding: 2px 0; }
  .nav a:hover { color: #c4a35a; }
  .nav .nav-section { color: #c4a35a; font-weight: 600; margin-top: 0.5rem; }
  .lightbox { display: none; position: fixed; top: 0; left: 0; width: 100%; height: 100%; background: rgba(0,0,0,0.9); z-index: 1000; cursor: pointer; justify-content: center; align-items: center; }
  .lightbox.active { display: flex; }
  .lightbox img { max-width: 95%; max-height: 95%; object-fit: contain; }
</style>
</head>
<body>
<h1>Gathering — Site Walkthrough</h1>
<p class="meta">Captured ${ts} | ${grouped.length} pages | ${hasMobile && hasDesktop ? 'Desktop + Mobile' : hasDesktop ? 'Desktop' : 'Mobile'} | Source: style-manifest.json v${manifest.version}</p>
`;

  // Nav sidebar
  html += '<div class="nav">\n';
  let curSection = '';
  for (const p of grouped) {
    if (p.section !== curSection) {
      curSection = p.section;
      html += `<div class="nav-section">${curSection}</div>\n`;
    }
    html += `<a href="#page-${p.seq}">${p.seq}. ${p.label}</a>\n`;
  }
  html += '</div>\n';

  // Page rows
  curSection = '';
  for (const p of grouped) {
    if (p.section !== curSection) {
      curSection = p.section;
      html += `<h2 class="section-header" id="section-${curSection.toLowerCase()}">${curSection}</h2>\n`;
    }

    const badges = [];
    if (p.theme === 'dark') badges.push('<span class="badge badge-dark">dark</span>');
    if (p.nav === 'doc-chrome') badges.push('<span class="badge badge-doc">doc-chrome</span>');
    const badgeHtml = badges.join(' ');

    html += `<div class="page-row" id="page-${p.seq}">\n`;

    if (p.desktop) {
      html += `  <div class="card card-desktop">
    <img src="${p.desktop}" alt="${p.label} (desktop)" loading="lazy" onclick="openLightbox(this.src)">
    <div class="card-info">
      <span class="seq">#${p.seq}</span><span class="label">${p.label}</span>
      <span class="vp">desktop</span> ${badgeHtml}
      <div class="slug">${p.slug}</div>
    </div>
  </div>\n`;
    }

    if (p.mobile) {
      html += `  <div class="card card-mobile">
    <img src="${p.mobile}" alt="${p.label} (mobile)" loading="lazy" onclick="openLightbox(this.src)">
    <div class="card-info">
      <span class="vp">mobile</span>
    </div>
  </div>\n`;
    }

    html += '</div>\n';
  }

  html += `<div class="lightbox" id="lightbox" onclick="this.classList.remove('active')">
  <img id="lightbox-img" src="" alt="Full size">
</div>
<script>
function openLightbox(src) {
  document.getElementById('lightbox-img').src = src;
  document.getElementById('lightbox').classList.add('active');
}
document.addEventListener('keydown', e => {
  if (e.key === 'Escape') document.getElementById('lightbox').classList.remove('active');
});
</script>
</body></html>`;

  writeFileSync(join(outDir, 'index.html'), html);
}

run().catch(err => {
  console.error(`Fatal: ${err.message}`);
  process.exit(1);
});
