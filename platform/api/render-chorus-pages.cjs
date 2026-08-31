const ejs = require('ejs');
const fs = require('fs');
const path = require('path');
const viewsDir = path.join(__dirname, 'views');
const outDir = path.join(__dirname, 'public', 'chorus-pages');
const LOOM_ROLES = JSON.parse(fs.readFileSync(path.join(__dirname, 'data', 'loom-roles.json'), 'utf8'));
const pages = [
  ['chorus-system', 'chorus', { title:'Chorus — System' }],
  ['chorus-model-data', 'chorus-model-data', { title:'Chorus Model Data' }],
  ['borg-assessment', 'borg-assessment', { title:'Borg Assessment' }],
  ['icd', 'icd', { title:'Convergence Architecture — ICD' }],
  ['werk', 'werk', { title:'Werk', workflows:[], cards:[], orphanWorkflows:[] }],
  ['harvest-manifests', 'harvest-manifests', { title:'Harvesting', manifests:{}, filter:'', isFocused:false }],
  // #4036 — the Loom pages render with the REAL role content (migrated from
  // Gathering's team.handler.ts into data/loom-roles.json); metrics hydrate
  // client-side from the live /api/loom-metrics.
  ['team', 'loom', { title:'Loom — Team', roles: LOOM_ROLES, metrics:{}, cards:[] }],
  ['loom-role', 'loom-jeff',  { role: LOOM_ROLES.jeff }],
  ['loom-role', 'loom-wren',  { role: LOOM_ROLES.wren }],
  ['loom-role', 'loom-silas', { role: LOOM_ROLES.silas }],
  ['loom-role', 'loom-kade',  { role: LOOM_ROLES.kade }],
  ['flow', 'flow', { title:'Flow', cards:[], data:{} }],
  ['ontology-views/model-data', 'model-data', { title:'Model Data', domainStats:{}, ontology:{} }],
];
const shell = (t)=>`<!doctype html><html lang="en"><head><meta charset="utf-8"><title>${t} — Chorus</title></head><body style="font-family:system-ui;max-width:42rem;margin:4rem auto;line-height:1.5"><h1>${t}</h1><p>Served from Chorus. Live data wiring is the prioritized follow-on (#3361).</p></body></html>`;
// #4036 — optional filter: `node render-chorus-pages.cjs loom` renders only
// pages whose OUT name starts with the filter. A full render regenerates
// every artifact, including ones whose views expect data this script does not
// carry — a scoped render lets one card rebuild its pages without touching
// the others' outputs.
const only = process.argv[2] || '';
let ok=0, sh=0;
for (const [view, out, data] of pages) {
  if (only && !out.startsWith(only)) continue;
  const f = path.join(viewsDir, view + '.ejs');
  let html;
  try { html = ejs.render(fs.readFileSync(f,'utf8'), {cspNonce:'', ...data}, {filename:f, views:[viewsDir]}); ok++; }
  catch (e) { html = shell(data.title || out); sh++; console.log('SHELL', out, '-', String(e.message).split('\n')[0].slice(0,70)); }
  fs.writeFileSync(path.join(outDir, out + '.html'), html);
}
console.log(`rendered ${ok} real, ${sh} shell`);
