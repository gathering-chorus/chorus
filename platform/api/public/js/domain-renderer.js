// domain-renderer.js — #3420. The shared renderer for the GENERATED Athena domain page
// (owl-api page_html emits the shell; this fills it). Replaces the retired hand-built domain-detail.js (#3351), onto
// the #3415 system.css vocabulary. Portable, dependency-free, no chorus runtime dep
// (sibling of content-actions.js). Data from the EXISTING Athena/chorus-domain endpoints
// (same-origin) + the owl-api model identity overlay (owner/step/comment).
//
// Anatomy: breadcrumb -> identity -> stats -> promise -> completeness -> 17 facet sections.
// Facets are config-driven (HERALD_FACETS) + a generic renderer; the source badge keeps
// empty states honest (authored-empty = no one wrote it; derived-empty = scanner found none).
(function () {
  'use strict';
  // Browser-globals (location/window) are read lazily in init() so this file is
  // require-able in node to unit-test the pure builders (#3420 page-level tests).
  var id, OWL;
  var ATHENA = '/api/athena';            // same-origin (chorus-api)
  var DOMAIN_API = '/api/chorus/domain';
  var ROLE_CLASS = { jeff: '', wren: 'role--wren', silas: 'role--silas', kade: 'role--kade', borg: 'role--borg' };

  function $(x) { return document.getElementById(x); }
  function esc(s) { return String(s == null ? '' : s).replace(/[&<>"]/g, function (c) { return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]; }); }
  function get(url) { return fetch(url).then(function (r) { return r.ok ? r.json() : null; }).catch(function () { return null; }); }
  function unwrap(b, key, alt) { var d = (b && b.data) || b || {}; var v = d[key] || (alt ? d[alt] : null) || []; return Array.isArray(v) ? v : []; }

  // v1-id → v2-name resolution (#3373; pure + exported). Exact match wins; else strip a
  // -domain/-service suffix onto its v2 home; v1-only ids stay null (never a wrong match).
  function resolveV2(id, names) {
    if (!id || !names) return null;
    if (names.indexOf(id) !== -1) return id;
    var stripped = id.replace(/-(domain|service)$/, '');
    return names.indexOf(stripped) !== -1 ? stripped : null;
  }

  // ---- identity overlay (#3373, non-fatal) ----
  function overlayIdentity(d) {
    return get(OWL + '/domains').then(function (list) {
      if (!list) return;
      var names = (list.items || []).map(function (x) { return x.name; });
      var v2 = resolveV2(id, names);
      if (!v2) return;
      return get(OWL + '/domains/' + encodeURIComponent(v2)).then(function (g) {
        if (g) {
          if (g.comment) d.comment = g.comment;
          if (g.ownedBy && g.ownedBy.label) d.owner = g.ownedBy.label;
          if (g.atStep && g.atStep.label) d.step = g.atStep.label;
          var sb = $('stats-bar'); if (sb) sb.setAttribute('data-identity-source', 'generated-api');
        }
        // AC2 upward direction — render the parent Product/SubProduct from the model (non-fatal).
        // #3351 downward structural recursion — render child domains via hasChild (non-fatal).
        return Promise.all([
          get(OWL + '/domains/' + encodeURIComponent(v2) + '/partof').then(renderPartOf),
          get(OWL + '/domains/' + encodeURIComponent(v2) + '/has-child').then(renderHasChild),
        ]);
      });
    });
  }

  // value is trusted markup (a badge/role span built here); label is escaped (defensive — the fn is exported).
  function statCard(value, label) { return '<div class="card"><div class="stat"><div class="stat-value">' + value + '</div><div class="stat-label">' + esc(label) + '</div></div></div>'; }

  // ---- upward decomposition (#3420 AC2) — parent Product/SubProduct, from owl-api /partof ----
  // pure builder (exported, unit-tested); names come back already-local from the /partof route.
  function partOfHtml(parents) {
    if (!parents || !parents.length) return '';
    var chips = parents.map(function (n) { return '<a class="badge" href="?id=' + encodeURIComponent(n) + '">' + esc(n) + '</a>'; }).join(' ');
    return '<div class="card"><span class="stat-label" style="margin-right:.5rem">Part of (upward)</span>' + chips + '</div>';
  }
  function renderPartOf(p) { var el = $('partof-block'); if (el) el.innerHTML = partOfHtml((p && p.partof) || []); }

  // ---- downward STRUCTURAL recursion (#3351) — child domains via owl-api /has-child ----
  // hasChild = structure (domain→domain), NOT contains=content (ADR-041). The clickable
  // parent→child walk: each child links to its own page, where its own children render.
  function hasChildHtml(children) {
    if (!children || !children.length) return '';
    var chips = children.map(function (n) { return '<a class="badge" href="?id=' + encodeURIComponent(n) + '">' + esc(n) + '</a>'; }).join(' ');
    return '<div class="card"><span class="stat-label" style="margin-right:.5rem">Children (structural)</span>' + chips + '</div>';
  }
  function renderHasChild(p) { var el = $('haschild-block'); if (el) el.innerHTML = hasChildHtml((p && p.hasChild) || []); }

  // ---- the SET / catalog (#3351) — the `domains` meta-domain renders all 34 as a page ----
  // Clickable rows → each domain's own page. Step/owner/status from the enriched GET /domains list.
  function catalogHtml(items) {
    if (!items || !items.length) return '<p class="muted">No domains in the model.</p>';
    var rows = items.map(function (d) {
      // owner/step come back as IRI local names (role-silas, value-stream-step-proving) — clean for display.
      var owner = (d.owner || '').replace(/^role-/, '');
      var step = (d.step || '').replace(/^value-stream-step-/, '');
      var rc = ROLE_CLASS[owner.toLowerCase()] || '';
      return '<tr>' +
        '<td><a href="?id=' + encodeURIComponent(d.name) + '">' + esc(d.label || d.name) + '</a></td>' +
        '<td>' + (step ? '<span class="badge">' + esc(step) + '</span>' : '<span class="muted">—</span>') + '</td>' +
        '<td>' + (owner ? '<span class="role ' + rc + '">' + esc(owner) + '</span>' : '<span class="muted">—</span>') + '</td>' +
        '<td>' + (d.status ? esc(d.status) : '<span class="muted">—</span>') + '</td>' +
      '</tr>';
    }).join('');
    return '<div class="card"><h2 style="margin-top:0">Domains (' + items.length + ')</h2>' +
      '<table class="table"><thead><tr><th>Domain</th><th>Step</th><th>Owner</th><th>Status</th></tr></thead><tbody>' +
      rows + '</tbody></table></div>';
  }
  function renderCatalog(body) { var el = $('content-sections'); if (el) el.innerHTML = catalogHtml((body && body.items) || []); }
  // the `domains` meta-domain page: title + the live set, straight from the generated API.
  function renderCatalogPage() {
    document.title = 'Domains — Athena';
    var t = $('domain-title'); if (t) t.textContent = 'Domains';
    var st = $('domain-subtitle'); if (st) st.textContent = 'The set — every domain in the model';
    var bc = $('bc-domain'); if (bc) bc.textContent = 'Domains';
    get(OWL + '/domains').then(renderCatalog);
  }

  function renderIdentity(d, consumers, cards) {
    document.title = (d.label || id) + ' — Athena';
    $('domain-title').textContent = d.label || id;
    $('domain-subtitle').textContent = d.subtitle || (d.type || 'Domain');
    $('bc-step').textContent = d.step || '—';
    if (d.step) $('bc-step').setAttribute('href', 'step-detail.html?step=' + encodeURIComponent(d.step));
    $('bc-domain').textContent = d.label || id;
    var actions = $('content-actions'); if (actions) { actions.setAttribute('data-title', (d.label || id) + ' — Athena'); actions.setAttribute('data-url', location.href); }
    var owner = d.owner || '?', rc = ROLE_CLASS[(owner || '').toLowerCase()] || '';
    $('stats-bar').innerHTML =
      '<div class="lanes" style="grid-template-columns:repeat(4,minmax(0,1fr))">' +
      statCard('<span class="role ' + rc + '">' + esc(owner) + '</span>', 'Owner') +
      statCard('<span class="badge">' + esc(d.step || '?') + '</span>', 'Primary Step') +
      statCard(String(consumers || 0), 'Consumers') +
      statCard(String(cards || 0), 'Active Cards') + '</div>';
    if (d.comment) $('promise-block').innerHTML = '<div class="callout">' + esc(d.comment) + '</div>';
  }

  function renderCompleteness(c) {
    if (!c) return;
    var pct = c.percentage || 0, tone = pct >= 80 ? 'success' : pct >= 50 ? 'warn' : 'error';
    var h = '<div class="card"><h2 style="margin-top:0">Completeness &mdash; ' + pct + '%</h2>';
    h += '<div style="background:var(--surface-2);border-radius:var(--r-pill);height:10px;margin:.5rem 0 .75rem;overflow:hidden"><div style="height:100%;width:' + pct + '%;background:var(--status-' + tone + ');border-radius:var(--r-pill)"></div></div><div>';
    (c.present || []).forEach(function (s) { h += '<span class="badge badge--success" style="margin:2px 4px 2px 0">' + esc(s) + '</span>'; });
    (c.missing || []).forEach(function (s) { h += '<span class="badge badge--error" style="margin:2px 4px 2px 0">' + esc(s) + '</span>'; });
    h += '</div></div>';
    $('completeness-block').innerHTML = h;
  }

  // ---- facets (the HERALD config — endpoints already exist; this only re-skins) ----
  function dlink(x) { return '<a href="?id=' + encodeURIComponent(x.id || x) + '">' + esc(x.label || x.id || x) + '</a>'; }

  var FACETS = [
    { key: 'dependencies', title: 'Dependencies', src: 'derived', def: 'Typed graph edges — what this domain consumes and is consumed by.',
      fetch: function () { return get(DOMAIN_API + '/' + encodeURIComponent(id) + '/dependencies'); },
      count: function (b) { var d = (b && b.data) || {}; var dir = d.direct || { consumes: [], consumedBy: [] }; return (dir.consumes || []).length + (dir.consumedBy || []).length + (d.shared || []).length; },
      render: function (b) {
        var d = (b && b.data) || {}, dir = d.direct || { consumes: [], consumedBy: [] }, shared = d.shared || [], h = '';
        h += '<div class="lanes" style="grid-template-columns:1fr 1fr">';
        h += '<div><div class="stat-label">depends on</div>' + ((dir.consumes || []).length ? (dir.consumes).map(dlink).join('<br>') : '<span class="muted">none</span>') + '</div>';
        h += '<div><div class="stat-label">consumed by</div>' + ((dir.consumedBy || []).length ? (dir.consumedBy).map(dlink).join('<br>') : '<span class="muted">none</span>') + '</div></div>';
        if (shared.length) { h += '<div class="stat-label" style="margin-top:.75rem">shared infrastructure</div><table class="table"><tbody>' + shared.map(function (s) { return '<tr><td>' + dlink({ id: s.domain, label: s.label }) + '</td><td>' + esc((s.sharedVia || []).join(', ')) + '</td></tr>'; }).join('') + '</tbody></table>'; }
        return h;
      } },
    { key: 'actors', title: 'Actors', src: 'authored', def: 'Who interacts with this domain — roles, services, humans.',
      fetch: function () { return get(ATHENA + '/subdomains/' + encodeURIComponent(id) + '/actors'); },
      count: function (b) { return unwrap(b, 'actors').length; },
      render: function (b, ctx) {
        var items = unwrap(b, 'actors');
        var chart = 'graph LR\n  D["' + (ctx.label || id).replace(/["\n]/g, '') + '"]\n';
        items.forEach(function (a, i) { chart += '  A' + i + '["' + String(a.label || (a.role || '').split('#').pop() || '?').replace(/["\n]/g, '') + '"] -->|"' + String(a.action || 'interacts').replace(/["|\n]/g, '') + '"| D\n'; });
        return '<div class="mermaid">' + chart + '</div>';
      } },
    { key: 'scenarios', title: 'Scenarios', src: 'authored', def: 'BDD given/when/then describing how this domain behaves.',
      fetch: function () { return get(ATHENA + '/subdomains/' + encodeURIComponent(id) + '/scenarios'); },
      count: function (b) { return unwrap(b, 'scenarios').length; },
      render: function (b) {
        return unwrap(b, 'scenarios').map(function (s) {
          var x = '<div style="margin:.5rem 0"><strong>' + esc(s.label || s.title || 'scenario') + '</strong>';
          if (s.given) x += '<div><span class="badge badge--success">Given</span> ' + esc(s.given) + '</div>';
          if (s.when) x += '<div><span class="badge badge--warn">When</span> ' + esc(s.when) + '</div>';
          if (s.then) x += '<div><span class="badge">Then</span> ' + esc(s.then) + '</div>';
          return x + '</div>';
        }).join('');
      } },
    { key: 'pages', title: 'UI Pages', src: 'derived', def: 'HTML pages presenting this domain.', fetch: function () { return get(ATHENA + '/subdomains/' + encodeURIComponent(id) + '/pages'); }, cols: ['route', 'path', 'pageType'], listKey: 'pages' },
    { key: 'integrations', title: 'Integration', src: 'hybrid', def: 'Wire protocols — how this domain talks to others.', fetch: function () { return get(ATHENA + '/subdomains/' + encodeURIComponent(id) + '/integrations'); }, cols: ['label', 'source', 'path', 'status'], listKey: 'integrations' },
    { key: 'endpoints', title: 'API Contract', src: 'derived', def: 'HTTP routes this domain exposes.', fetch: function () { return get(ATHENA + '/subdomains/' + encodeURIComponent(id) + '/services'); }, cols: ['method', 'path', 'handler'], listKey: 'endpoints', altKey: 'services' },
    { key: 'code', title: 'Code', src: 'derived', def: 'Source files implementing this domain.', fetch: function () { return get(ATHENA + '/subdomains/' + encodeURIComponent(id) + '/code'); }, cols: ['path', 'type'], listKey: 'files' },
    { key: 'tests', title: 'Tests', src: 'derived', def: 'Test files exercising this domain.', fetch: function () { return get(DOMAIN_API + '/' + encodeURIComponent(id) + '/tests'); }, cols: ['path', 'type'], listKey: 'tests' },
    { key: 'persistence', title: 'Persistence', src: 'derived', def: 'Data stores this domain reads/writes.', fetch: function () { return get(ATHENA + '/subdomains/' + encodeURIComponent(id) + '/persistence'); }, cols: ['label', 'namespace', 'records', 'status'], listKey: 'stores', altKey: 'persistence' },
    { key: 'pipeline', title: 'Pipeline', src: 'authored', def: 'Build/deploy/CI stages.', fetch: function () { return get(DOMAIN_API + '/' + encodeURIComponent(id) + '/pipeline'); }, cols: ['name', 'status', 'evidence', 'summary'], listKey: 'stages' },
    { key: 'releases', title: 'Release History', src: 'derived', def: 'Shipped versions (git/acp).', fetch: function () { return get(DOMAIN_API + '/' + encodeURIComponent(id) + '/releases'); }, cols: ['timestamp', 'cardId', 'title', 'role', 'commit'], listKey: 'releases' },
    { key: 'infra', title: 'Infrastructure', src: 'derived', def: 'Runtime environment — processes, hosts.', fetch: function () { return get(DOMAIN_API + '/' + encodeURIComponent(id) + '/infra'); }, cols: ['name', 'port', 'engine', 'host'], listKey: 'environments' },
    { key: 'priorArt', title: 'Prior Art', src: 'authored', def: 'References, predecessors, related ADRs.', fetch: function () { return get(ATHENA + '/subdomains/' + encodeURIComponent(id) + '/prior-art'); }, cols: ['label', 'path', 'description'], listKey: 'items' },
    { key: 'decisions', title: 'Decisions', src: 'derived', def: 'DEC/ADR recorded for this domain.', fetch: function () { return get(DOMAIN_API + '/' + encodeURIComponent(id) + '/decisions'); }, cols: ['id', 'title', 'type', 'enforcement', 'date'], listKey: 'decisions' },
    { key: 'logs', title: 'Logs', src: 'derived', def: 'Log streams emitted by this domain.', fetch: function () { return get(DOMAIN_API + '/' + encodeURIComponent(id) + '/logs'); }, cols: ['label', 'location', 'retention', 'status'], listKey: 'logs' },
    { key: 'alerts', title: 'Alerts', src: 'derived', def: "Alert rules monitoring this domain's health.", fetch: function () { return get(DOMAIN_API + '/' + encodeURIComponent(id) + '/alerts'); }, cols: ['name', 'description', 'severity'], listKey: 'alerts' },
    { key: 'gaps', title: 'Gaps & Status', src: 'derived', def: 'Known gaps or incomplete implementation.',
      fetch: function () { return get(ATHENA + '/subdomains/' + encodeURIComponent(id) + '/gaps'); }, count: function (b) { return unwrap(b, 'gaps').length; },
      render: function (b) { return unwrap(b, 'gaps').map(function (g) { var resolved = g.type === 'resolved'; return '<div class="callout ' + (resolved ? '' : 'callout--gap') + '"><strong>' + (resolved ? 'RESOLVED' : 'GAP') + ':</strong> ' + esc(g.description || g.label || '') + (g.severity ? ' (' + esc(g.severity) + ')' : '') + '</div>'; }).join(''); } },
  ];

  function tableFor(items, cols) {
    var h = '<table class="table"><thead><tr>' + cols.map(function (c) { return '<th>' + c.charAt(0).toUpperCase() + c.slice(1) + '</th>'; }).join('') + '</tr></thead><tbody>';
    items.forEach(function (it) {
      h += '<tr>' + cols.map(function (c) {
        var v = it[c]; if (v == null) v = '';
        if (c === 'path' || c === 'commit' || c === 'handler') v = '<code>' + esc(v) + '</code>';
        else if (c === 'cardId' && v) v = '#' + esc(v);
        else if (c === 'timestamp' && typeof v === 'string') v = esc(v.slice(0, 10));
        else if (c === 'route' && typeof v === 'string' && v.charAt(0) === '/') v = '<a href="' + esc(v) + '">' + esc(v) + '</a>';
        else v = esc(v);
        return '<td>' + v + '</td>';
      }).join('') + '</tr>';
    });
    return h + '</tbody></table>';
  }

  function renderFacet(f, body, ctx) {
    var count = f.count ? f.count(body) : unwrap(body, f.listKey, f.altKey).length;
    var inner;
    if (count === 0) inner = '<p class="muted">No ' + f.title.toLowerCase() + ' for this domain.</p>';
    else if (f.render) inner = f.render(body, ctx);
    else inner = tableFor(unwrap(body, f.listKey, f.altKey), f.cols);
    return '<div class="card"><details><summary style="cursor:pointer;font-weight:var(--fw-semibold)">' +
      esc(f.title) + ' (' + count + ') ' +
      '<span class="muted" style="font-size:var(--fs-xs);text-transform:uppercase;letter-spacing:.05em" title="' + esc(f.def) + '">' + esc(f.src) + '</span>' +
      '</summary><div style="padding-top:.5rem">' + inner + '</div></details></div>';
  }

  function init() {
    var params = new URLSearchParams(location.search);
    id = params.get('id') || params.get('name');
    OWL = location.protocol + '//' + location.hostname + ':' + (window.OWL_PORT || 3360); // model identity (non-fatal)
    if (!id) { $('content-sections').innerHTML = '<div class="callout callout--gap">Add <code>?id=&lt;domain&gt;</code> to the URL.</div>'; return; }
    // #3351 — the `domains` meta-domain renders THE SET (catalog), not a single domain's facets.
    if (id === 'domains') { renderCatalogPage(); return; }
    get(ATHENA + '/subdomains/' + encodeURIComponent(id)).then(function (body) {
      if (!body) { $('content-sections').innerHTML = '<div class="callout callout--gap">Could not load <code>' + esc(id) + '</code> from Athena.</div>'; return; }
      var d = body.data || {};
      Promise.resolve(overlayIdentity(d)).then(function () {
        return Promise.all([
          get(ATHENA + '/subdomains/' + encodeURIComponent(id) + '/blast-radius'),
          get(ATHENA + '/subdomains/' + encodeURIComponent(id) + '/cards'),
          get(ATHENA + '/subdomains/' + encodeURIComponent(id) + '/completeness'),
        ]).then(function (res) {
          var consumers = (d.consumedBy && d.consumedBy.length) || (res[0] && res[0].data && res[0].data.consumers && res[0].data.consumers.length) || 0;
          var cards = (res[1] && res[1].data && (res[1].data.cards || res[1].data) && (res[1].data.cards || res[1].data).length) || 0;
          renderIdentity(d, consumers, cards);
          renderCompleteness(res[2] && res[2].data);
        });
      });
      // facets — fetch all in parallel, render each as it lands (order preserved by slots)
      var ctx = { label: d.label || id };
      $('content-sections').innerHTML = FACETS.map(function (f) { return '<div id="facet-' + f.key + '"></div>'; }).join('');
      FACETS.forEach(function (f) {
        f.fetch().then(function (b) {
          var slot = $('facet-' + f.key); if (slot) slot.innerHTML = renderFacet(f, b, ctx);
          if (f.key === 'actors' && window.mermaid) { try { window.mermaid.run({ querySelector: '#facet-actors .mermaid' }); } catch (e) {} }
        });
      });
    });
  }
  // browser: wire on load. node/test: skip (no document) + export the pure builders.
  if (typeof document !== 'undefined') document.addEventListener('DOMContentLoaded', init);
  if (typeof module !== 'undefined' && module.exports) {
    module.exports = { esc: esc, unwrap: unwrap, tableFor: tableFor, renderFacet: renderFacet, statCard: statCard, partOfHtml: partOfHtml, hasChildHtml: hasChildHtml, catalogHtml: catalogHtml, resolveV2: resolveV2, FACETS: FACETS, ROLE_CLASS: ROLE_CLASS, dlink: dlink };
  }
})();
