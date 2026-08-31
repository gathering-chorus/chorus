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
  /** #4035 — the run was STOPPED (a person or agent-state ended it). Distinct
   *  from wedged (no output, nobody ended it) and from still-running. */
  stoppedAt?: string;
  stoppedDetail?: string;
  rows: NightlyRow[];
  /** #4009 — liveness. A run that never completed is either working or wedged,
   *  and the page could not tell them apart: on 2026-08-25 a lane sat silent
   *  for 38 minutes while a human was told three different things about it.
   *  quietForMs is the gap since the last row; the page names it. */
  quietForMs?: number;
  lastRowAt?: string;
};

/** Parse the LAST run block (RUN|start … RUN|complete) from the nightly log. */
export function parseNightlyLog(text: string): NightlyRun | null {
  const all = text.split('\n');
  const start = all.reduce((acc, l, i) => (l.startsWith('RUN|start|') ? i : acc), -1);
  if (start === -1) return null;
  const startLine = all.slice(start, start + 1).join('');
  const run: NightlyRun = {
    startedAt: startLine.split('|')[2] ?? '?',
    completed: false,
    rows: [],
  };
  for (const l of all.slice(start + 1)) {
    if (l.startsWith('RUN|complete|')) {
      run.completed = true;
      run.completedAt = l.split('|')[2];
      break;
    }
    if (l.startsWith('RUN|stopped|')) {
      // #4035 — the wrapper's stop handler wrote this; the block ends here.
      run.stoppedAt = l.split('|')[2];
      run.stoppedDetail = l.split('|')[3] ?? '';
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
  // #4009 — how long has this run been silent? Rows carry no timestamps, so the
  // honest source is the log file's own last write, supplied by the caller.
  return run;
}

/** #4009 — a run is WEDGED-LOOKING when it never completed and nothing has been
 *  written for longer than the threshold. Not a verdict on the code — a verdict
 *  on the RUN, which is exactly the distinction that was missing. */
export function quietVerdict(run: NightlyRun, quietMs: number, thresholdMs = 10 * 60 * 1000):
  'complete' | 'live' | 'quiet' {
  if (run.completed) return 'complete';
  return quietMs >= thresholdMs ? 'quiet' : 'live';
}

const esc = (s: string): string =>
  s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

/** #3964 — display form of a suite path: repo-relative, phone-readable.
 *  Old walker rows carry absolute paths; strip the chorus root, label the
 *  app root, and fall back to home-stripping so /Users never hits a screen. */
export function displayPath(p: string): string {
  const m = /^\/[^]*?\/CascadeProjects\/(chorus\/|jeff-bridwell-personal-site(\/|$)|[^/]+\/?)/.exec(p);
  if (!m) return p;
  if (m[1] === 'chorus/') return p.slice(m[0].length) || 'chorus';
  if (m[1].startsWith('jeff-bridwell-personal-site')) {
    const rest = p.slice(m[0].length);
    return rest ? `app:${rest}` : 'app:jeff-bridwell-personal-site';
  }
  return p.slice(m.index + m[0].length - m[1].length);
}

/** Render the run as the one-look report page. */
export function renderNightlyPage(run: NightlyRun | null): string {
  if (!run) {
    return page('Nightly', '<div class="banner empty">No nightly run recorded yet — first run lands at 03:00.</div>');
  }
  const reds = run.rows.filter((r) => r.status === 'fail');
  const skips = run.rows.filter((r) => r.status === 'skip');
  const greens = run.rows.filter((r) => r.status === 'pass');
  const verdict = reds.length === 0 ? 'ALL GREEN' : `${reds.length} RED`;
  const cls = reds.length === 0 ? 'green' : 'red';
  // #4009 — say WHICH not-finished state this is. "PARTIAL" covered both a run
  // still working and a run wedged 38 minutes; a reader could not act on it.
  const quiet = run.quietForMs ?? 0;
  const liveness = quietVerdict(run, quiet);
  const mins = Math.round(quiet / 60000);
  const stoppedBanner = run.stoppedAt
    ? `<div class="banner partial">STOPPED at ${esc(run.stoppedAt)}${run.stoppedDetail ? ' (' + esc(run.stoppedDetail) + ')' : ''} — not a full night; the suites below ran before the stop.</div>`
    : '';
  const liveBanner = liveness === 'quiet'
    ? `<div class="banner partial">NO OUTPUT for ${mins} min — this run started ${esc(run.startedAt)} and has emitted nothing since. Treat it as wedged, not slow.</div>`
    : `<div class="banner partial">RUNNING — started ${esc(run.startedAt)}, last result ${mins} min ago. ${run.rows.length} suite(s) so far; not a full night yet.</div>`;
  const partial = run.completed
    ? ''
    : (stoppedBanner || liveBanner);
  const row = (r: NightlyRow) => `
    <tr class="${esc(r.status)}">
      <td class="st">${esc(r.status)}</td>
      <td class="kind">${esc(r.kind)}</td>
      <td class="path">${esc(displayPath(r.path))}</td>
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
