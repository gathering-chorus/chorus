const ejs = require('ejs');
const fs = require('fs');
const path = require('path');
const viewsDir = path.join(__dirname, 'views');
const outDir = path.join(__dirname, 'public', 'chorus-pages');
const pages = [
  ['chorus-system', 'chorus', { title:'Chorus — System' }],
  ['chorus-model-data', 'chorus-model-data', { title:'Chorus Model Data' }],
  ['borg-assessment', 'borg-assessment', { title:'Borg Assessment' }],
  ['icd', 'icd', { title:'Convergence Architecture — ICD' }],
  ['werk', 'werk', { title:'Werk', workflows:[], cards:[], orphanWorkflows:[] }],
  ['harvest-manifests', 'harvest-manifests', { title:'Harvesting', manifests:{}, filter:'', isFocused:false }],
  ['team', 'loom', { title:'Loom — Team', roles:[], metrics:{}, cards:[] }],
  ['flow', 'flow', { title:'Flow', cards:[], data:{} }],
  ['ontology-views/model-data', 'model-data', { title:'Model Data', domainStats:{}, ontology:{} }],
];
const shell = (t)=>`<!doctype html><html lang="en"><head><meta charset="utf-8"><title>${t} — Chorus</title></head><body style="font-family:system-ui;max-width:42rem;margin:4rem auto;line-height:1.5"><h1>${t}</h1><p>Served from Chorus. Live data wiring is the prioritized follow-on (#3361).</p></body></html>`;
let ok=0, sh=0;
for (const [view, out, data] of pages) {
  const f = path.join(viewsDir, view + '.ejs');
  let html;
  try { html = ejs.render(fs.readFileSync(f,'utf8'), {cspNonce:'', ...data}, {filename:f, views:[viewsDir]}); ok++; }
  catch (e) { html = shell(data.title || out); sh++; console.log('SHELL', out, '-', String(e.message).split('\n')[0].slice(0,70)); }
  fs.writeFileSync(path.join(outDir, out + '.html'), html);
}
console.log(`rendered ${ok} real, ${sh} shell`);
