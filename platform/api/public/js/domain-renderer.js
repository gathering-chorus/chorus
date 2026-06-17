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
        // #3351 downward recursion — walk hasChild to any depth and render the composition tree (non-fatal).
        return Promise.all([
          get(OWL + '/domains/' + encodeURIComponent(v2) + '/partof').then(renderPartOf),
          buildChildTree(v2, new Set(), 0).then(renderChildTree),
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

  // ---- recursive composition TREE (#3351) — hasChild walked to ANY depth (ADR-041) ----
  // hasChild = structure (domain→domain), NOT contains=content. A node is { name, children:[node...] }.
  // childTreeHtml is pure + recursive: d1→d2→d3 renders as a nested tree, every node a link to its
  // own page. The depth is whatever the model holds; the recursion is built once.
  function childTreeHtml(node) {
    var link = '<a href="?id=' + encodeURIComponent(node.name) + '-domain">' + esc(node.name) + '</a>';
    var kids = (node.children && node.children.length)
      ? '<ul style="list-style:none;margin:.25rem 0 .25rem 1.1rem;padding-left:.6rem;border-left:1px solid var(--surface-2)">' +
          node.children.map(childTreeHtml).join('') + '</ul>'
      : '';
    return '<li style="margin:.15rem 0">' + link + kids + '</li>';
  }
  function countTree(node) { return (node.children || []).reduce(function (n, c) { return n + 1 + countTree(c); }, 0); }
  // async walk: fetch hasChild per node, recurse, cycle-guarded + depth-capped → the node tree.
  function buildChildTree(name, seen, depth) {
    if (depth > 8 || seen.has(name)) return Promise.resolve({ name: name, children: [] });
    seen.add(name);
    return get(OWL + '/domains/' + encodeURIComponent(name) + '/has-child').then(function (p) {
      var kids = (p && p.hasChild) || [];
      return Promise.all(kids.map(function (k) { return buildChildTree(k, seen, depth + 1); }))
        .then(function (nodes) { return { name: name, children: nodes }; });
    });
  }
  function renderChildTree(root) {
    var el = $('haschild-block'); if (!el) return;
    var kids = (root && root.children) || [];
    if (!kids.length) { el.innerHTML = ''; return; }   // honest: a leaf domain renders no tree
    el.innerHTML = '<div class="card"><details open><summary style="cursor:pointer;font-weight:var(--fw-semibold)">Composed of (' + countTree(root) + ')</summary>' +
      '<ul style="list-style:none;padding-left:0;margin:.5rem 0">' + kids.map(childTreeHtml).join('') + '</ul></details></div>';
  }

  // ---- THE SET (#3351 AC3 / #3378 wireframe) — the domains-domain renders the live catalog of ALL domains ----
  // pure row builder (exported, unit-tested). Each row links to that domain's own page.
  function setRowsHtml(rows) {
    return '<table class="table"><thead><tr><th>Domain</th><th>Step</th><th>Owner</th><th>Status</th></tr></thead><tbody>' +
      rows.map(function (r) {
        return '<tr><td><a href="?id=' + encodeURIComponent(r.name) + '">' + esc(r.label || r.name) + '</a></td>' +
          '<td>' + esc(r.step || '?') + '</td><td>' + esc(r.owner || '?') + '</td><td>' + esc(r.status || '?') + '</td></tr>';
      }).join('') + '</tbody></table>';
  }
  // async: fetch the full set from owl-api, enrich each with step/owner/status, render the catalog fold.
  function renderTheSet() {
    return get(OWL + '/domains').then(function (list) {
      var items = (list && list.items) || [];
      if (!items.length) return;
      return Promise.all(items.map(function (it) {
        return get(OWL + '/domains/' + encodeURIComponent(it.name)).then(function (g) {
          return { name: it.name, label: it.label || it.name, status: (g && g.status) || it.status || '?',
                   step: (g && g.atStep && g.atStep.label) || '?', owner: (g && g.ownedBy && g.ownedBy.label) || '?' };
        });
      })).then(function (rows) {
        var slot = $('the-set'); if (!slot) return;
        slot.innerHTML = '<div class="card"><details open><summary style="cursor:pointer;font-weight:var(--fw-semibold)">The Set &mdash; all domains (' + rows.length + ', live catalog)</summary>' +
          '<div style="padding-top:.5rem">' + setRowsHtml(rows) + '</div></details></div>';
      });
    });
  }

  // ---- Cards (derived + reachable) — the domain's board items, each linking through to Vikunja.
  // A standalone section (like THE SET / the tree), not a counted facet.
  // pure builder (exported, unit-tested): the cards table, each row reachable to the board.
  function cardRowsHtml(cards) {
    var cap = 40, shown = cards.slice(0, cap);
    var rows = shown.map(function (c) {
      var oc = (c.owner || '').toLowerCase();
      var owner = ROLE_CLASS[oc] !== undefined ? '<span class="role role--' + esc(oc) + '">' + esc(c.owner) + '</span>' : esc(c.owner || '');
      return '<tr><td><a href="http://localhost:3456/tasks/' + esc(c.id) + '">#' + esc(c.id) + '</a></td>' +
        '<td>' + esc(c.title) + '</td><td><span class="badge">' + esc(c.status) + '</span></td>' +
        '<td>' + esc(c.priority || '') + '</td><td>' + owner + '</td></tr>';
    }).join('');
    var more = cards.length > cap ? '<p class="muted">…and ' + (cards.length - cap) + ' more — <a href="http://localhost:3456">on the board</a></p>' : '';
    return '<table class="table"><thead><tr><th>Card</th><th>Title</th><th>Status</th><th>Pri</th><th>Owner</th></tr></thead><tbody>' + rows + '</tbody></table>' + more;
  }
  function renderCards() {
    return get(ATHENA + '/subdomains/' + encodeURIComponent(id) + '/cards').then(function (b) {
      var el = $('cards-block'); if (!el) return;
      var dd = (b && b.data) || b || {}; var cards = dd.cards || (Array.isArray(dd) ? dd : []);
      if (!cards.length) { el.innerHTML = '<div class="card"><details class="fold"><summary style="cursor:pointer;font-weight:var(--fw-semibold)">Cards (0) <span class="muted">derived</span></summary><p class="muted">No cards on the board for this domain.</p></details></div>'; return; }
      el.innerHTML = '<div class="card"><details open><summary style="cursor:pointer;font-weight:var(--fw-semibold)">Cards (' + cards.length + ') <span class="muted">derived · reachable</span></summary>' + cardRowsHtml(cards) + '</details></div>';
    });
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
    get(ATHENA + '/subdomains/' + encodeURIComponent(id)).then(function (body) {
      // owl-api is the source of record; Athena facets are supplementary + non-fatal (#3378 wireframe).
      // A meta-domain like `domains` has no Athena subdomain record — DON'T bail; let owl-api identity +
      // THE SET render anyway. Only the per-facet fetches come up empty, which renders honestly.
      var d = (body && body.data) || {};
      Promise.resolve(overlayIdentity(d)).then(function () {
        return Promise.all([
          get(ATHENA + '/subdomains/' + encodeURIComponent(id) + '/blast-radius'),
          get(ATHENA + '/subdomains/' + encodeURIComponent(id) + '/cards'),
          // #3468 — completeness is now MODEL-DRIVEN: owl-api computes present-vs-floor
          // from the shape (sh:severity sh:Violation), severing the Athena-v1 dependency.
          get(OWL + '/domains/' + encodeURIComponent(id) + '/completeness'),
        ]).then(function (res) {
          var consumers = (d.consumedBy && d.consumedBy.length) || (res[0] && res[0].data && res[0].data.consumers && res[0].data.consumers.length) || 0;
          var cards = (res[1] && res[1].data && (res[1].data.cards || res[1].data) && (res[1].data.cards || res[1].data).length) || 0;
          renderIdentity(d, consumers, cards);
          renderCompleteness(res[2]); // owl-api returns the gauge unwrapped (not v1 .data)
        });
      });
      // facets — fetch all in parallel, render each as it lands (order preserved by slots)
      var ctx = { label: d.label || id };
      // THE SET (#3378 wireframe): on the domains-domain, the catalog of all domains renders ABOVE the facets.
      var isDomainsRoot = id === 'domains' || id === 'domains-domain';
      $('content-sections').innerHTML = (isDomainsRoot ? '<div id="the-set"></div>' : '') + '<div id="cards-block"></div>' + FACETS.map(function (f) { return '<div id="facet-' + f.key + '"></div>'; }).join('');
      if (isDomainsRoot) renderTheSet();
      renderCards();
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
    module.exports = { esc: esc, unwrap: unwrap, tableFor: tableFor, renderFacet: renderFacet, statCard: statCard, partOfHtml: partOfHtml, childTreeHtml: childTreeHtml, setRowsHtml: setRowsHtml, cardRowsHtml: cardRowsHtml, resolveV2: resolveV2, FACETS: FACETS, ROLE_CLASS: ROLE_CLASS, dlink: dlink };
  }
})();
