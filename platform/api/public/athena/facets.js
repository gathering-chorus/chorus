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
// The facet id is the bare domain name.
async function resolveFacetId(d) {
  return d;
}
async function renderFacetTables(el, fid, opts) {
  opts = opts || {};
  const dname = opts.domain || String(fid).replace(/-domain$/, '');
  // one page-wide model base (#4064/#4084): the page may define OWL; otherwise the same-origin /owl proxy
  // #5040 — both bases go through window.basePath (the one mount helper every page that loads
  // this file defines in its head), asked here at call time. The model base was taken from a
  // page constant captured before the page knew its mount, and the facet base was root-absolute
  // with no mount: under lightlifeurbangardens.com/chorus every fold asked the garden site
  // (Jeff's phone, 2026-11-25 08:02: "could not read … HTTP 404" down the page).
  const OWL = window.basePath('/owl');
  const DOM = window.basePath('/api/chorus/domain/');
  // #4353 — code, pages and endpoints are graph rows carrying hasDomain; read them
  // like the Logs fold.
  // hasDomain is one name or a list of them (115 of 6,329 code files name several)
  const onDomain = r => [].concat(r.hasDomain || (r.links && r.links.hasDomain) || []).some(v => tail(v) === dname);
  // #5040 — the door filters on hasDomain itself. The page read whole tables (20,000 code files,
  // 14 to 19 s) to keep 50 rows; a phone connection gave up ("could not read Code: no answer").
  // The client filter stays, so a door that ignored the filter could not widen what is shown.
  const forDomain = '?hasDomain=' + encodeURIComponent(dname) + '&limit=5000';
  // #5041 — listed in the order a reader needs: what the domain exposes, whether it is healthy,
  // then the work and the code. A page that draws them all (no `only`) draws them in this order.
  // The NAMES are fixed by the model's Domain shape and the folds counter; only the order moved.
  const FACETS = [
    { t: 'API Contract', u: OWL + '/code/endpoints' + forDomain, k: 'items', graph: true, filter: onDomain, cols: ['httpMethod', 'routePath', 'filePath'], src: 'chorus:Endpoint · hasDomain = ' + dname },
    { t: 'Dependencies', u: DOM + fid + '/dependencies', k: '_deps' },
    { t: 'Decisions', u: DOM + fid + '/decisions', k: 'decisions', cols: ['id', 'title', 'type', 'date'] },
    // #5041 — Jeff, 2026-11-25 10:59: "the ui interfaces fold must give clickable links". A route
    // is the page's own address, so it is a link to that page, through the mount.
    { t: 'UI Pages', u: OWL + '/code/pages' + forDomain, k: 'items', graph: true, filter: onDomain, cols: ['route', 'filePath', 'pageType'], src: 'chorus:Page · hasDomain = ' + dname,
      link: { route: v => v.charAt(0) === '/' ? window.basePath(v) : null } },
    { t: 'Alerts', u: DOM + fid + '/alerts', k: 'alerts', cols: ['name', 'file', 'source'] },   // #4285: rows from the alerts graph
    // #4084 — the FIRST fold read from the graph, not a v1 registry: chorus:LogSource rows (harvested hourly by
    // log-harvest.sh) carrying hasDomain from the authored UnitDomainMapping rows. Filtered client-side on the
    // domain edge; the query is shown in the fold so a zero is 'no rows for this domain', never a lookup miss.
    { t: 'Logs', u: OWL + '/logs/sources' + forDomain, k: 'items', graph: true, filter: r => tail(r.hasDomain || (r.links && r.links.hasDomain) || '') === dname,
      cols: ['launchdLabel', 'logStatus', 'lokiJob', 'lastWrittenAt', 'logPath'], src: 'chorus:LogSource · hasDomain = ' + dname },
    // #5021 — its own route: the board's domain:<name> cards as a list (the old read answered 25 v1 names only)
    // #5044 — a card number is a link to the card (Jeff, 2026-11-25 15:50: "cant link to cards
    // like clearing v2 does"); cardHref knows where Clearing v2 is from here. The link is HERE
    // only: Releases names the same cards, and one piece of data has one link on this page
    // (his rule of 10:40, held by the links suite). Releases shows the number as "#5033", unlinked.
    { t: 'Cards', u: DOM + dname + '/cards', k: 'cards', cols: ['id', 'title', 'owner', 'status', 'priority'], link: { id: v => cardHref(v) } },
    // #5044 — each row counts what the fold beside it lists; `dname`, like Cards: any domain's name is asked
    // (its step names are one short word each: one line; and the table is `fit`: it takes the fold's
    // width and the summary wraps inside it. Wider than the fold, the summary ran off the right
    // edge on one line: the cut Jeff photographed at 15:45, read again from this page at 19:47)
    { t: 'Pipeline', u: DOM + dname + '/pipeline', k: 'stages', cols: ['name', 'status', 'summary'], one: ['name'], fit: true },
    { t: 'Releases', u: DOM + fid + '/releases', k: 'releases', cols: ['timestamp', 'cardId', 'title', 'role'] },
    { t: 'Infrastructure', u: DOM + fid + '/infra', k: 'environments', cols: ['name', 'port', 'engine', 'host'] },
    { t: 'Tests', u: DOM + fid + '/tests', k: 'tests', cols: ['path', 'type'] },
    { t: 'Code', u: OWL + '/code/files' + forDomain, k: 'items', graph: true, filter: onDomain, cols: ['filePath', 'hasLanguage'], src: 'chorus:SourceFile · hasDomain = ' + dname },
  ];
  // #5041 — Jeff's phone, 2026-11-25: a card id drawn one character a line (#/5/0/2/1), a release
  // time broken over three lines, a path cut mid-word. The fold's wrap-anywhere rule did that to
  // every narrow column. These columns are one line each (class "one"; athena-flow.css decides
  // what that means, and the table scrolls inside its fold); prose columns still wrap. A route
  // and its method are addresses too: beside a one-line file path the UI Pages route wrapped
  // five letters a line (Borg Queen at 390px, 2026-11-25 12:34).
  const ONE_LINE = ['id', 'cardId', 'owner', 'role', 'status', 'priority', 'timestamp', 'filePath', 'path', 'file', 'logPath', 'route', 'routePath', 'httpMethod'];
  // how a column's value is shown: a card id with its #, a stored time in the viewer's own zone
  // (localTime gives '' for a value that is not a time: the stored text is shown then), a bare owner
  const SHOWN = { id: v => '#' + v, cardId: v => '#' + v, timestamp: v => localTime(v) || v, owner: v => ownerLocal(v), role: v => ownerLocal(v) };
  function tail(iri) { return String(iri || '').split('#').pop().replace(/^chorus:/, ''); }
  function unwrap(b, k, alt) {
    if (b && Array.isArray(b.data)) return b.data;   // #4084: an athena-make collection envelope carries its rows in `data`
    const dd = (b && b.data) || b || {};
    const v = dd[k] || (alt ? dd[alt] : null) || [];
    return Array.isArray(v) ? v : [];
  }
  // `link` (optional) maps a column to the address its value opens; no address, plain text
  // `one` (optional) names more columns of THIS fold that are one line each; `fit` (optional)
  // marks a table that is to fit its fold, not scroll
  function table(items, cols, link, one, fit) {
    const oneLine = c => ONE_LINE.includes(c) || (one || []).includes(c);
    const cell = (c, v) => {
      const text = esc(v && SHOWN[c] ? SHOWN[c](v) : v);
      const href = v && link && link[c] ? link[c](v) : null;
      return href ? `<a class="edge" href="${esc(href)}">${text}</a>` : text;
    };
    const head = cols.map(c => `<th>${esc(c)}</th>`).join('');
    const rows = items.slice(0, 40).map(it =>
      // #5034 — the whole value, always: a path was shortened to '…/name' with the rest in a hover
      // title, which a phone cannot read (and it mangled any text that merely began with a slash)
      `<tr>${cols.map(c => { const v = String(it[c] ?? ''); return `<td${oneLine(c) ? ' class="one"' : ''}>${cell(c, v)}</td>`; }).join('')}</tr>`).join('');
    const more = items.length > 40 ? `<p class="empty">…and ${items.length - 40} more</p>` : '';
    return `<div class="tbl"><table${fit ? ' class="fit"' : ''}><tr>${head}</tr>${rows}</table></div>${more}`;   // #4084: Jeff on his phone: the Logs table ran off the right edge; the fold scrolls its own table now
  }
  // #4285 — `except` lets a page mount some folds elsewhere (Actors/Scenarios by the Model, gaps in the Gaps fold)
  const CHOSEN = opts.only ? opts.only.map(t => FACETS.find(f => f.t === t)).filter(Boolean)
    : opts.except ? FACETS.filter(f => !opts.except.includes(f.t)) : FACETS;
  const bodies = await Promise.all(CHOSEN.map(f =>
    // #5021 — a read that fails is kept as a failure (status + message), never turned into an empty fold
    fetch(f.u).then(async r => r.ok ? r.json() : { _failed: 'HTTP ' + r.status, _body: await r.json().catch(() => null) })
      .catch(e => ({ _failed: 'no answer (' + (e && e.message || 'network') + ')' }))));
  el.innerHTML = CHOSEN.map((f, i) => {
    const b = bodies[i];
    let inner, count;
    if (b && b._failed) {
      const why = b._body && ((b._body.data && b._body.data.error) || b._body.error);
      const heading = opts.heading ? opts.heading(f.t) : f.t;
      return `<details class="fold" open><summary>${esc(heading)} (?) <span class="src">could not read</span></summary><div class="body"><div class="bad">could not read ${esc(f.t)}: ${esc(b._failed)}${why ? ' &middot; ' + esc(why) : ''}</div></div></details>`;
    }
    if (f.k === '_deps') {
      const dd = (b && b.data) || {};
      const dir = dd.direct || { consumes: [], consumedBy: [] };
      count = (dir.consumes || []).length + (dir.consumedBy || []).length;
      // #4293 — the CMDB half: which layer the domain sits in and which services it
      // hosts, shown even when it has no explicit dependency (a foundation domain
      // with none is still a fact worth reading).
      const layerLine = dd.layer
        ? `<div style="margin-bottom:6px"><span class="lbl">layer</span> <span class="chip">${esc(dd.layer.label || dd.layer.id)}</span></div>`
        : '<div style="margin-bottom:6px"><span class="lbl">layer</span> <span class="empty">none recorded</span></div>';
      const hostsLine = (dd.hosts || []).length
        ? `<div style="margin-bottom:6px"><span class="lbl">hosts</span> ${dd.hosts.map(x => `<span class="chip">${esc(x.label || x.id)}</span>`).join('')}</div>`
        : '';
      const failed = b && b._meta && b._meta.error ? `<div class="empty">could not read dependencies: ${esc(b._meta.message || 'store error')}</div>` : '';
      inner = layerLine + hostsLine + failed + (count
        ? `<div><span class="lbl">depends on</span> ${(dir.consumes || []).map(x => `<a class="chip" href="domain.html?d=${encodeURIComponent(String(x.id || x).replace(/-domain$/, ''))}">${esc(x.label || x.id || x)}</a>`).join('') || '<span class="empty">none</span>'}</div>
           <div style="margin-top:6px"><span class="lbl">consumed by</span> ${(dir.consumedBy || []).map(x => `<a class="chip" href="domain.html?d=${encodeURIComponent(String(x.id || x).replace(/-domain$/, ''))}">${esc(x.label || x.id || x)}</a>`).join('') || '<span class="empty">none</span>'}</div>`
        : '<div class="empty">no dependencies recorded</div>');
    } else {
      const items = unwrap(b, f.k, f.alt);
      const rows = f.filter ? items.filter(f.filter) : items;
      count = rows.length;
      // #4285 — a zero names the graph that was asked, so "0" can never mean
      // "the page asked the wrong graph" without saying so.
      const askedGraph = b && b._meta && b._meta.graph ? ` &middot; graph asked: <code>${esc(b._meta.graph)}</code>` : '';
      inner = count ? table(rows, f.cols, f.link, f.one, f.fit) : `<div class="empty">no ${f.t.toLowerCase()} for this domain${askedGraph}</div>`;
    }
    // #5021 — an empty fold shows the Domain row's written reason for it (chorus:foldNote "<Fold>: <why>")
    const note = !count && opts.notes && opts.notes[f.t];
    if (note) inner = `<p class="note"><strong>Why empty:</strong> ${esc(note)}</p>` + inner;
    const heading = opts.heading ? opts.heading(f.t) : f.t;
    // #4285 — the Tests fold lists FILES; the store counts CASES. Say both.
    const total = b && b.data && typeof b.data.total === 'number' ? b.data.total : null;
    const countTxt = f.k === 'tests' && total !== null && total !== count ? `${count} files &middot; ${total} cases` : String(count);
    const srcTxt = f.graph ? esc(f.src) + ' &middot; graph' : (opts.src ? opts.src : 'materialized &middot; live');
    return `<details class="fold"${count ? ' open' : ''}><summary>${esc(heading)} (${countTxt}) <span class="src">${srcTxt}</span></summary><div class="body">${inner}</div></details>`;
  }).join('');
}
