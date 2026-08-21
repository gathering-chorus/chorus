// #3920 — /nightly: the rendered viewing surface for the nightly run.
// One page answering "was the night green?" — verdict first, reds on top,
// typed skips counted, killed runs named PARTIAL. Reads the same log
// nightly-suites.sh writes (RUN|/SUITE| lines): the page renders the record,
// it never re-derives verdicts (one verdict vocabulary, #3920 fold).

export type NightlyRow = {
  kind: string;
  path: string;
  owner: string;
  status: string; // pass | fail | skip
  summary: string;
};

export type NightlyRun = {
  startedAt: string;
  completedAt?: string;
  completed: boolean;
  rows: NightlyRow[];
};

/** Parse the LAST run block (RUN|start … RUN|complete) from the nightly log. */
export function parseNightlyLog(text: string): NightlyRun | null {
  const lines = text.split('\n');
  let start = -1;
  for (let i = lines.length - 1; i >= 0; i--) {
    if (lines[i].startsWith('RUN|start|')) { start = i; break; }
  }
  if (start === -1) return null;
  const run: NightlyRun = {
    startedAt: lines[start].split('|')[2] ?? '?',
    completed: false,
    rows: [],
  };
  for (let i = start + 1; i < lines.length; i++) {
    const l = lines[i];
    if (l.startsWith('RUN|complete|')) {
      run.completed = true;
      run.completedAt = l.split('|')[2];
      break;
    }
    if (l.startsWith('SUITE|')) {
      // summary may itself contain '|'-free text; split into 6 parts max.
      const parts = l.split('|');
      if (parts.length >= 6) {
        run.rows.push({
          kind: parts[1],
          path: parts[2],
          owner: parts[3],
          status: parts[4],
          summary: parts.slice(5).join('|'),
        });
      }
    }
  }
  return run;
}

const esc = (s: string): string =>
  s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

/** Render the run as the one-look report page. */
export function renderNightlyPage(run: NightlyRun | null): string {
  if (!run) {
    return page('Nightly', `<div class="banner empty">No nightly run recorded yet — first run lands at 03:00.</div>`);
  }
  const reds = run.rows.filter((r) => r.status === 'fail');
  const skips = run.rows.filter((r) => r.status === 'skip');
  const greens = run.rows.filter((r) => r.status === 'pass');
  const verdict = reds.length === 0 ? 'ALL GREEN' : `${reds.length} RED`;
  const cls = reds.length === 0 ? 'green' : 'red';
  const partial = run.completed
    ? ''
    : `<div class="banner partial">PARTIAL — this run started ${esc(run.startedAt)} and never completed; results below are not a full night.</div>`;
  const row = (r: NightlyRow) => `
    <tr class="${esc(r.status)}">
      <td class="st">${esc(r.status)}</td>
      <td class="kind">${esc(r.kind)}</td>
      <td class="path">${esc(r.path)}</td>
      <td>${esc(r.owner)}</td>
      <td class="sum">${esc(r.summary)}</td>
    </tr>`;
  const ordered = [...reds, ...skips, ...greens];
  const body = `
  ${partial}
  <div class="banner ${cls}">
    <span class="verdict">${verdict}</span>
    <span class="counts">${greens.length} passed · ${reds.length} failed · ${skips.length} skipped</span>
    <span class="when">${esc(run.startedAt)}${run.completedAt ? ' → ' + esc(run.completedAt) : ''}</span>
  </div>
  <table>
    <thead><tr><th></th><th>tier</th><th>suite</th><th>owner</th><th>result</th></tr></thead>
    <tbody>${ordered.map(row).join('')}</tbody>
  </table>
  <p class="prov">cargo tier runs via <code>werk-test --nightly</code> — registry selection, nextest, typed needs-stack skips (#3920). Page renders the run record verbatim; it holds no verdict of its own.</p>`;
  return page(`Nightly — ${verdict}`, body);
}

function page(title: string, body: string): string {
  return `<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${esc(title)}</title>
<style>
  :root { --bg:#fff; --fg:#1a1a1a; --mut:#667; --red:#c0392b; --green:#1e7d32; --amber:#8a6d1a; --line:#e5e5ea; }
  @media (prefers-color-scheme: dark) { :root { --bg:#131316; --fg:#eee; --mut:#99a; --red:#ff6b5e; --green:#5dd879; --amber:#e3c05a; --line:#2a2a30; } }
  body { background:var(--bg); color:var(--fg); font:15px/1.5 -apple-system,system-ui,sans-serif; max-width:70rem; margin:2rem auto; padding:0 1rem; }
  .banner { padding:1rem 1.25rem; border-radius:10px; margin-bottom:1rem; display:flex; gap:1rem; align-items:baseline; flex-wrap:wrap; }
  .banner.green { background:color-mix(in srgb, var(--green) 12%, transparent); }
  .banner.red { background:color-mix(in srgb, var(--red) 12%, transparent); }
  .banner.partial, .banner.empty { background:color-mix(in srgb, var(--amber) 14%, transparent); }
  .verdict { font-size:1.6rem; font-weight:700; }
  .banner.green .verdict { color:var(--green); } .banner.red .verdict { color:var(--red); }
  .counts, .when { color:var(--mut); }
  table { width:100%; border-collapse:collapse; }
  th { text-align:left; color:var(--mut); font-weight:600; padding:.4rem .5rem; border-bottom:1px solid var(--line); }
  td { padding:.35rem .5rem; border-bottom:1px solid var(--line); vertical-align:top; }
  tr.fail .st { color:var(--red); font-weight:700; }
  tr.pass .st { color:var(--green); }
  tr.skip .st { color:var(--amber); }
  .path { font-family:ui-monospace,monospace; font-size:.85rem; word-break:break-all; }
  .sum { color:var(--mut); }
  .prov { color:var(--mut); font-size:.85rem; margin-top:1.25rem; }
  code { font-family:ui-monospace,monospace; }
</style></head><body>${body}</body></html>`;
}
