// trace-page-run.js — load the served trace reader page's own <script> in node with a
// minimal DOM and a stubbed fetch, open it at ?card=<n>, and print what it rendered:
// the summary line and the pipeline chip of every row, in order (#4336 — the page
// was checked by grepping its source for identifiers).
//   node trace-page-run.js <trace.html> <events.json>
const fs = require('fs');
const [html, eventsFile] = process.argv.slice(2);
const src = fs.readFileSync(html, 'utf8');
const m = src.match(/<script>([\s\S]*?)<\/script>/);
if (!m) { console.error('no inline <script> in ' + html); process.exit(2); }
const events = JSON.parse(fs.readFileSync(eventsFile, 'utf8'));
const els = {};
function el(id) {
  if (els[id]) return els[id];
  const e = {
    id, value: '', innerHTML: '', textContent: '',
    addEventListener() {},
    querySelectorAll(sel) {
      // the workflow filter chips: every <input value=...> rendered into this element, all checked
      if (!/^input/.test(sel)) return [];
      return [...e.innerHTML.matchAll(/<input[^>]*value="([^"]*)"[^>]*>/g)]
        .map(x => ({ value: x[1], checked: true, addEventListener() {} }));
    },
  };
  return (els[id] = e);
}
global.window = global;
global.document = { querySelector: s => el(s.replace(/^#/, '')) };
global.location = { search: '?card=4195' };
global.fetch = async () => ({ ok: true, status: 200, json: async () => ({ ok: true, events }) });
new Function(m[1])();
setTimeout(() => {
  const chips = [...els.stream.innerHTML.matchAll(/<span class="chip" style="background:([^"]*)">(werk|athena)<\/span>/g)]
    .map(x => x[2] + '@' + x[1]);
  console.log('SUM ' + els.sum.innerHTML.replace(/<[^>]+>/g, ''));
  console.log('CHIPS ' + chips.join(','));
}, 50);
