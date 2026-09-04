// #4102 — the History fold, one implementation for every page that renders a row.
// Jeff, 2026-09-04: "at staples when we built athena we had a document level and
// field level revision history — this allowed us to run diffs on versions of docs
// to see what changed", and "having a revision history for the doc makes sense
// that is visible on the page". The door keeps each replaced version as a
// chorus:Revision; this reads them and diffs each against the version after it
// (the newest against the row as it stands now), field by field.
//
// Three pages render rows — product, service, document — and a fold copied three
// times drifts three ways, so it lives here and they call it.
// An entity read splits a row in two — literals in `data`, edges in `links`
// tagged `chorus:` (#3635). A snapshot holds the whole row, so the current row
// has to be put back together the same way or every edge reads as removed.
function rowWithLinks(env) {
  const out = Object.assign({}, (env || {}).data || {});
  for (const [k, v] of Object.entries(((env || {}).links) || {})) {
    if (k === 'type') continue;
    out[k] = Array.isArray(v) ? v.map(x => String(x).replace(/^chorus:/, '')) : String(v).replace(/^chorus:/, '');
  }
  return out;
}

async function historyFold(plural, rowname, current) {
  const revEnv = await fetchJSON('/revisions').catch(() => null);
  const mine = (((revEnv || {}).data) || []).filter(r => r.ofRow === `${plural}/${rowname}`)
    .map(r => { let snap = {}; try { snap = JSON.parse(r.snapshot || '{}'); } catch (_) {} return { v: Number(r.version) || 0, at: r.changedAt || snap.changedAt || '', snap }; })
    .sort((a, b) => b.v - a.v);
  const SKIP = ['version', 'changedAt', 'changedIn', 'modified', 'created', 'name', 'iri', 'type'];
  const diffOf = (older, newer) => {
    const keys = Array.from(new Set([...Object.keys(older), ...Object.keys(newer)])).filter(k => !SKIP.includes(k)).sort();
    const norm = v => Array.isArray(v) ? v.map(String).sort() : (v == null ? '' : String(v));
    const out = [];
    for (const k of keys) {
      const a = norm(older[k]), b = norm(newer[k]);
      if (JSON.stringify(a) === JSON.stringify(b)) continue;
      if (Array.isArray(a) || Array.isArray(b)) {
        const A = Array.isArray(a) ? a : (a ? [a] : []), B = Array.isArray(b) ? b : (b ? [b] : []);
        out.push({ k, added: B.filter(x => !A.includes(x)), removed: A.filter(x => !B.includes(x)) });
      } else out.push({ k, from: a, to: b });
    }
    return out;
  };
  // a diagram or a promise runs to thousands of characters; a history line is a
  // pointer to the change, not the document.
  const clip = s => { s = String(s || ''); return s.length > 160 ? s.slice(0, 157) + '…' : s; };
  if (mine.length === 0) {
    return `<section class="ch empty" id="history"><h2><span class="n">&middot;</span>History<span class="src">chorus:Revision &middot; no prior versions kept yet</span></h2></section>`;
  }
  return `<details class="ch" id="history"><summary><h2><span class="n">&middot;</span>History<span class="src">chorus:Revision &middot; ${mine.length} prior version${mine.length === 1 ? '' : 's'}</span></h2></summary>` +
    mine.map((r, i) => {
      const newer = i === 0 ? current : mine[i - 1].snap;
      const d = diffOf(r.snap, newer);
      const label = i === 0 ? `v${r.v} &rarr; now (v${esc(String(current.version || ''))})` : `v${r.v} &rarr; v${mine[i - 1].v}`;
      const rows = d.length === 0 ? '<p class="unwritten">saved again with no field changed</p>' : d.map(x => x.from !== undefined
        ? `<p><b>${esc(x.k)}</b><br><span class="was">${esc(clip(x.from)) || '<i>empty</i>'}</span><br><span class="now">${esc(clip(x.to)) || '<i>empty</i>'}</span></p>`
        : `<p><b>${esc(x.k)}</b> ${x.added.length ? 'added ' + x.added.map(v => esc(clip(v))).join(', ') : ''} ${x.removed.length ? 'removed ' + x.removed.map(v => esc(clip(v))).join(', ') : ''}</p>`).join('');
      return `<details class="rev"><summary>${label} <span class="src">${esc(String(r.at).slice(0, 10))}</span></summary>${rows}</details>`;
    }).join('') + '</details>';
}
