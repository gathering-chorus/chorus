// #4093 — the ONE renderer for the domain's materialized facets (cards, code, tests, logs,
// alerts, decisions, deps, …). domain.html draws all 17 for a domain; service.html composes
// the ones a service needs (Jeff, 2026-09-03: the domains show TRUE state; a service page
// reads its hosting domain's facets and adds only what a service owns — interfaces,
// runs-as, depends-on, commitments, flows). A second copy of this table anywhere is the
// defect this file exists to prevent (negative proof in service-engineering-4093.test.ts).
// #4084's Logs fold (chorus:LogSource rows read from the graph, filtered on hasDomain) and
// the phone-safe scrolling table live here now, once, for both pages.
//
// renderFacetTables(el, fid, { only: ['Tests', ...], heading: t => label, src: 'text', domain: 'tests' })
//   fid     the materialized-view id (<name>-domain or bare name; resolveFacetId finds it)
//   only    optional subset of facet titles, rendered in the order given
//   domain  the bare domain name (defaults to fid without -domain); the graph folds filter on it
async function resolveFacetId(d) {
  const ATHENA = '/api/athena/subdomains/';
  let fid = `${d}-domain`;
  const probe = await fetch(ATHENA + encodeURIComponent(fid)).then(r => r.ok ? r.json() : null).catch(() => null);
  if (!probe || probe.error) fid = d;
  return fid;
}
async function renderFacetTables(el, fid, opts) {
  opts = opts || {};
  const dname = opts.domain || String(fid).replace(/-domain$/, '');
  // one page-wide model base (#4064/#4084): the page may define OWL; otherwise the same-origin /owl proxy
  const OWL = (typeof window.OWL === 'string') ? window.OWL : ((typeof window.basePath === 'function') ? window.basePath('/owl') : '/owl');
  const ATHENA = '/api/athena/subdomains/';
  const DOM = '/api/chorus/domain/';
  const FACETS = [
    { t: 'Cards', u: ATHENA + fid + '/cards', k: 'cards', cols: ['id', 'title', 'owner', 'status', 'priority'] },
    { t: 'Dependencies', u: DOM + fid + '/dependencies', k: '_deps' },
    { t: 'API Contract', u: ATHENA + fid + '/services', k: 'endpoints', alt: 'services', cols: ['method', 'path', 'handler'] },
    { t: 'UI Pages', u: ATHENA + fid + '/pages', k: 'pages', cols: ['route', 'path', 'pageType'] },
    { t: 'Code', u: ATHENA + fid + '/code', k: 'files', cols: ['path', 'type'] },
    { t: 'Tests', u: DOM + fid + '/tests', k: 'tests', cols: ['path', 'type'] },
    { t: 'Persistence', u: ATHENA + fid + '/persistence', k: 'stores', alt: 'persistence', cols: ['label', 'namespace', 'records', 'status'] },
    { t: 'Pipeline', u: DOM + fid + '/pipeline', k: 'stages', cols: ['name', 'status', 'summary'] },
    { t: 'Releases', u: DOM + fid + '/releases', k: 'releases', cols: ['timestamp', 'cardId', 'title', 'role'] },
    { t: 'Infrastructure', u: DOM + fid + '/infra', k: 'environments', cols: ['name', 'port', 'engine', 'host'] },
    { t: 'Decisions', u: DOM + fid + '/decisions', k: 'decisions', cols: ['id', 'title', 'type', 'date'] },
    // #4084 — the FIRST fold read from the graph, not a v1 registry: chorus:LogSource rows (harvested hourly by
    // log-harvest.sh) carrying hasDomain from the authored UnitDomainMapping rows. Filtered client-side on the
    // domain edge; the query is shown in the fold so a zero is 'no rows for this domain', never a lookup miss.
    { t: 'Logs', u: OWL + '/logsources?limit=500', k: 'items', graph: true, filter: r => tail(r.hasDomain || (r.links && r.links.hasDomain) || '') === dname,
      cols: ['launchdLabel', 'logStatus', 'lokiJob', 'lastWrittenAt', 'logPath'], src: 'chorus:LogSource · hasDomain = ' + dname },
    { t: 'Alerts', u: DOM + fid + '/alerts', k: 'alerts', cols: ['name', 'description', 'severity'] },
    { t: 'Integration', u: ATHENA + fid + '/integrations', k: 'integrations', cols: ['label', 'source', 'status'] },
    { t: 'Actors', u: ATHENA + fid + '/actors', k: 'actors', cols: ['label', 'role', 'action'] },
    { t: 'Scenarios', u: ATHENA + fid + '/scenarios', k: 'scenarios', cols: ['label', 'given', 'when', 'then'] },
    { t: 'Gaps & Status', u: ATHENA + fid + '/gaps', k: 'gaps', cols: ['type', 'description', 'severity'] },
  ];
  function tail(iri) { return String(iri || '').split('#').pop().replace(/^chorus:/, ''); }
  function unwrap(b, k, alt) {
    if (b && Array.isArray(b.data)) return b.data;   // #4084: an athena-make collection envelope carries its rows in `data`
    const dd = (b && b.data) || b || {};
    const v = dd[k] || (alt ? dd[alt] : null) || [];
    return Array.isArray(v) ? v : [];
  }
  function table(items, cols) {
    const head = cols.map(c => `<th>${esc(c)}</th>`).join('');
    const rows = items.slice(0, 40).map(it =>
      `<tr>${cols.map(c => { const v = String(it[c] ?? ''); const short = /^\/.+\//.test(v) && v.length > 28 ? `<span title="${esc(v)}">…/${esc(v.split('/').pop())}</span>` : esc(v.slice(0, 120)); return `<td>${c === 'id' && it[c] ? '#' + esc(it[c]) : short}</td>`; }).join('')}</tr>`).join('');
    const more = items.length > 40 ? `<p class="empty">…and ${items.length - 40} more</p>` : '';
    return `<div class="tbl"><table><tr>${head}</tr>${rows}</table></div>${more}`;   // #4084: Jeff on his phone: the Logs table ran off the right edge; the fold scrolls its own table now
  }
  const CHOSEN = opts.only ? opts.only.map(t => FACETS.find(f => f.t === t)).filter(Boolean) : FACETS;
  const bodies = await Promise.all(CHOSEN.map(f =>
    fetch(f.u).then(r => r.ok ? r.json() : null).catch(() => null)));
  el.innerHTML = CHOSEN.map((f, i) => {
    const b = bodies[i];
    let inner, count;
    if (f.k === '_deps') {
      const dd = (b && b.data) || {};
      const dir = dd.direct || { consumes: [], consumedBy: [] };
      count = (dir.consumes || []).length + (dir.consumedBy || []).length;
      inner = count
        ? `<div><span class="lbl">depends on</span> ${(dir.consumes || []).map(x => `<a class="chip" href="domain.html?d=${encodeURIComponent(String(x.id || x).replace(/-domain$/, ''))}">${esc(x.label || x.id || x)}</a>`).join('') || '<span class="empty">none</span>'}</div>
           <div style="margin-top:6px"><span class="lbl">consumed by</span> ${(dir.consumedBy || []).map(x => `<a class="chip" href="domain.html?d=${encodeURIComponent(String(x.id || x).replace(/-domain$/, ''))}">${esc(x.label || x.id || x)}</a>`).join('') || '<span class="empty">none</span>'}</div>`
        : '<div class="empty">none recorded</div>';
    } else {
      const items = unwrap(b, f.k, f.alt);
      const rows = f.filter ? items.filter(f.filter) : items;
      count = rows.length;
      inner = count ? table(rows, f.cols) : `<div class="empty">no ${f.t.toLowerCase()} for this domain</div>`;
    }
    const heading = opts.heading ? opts.heading(f.t) : f.t;
    const srcTxt = f.graph ? esc(f.src) + ' &middot; graph' : (opts.src ? opts.src : 'materialized &middot; live');
    return `<details class="fold"${count ? ' open' : ''}><summary>${esc(heading)} (${count}) <span class="src">${srcTxt}</span></summary><div class="body">${inner}</div></details>`;
  }).join('');
}
