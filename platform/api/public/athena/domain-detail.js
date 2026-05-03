if (typeof mermaid !== 'undefined') { mermaid.initialize({ startOnLoad: false, theme: 'neutral' }); }
const ATHENA = '/api/athena';
const DOMAIN_API = '/api/chorus/domain'; // #2060: consolidated facet endpoints

// #2431 — per-instance enrichment cache keyed by URI. Populated on first
// fetch of /api/loom/principles, /api/loom/policies, etc. and read by the
// type-specific renderers below.
const INSTANCE_ENRICHMENT = {};

function escapeHtml(s) {
  if (s === null || s === undefined) return '';
  return String(s).replace(/[&<>"']/g, function(c) {
    return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c];
  });
}

// #2431 — type dispatch. Custom renderer per rdf:type; generic fallback is
// the default. New types drop in as one function + one map entry.
const TYPE_RENDERERS = {
  Principle: renderPrinciple,
  Policy: renderPolicy,
  Decision: renderDecision,
};

function renderInstanceBody(inst, typeName) {
  var fn = TYPE_RENDERERS[typeName] || renderGeneric;
  return fn(inst);
}

function renderGeneric(inst) {
  var html = '<details class="instance-body"><summary style="cursor:pointer;padding:6px 0;color:#0369a1;font-weight:500;">' + escapeHtml(inst.label) + '</summary>';
  html += '<div style="padding:0 0 12px 16px;">';
  if (inst.comment) html += '<p>' + escapeHtml(inst.comment) + '</p>';
  html += '</div></details>';
  return html;
}

// #2431 — Principle renderer branches on isPermacultureParent:
//   true  → three panels (Permaculture / Tech / Jeff) — the parent shape
//   false → one panel (comment) + "Specialization of <parent>" — the child shape
// Chorus specializations have techReading/jeffReading as empty strings; the
// Permaculture label is wrong for them because their rdfs:comment IS the
// Chorus-specific reading, not the permaculture text.
function renderPrinciple(inst) {
  var enriched = INSTANCE_ENRICHMENT[inst.uri] || {};
  var isParent = enriched.isPermacultureParent === true;
  var html = '<details class="instance-body"><summary style="cursor:pointer;padding:6px 0;color:#0369a1;font-weight:500;">' + escapeHtml(inst.label) + '</summary>';
  html += '<div style="padding:0 0 12px 16px;">';
  if (isParent) {
    if (inst.comment) html += '<p><strong>Permaculture:</strong> ' + escapeHtml(inst.comment) + '</p>';
    if (enriched.techReading) html += '<p><strong>Tech:</strong> ' + escapeHtml(enriched.techReading) + '</p>';
    if (enriched.jeffReading) html += '<p><strong>Jeff:</strong> ' + escapeHtml(enriched.jeffReading) + '</p>';
  } else {
    if (inst.comment) html += '<p>' + escapeHtml(inst.comment) + '</p>';
    if (enriched.parents && enriched.parents.length > 0) {
      html += '<p style="font-size:0.9em;color:#64748b;">Specialization of: ';
      html += enriched.parents.map(function(p) { return escapeHtml(p.label || p.id || ''); }).join(', ');
      html += '</p>';
    }
  }
  html += '</div></details>';
  return html;
}

// #2431 — Principle-specific nested render: 12 parents at top, each with
// their Chorus specializations nested under them by parents[] edges.
// Mirrors the /loom/principles.html prior-art page's hierarchy.
function renderPrincipleTree(principleInstances) {
  // Build parent-id → children map using enrichment.parents[]
  var parents = [];
  var childrenByParentId = {};
  principleInstances.forEach(function(inst) {
    var e = INSTANCE_ENRICHMENT[inst.uri] || {};
    if (e.isPermacultureParent === true) {
      parents.push(inst);
    } else {
      (e.parents || []).forEach(function(p) {
        var pid = p.id || (p.uri ? p.uri.split('#').pop() : '');
        if (!childrenByParentId[pid]) childrenByParentId[pid] = [];
        childrenByParentId[pid].push(inst);
      });
    }
  });

  // If no parents resolved (unexpected), fall back to flat render
  if (parents.length === 0) {
    return principleInstances.map(function(inst) { return renderPrinciple(inst); }).join('');
  }

  var html = '';
  parents.forEach(function(parent) {
    var parentId = parent.id || (parent.uri ? parent.uri.split('#').pop() : '');
    var kids = childrenByParentId[parentId] || [];
    html += renderPrinciple(parent);
    if (kids.length > 0) {
      // Inject nested specializations inside the parent's open details
      // is harder — simpler: append a sibling block right after.
      html += '<div style="padding:0 0 8px 24px;border-left:2px solid #1e293b;margin-left:8px;">';
      html += '<details><summary style="cursor:pointer;padding:4px 0;color:#64748b;font-size:0.9em;">Specializations (' + kids.length + ')</summary>';
      html += '<div style="padding:4px 0 4px 12px;">';
      kids.forEach(function(child) { html += renderPrinciple(child); });
      html += '</div></details>';
      html += '</div>';
    }
  });
  return html;
}

function renderPolicy(inst) {
  var enriched = INSTANCE_ENRICHMENT[inst.uri] || {};
  var html = '<details class="instance-body"><summary style="cursor:pointer;padding:6px 0;color:#0369a1;font-weight:500;">' + escapeHtml(inst.label) + '</summary>';
  html += '<div style="padding:0 0 12px 16px;">';
  if (inst.comment) html += '<p>' + escapeHtml(inst.comment) + '</p>';
  if (enriched.surface) {
    html += '<p><span style="display:inline-block;padding:2px 8px;background:#1e293b;color:#93c5fd;border-radius:4px;font-size:0.85em;">' + escapeHtml(enriched.surface) + '</span></p>';
  }
  if (enriched.enforces && enriched.enforces.length > 0) {
    html += '<p style="font-size:0.9em;color:#64748b;">Enforces: ';
    html += enriched.enforces.map(function(p) { return escapeHtml(p.label || p.id || ''); }).join(', ');
    html += '</p>';
  }
  html += '</div></details>';
  return html;
}

function renderDecision(inst) {
  var enriched = INSTANCE_ENRICHMENT[inst.uri] || {};
  var html = '<details class="instance-body"><summary style="cursor:pointer;padding:6px 0;color:#0369a1;font-weight:500;">' + escapeHtml(inst.label) + '</summary>';
  html += '<div style="padding:0 0 12px 16px;">';
  if (inst.comment) html += '<p>' + escapeHtml(inst.comment) + '</p>';
  var meta = [];
  if (enriched.decisionDate) meta.push('<strong>Date:</strong> ' + escapeHtml(enriched.decisionDate));
  if (enriched.status) meta.push('<strong>Status:</strong> ' + escapeHtml(enriched.status));
  if (enriched.enforcementLevel) meta.push('<strong>Enforcement:</strong> ' + escapeHtml(enriched.enforcementLevel));
  if (meta.length > 0) html += '<p style="font-size:0.9em;color:#64748b;">' + meta.join(' · ') + '</p>';
  html += '</div></details>';
  return html;
}

// #2431 — load per-type enrichment when subdomain contains content-bearing
// instances. Called alongside the 22 parallel fetches already in flight.
function loadInstanceEnrichment(instances) {
  var types = new Set(instances.map(function(i) { return i.type; }));
  var fetches = [];
  // #2431 — loom APIs live on chorus-api (3340), not gathering (3000).
  // Gathering doesn't proxy /api/loom, so target absolute. Existing pattern
  // in this file: tracedFetch targets 3340 via X-Trace-Id fan-out. Simpler
  // for one-shot enrichment: direct absolute URL.
  var CHORUS_API_BASE = 'http://localhost:3340';
  if (types.has('Principle')) {
    fetches.push(fetch(CHORUS_API_BASE + '/api/loom/principles').then(function(r) { return r.ok ? r.json() : null; }).then(function(d) {
      if (!d || !d.data) return;
      (d.data.principles || []).forEach(function(p) { INSTANCE_ENRICHMENT[p.uri] = p; });
    }).catch(function() {}));
  }
  if (types.has('Policy')) {
    fetches.push(fetch(CHORUS_API_BASE + '/api/loom/policies').then(function(r) { return r.ok ? r.json() : null; }).then(function(d) {
      if (!d || !d.data) return;
      (d.data.policies || []).forEach(function(p) { INSTANCE_ENRICHMENT[p.uri] = p; });
    }).catch(function() {}));
  }
  if (types.has('Decision')) {
    fetches.push(fetch('/api/chorus/domain/' + encodeURIComponent(domainId) + '/decisions').then(function(r) { return r.ok ? r.json() : null; }).then(function(d) {
      if (!d) return;
      var list = (d.data && d.data.decisions) || d.decisions || [];
      list.forEach(function(dec) {
        var uri = dec.uri || ('https://jeffbridwell.com/chorus#' + (dec.id || ''));
        INSTANCE_ENRICHMENT[uri] = dec;
      });
    }).catch(function() {}));
  }
  return Promise.all(fetches);
}

const ownerClasses = { Jeff: 'jeff', Wren: 'wren', Silas: 'silas', Kade: 'kade' };
const stepClasses = { Shaping: 'step-shaping', Directing: 'step-directing', Designing: 'step-designing', Building: 'step-building', Proving: 'step-proving' };

const params = new URLSearchParams(window.location.search);
const domainId = params.get('id') || 'cards-service';

// Aggregation domains show ALL data across domains, not domain-scoped (#2098)
const AGGREGATION_DOMAINS = ['tests-domain', 'code-domain'];
const isAggregation = AGGREGATION_DOMAINS.includes(domainId);

// Render correlation ID for trace call stack (#2101)
const RENDER_ID = 'ui-' + Date.now() + '-' + Math.random().toString(36).slice(2, 8);
let hopCounter = 0;

function tracedFetch(url) {
  const hop = ++hopCounter;
  const start = Date.now();
  return fetch(url, { headers: { 'X-Trace-Id': RENDER_ID } }).then(function(res) {
    const ms = Date.now() - start;
    // Fire-and-forget trace hop to chorus API
    fetch('http://localhost:3340/api/chorus/trace', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        correlationId: RENDER_ID,
        hop: hop,
        callStack: 'ui:domain-detail',
        source: 'domain-detail',
        destination: url.replace(/^.*\/api\//, '/api/'),
        latencyMs: ms,
        error: res.ok ? null : 'HTTP ' + res.status
      })
    }).catch(function() {});
    return res;
  });
}


// --- Herald facet auto-wiring (#2104, #2485 round 2) ---
// One config, one renderer. Adding a new herald = one line here, zero UI code.
//
// Contract (Jeff's "tabs-must-render-empty-not-missing" rule, 2026-04-25):
// every entry here renders a section header + body unconditionally. Empty data
// shows the section + emptyMsg; never hidden. Non-table renders declare
// `customRender(items, ctx, raw)` returning the inner HTML for the <details> body.
// Facets whose data isn't a flat list (e.g., dependencies = direct + shared)
// declare `extract(data)` to compute the items count for the header.
const HERALD_FACETS = [
  { key: 'actors', title: 'Actors', endpoint: function(id) { return ATHENA + '/subdomains/' + id + '/actors'; }, listKey: 'actors',
    customRender: function(items, ctx) {
      var chart = 'graph LR\n  DOMAIN["' + (ctx.label || '') + '"]\n';
      items.forEach(function(a, i) {
        var safeLabel = (a.label || (a.role || '').split('#').pop() || 'unknown').replace(/[^a-zA-Z0-9 .]/g, '');
        var safeAction = (a.action || 'interacts').replace(/[^a-zA-Z0-9 ,./]/g, '').substring(0, 50);
        chart += '  A' + i + '["' + safeLabel + '"] -->|"' + safeAction + '"| DOMAIN\n';
      });
      return '<div class="mermaid" id="mermaid-actors-' + Date.now() + '">' + chart + '</div>';
    },
    emptyMsg: 'No actors for this domain', source: { kind: 'authored', from: 'POST /api/athena/subdomains/:id/actors' } },
  { key: 'scenarios', title: 'Scenarios', endpoint: function(id) { return ATHENA + '/subdomains/' + id + '/scenarios'; }, listKey: 'scenarios',
    customRender: function(items) {
      return items.map(function(s) {
        var body = '<div style="padding:4px 0 8px 16px;">';
        if (s.given) body += '<div><strong style="color:#16a34a;">Given</strong> ' + s.given + '</div>';
        if (s.when) body += '<div><strong style="color:#d97706;">When</strong> ' + s.when + '</div>';
        if (s.then) body += '<div><strong style="color:#2563eb;">Then</strong> ' + s.then + '</div>';
        if (s.notes) body += '<details style="margin-top:4px;"><summary style="cursor:pointer;font-size:0.85em;color:#666;">Implementation notes</summary><p style="padding:4px 0 0 8px;font-size:0.85em;color:#999;">' + s.notes + '</p></details>';
        body += '</div>';
        return '<details style="margin:4px 0;"><summary style="cursor:pointer;padding:6px 0;font-weight:500;color:#0369a1;">' + (s.label || s.title || 'Untitled') + '</summary>' + body + '</details>';
      }).join('');
    },
    emptyMsg: 'No scenarios for this domain', source: { kind: 'authored', from: 'POST /api/athena/subdomains/:id/scenarios' } },
  { key: 'dependencies', title: 'Dependencies', endpoint: function(id) { return DOMAIN_API + '/' + id + '/dependencies'; },
    extract: function(data) {
      var direct = (data && data.direct) || { consumes: [], consumedBy: [] };
      var shared = (data && data.shared) || [];
      var count = direct.consumes.length + direct.consumedBy.length + shared.length;
      return { count: count };
    },
    customRender: function(_items, _ctx, raw) {
      var direct = (raw && raw.direct) || { consumes: [], consumedBy: [] };
      var shared = (raw && raw.shared) || [];
      var html = '';
      if (direct.consumes.length > 0 || direct.consumedBy.length > 0) {
        html += '<h3 style="font-size:0.9em;color:#444;margin:8px 0 4px;">Direct</h3><div style="display:grid;grid-template-columns:1fr 1fr;gap:24px;">';
        html += '<div><strong style="font-size:0.8em;color:#666;">Depends On</strong>';
        if (direct.consumes.length > 0) direct.consumes.forEach(function(d) { html += '<div style="padding:3px 0;"><a href="domain-detail.html?id=' + d.id + '">' + d.label + '</a></div>'; });
        else html += '<p class="placeholder" style="margin:4px 0;">None</p>';
        html += '</div><div><strong style="font-size:0.8em;color:#666;">Consumed By</strong>';
        if (direct.consumedBy.length > 0) direct.consumedBy.forEach(function(c) { html += '<div style="padding:3px 0;"><a href="domain-detail.html?id=' + c.id + '">' + c.label + '</a></div>'; });
        else html += '<p class="placeholder" style="margin:4px 0;">None</p>';
        html += '</div></div>';
      }
      if (shared.length > 0) {
        html += '<h3 style="font-size:0.9em;color:#444;margin:12px 0 4px;">Shared Infrastructure</h3><table><tr><th>Domain</th><th>Shared Via</th></tr>';
        shared.forEach(function(s) { html += '<tr><td><a href="domain-detail.html?id=' + s.domain + '">' + s.label + '</a></td><td>' + (s.sharedVia || []).join(', ') + '</td></tr>'; });
        html += '</table>';
      }
      return html;
    },
    emptyMsg: 'No dependencies for this domain', source: { kind: 'derived', from: 'graph edges: chorus:dependsOn / consumedBy / sharedVia' } },
  { key: 'pages', title: 'UI Pages', endpoint: function(id) { return ATHENA + '/subdomains/' + id + '/pages'; }, listKey: 'pages', columns: ['route', 'path', 'pageType'], emptyMsg: 'No UI pages for this domain', source: { kind: 'derived', from: 'discover-pages scanner' } },
  { key: 'integrations', title: 'Integration', endpoint: function(id) { return ATHENA + '/subdomains/' + id + '/integrations'; }, listKey: 'integrations', columns: ['label', 'source', 'path', 'status'], emptyMsg: 'No integrations for this domain', source: { kind: 'hybrid', from: 'icd-instance TTL + integration scanner' } },
  { key: 'endpoints', title: 'Endpoints', endpoint: function(id) { return ATHENA + '/subdomains/' + id + '/services'; }, listKey: 'endpoints', altKey: 'services', columns: ['method', 'path', 'handler'], emptyMsg: 'No endpoints for this domain', source: { kind: 'derived', from: 'discover-endpoints scanner' } },
  { key: 'code', title: 'Code', endpoint: function(id) { return ATHENA + '/subdomains/' + id + '/code'; }, listKey: 'files', columns: ['path', 'type'], emptyMsg: 'No code for this domain', source: { kind: 'derived', from: 'discover-code scanner' } },
  { key: 'tests', title: 'Tests', endpoint: function(id) { return DOMAIN_API + '/' + id + '/tests'; }, listKey: 'tests', columns: ['path', 'type'], emptyMsg: 'No tests for this domain', source: { kind: 'derived', from: 'discover-tests scanner' } },
  { key: 'persistence', title: 'Persistence', endpoint: function(id) { return ATHENA + '/subdomains/' + id + '/persistence'; }, listKey: 'stores', altKey: 'persistence', columns: ['label', 'namespace', 'records', 'status'], emptyMsg: 'No persistence for this domain', source: { kind: 'derived', from: 'icd persistence section' } },
  { key: 'pipeline', title: 'Pipeline', endpoint: function(id) { return DOMAIN_API + '/' + id + '/pipeline'; }, listKey: 'stages', columns: ['name', 'status', 'evidence', 'summary'], emptyMsg: 'No pipeline for this domain', source: { kind: 'authored', from: 'pipeline manifest per domain' } },
  { key: 'releases', title: 'Release History', endpoint: function(id) { return DOMAIN_API + '/' + id + '/releases'; }, listKey: 'releases', columns: ['timestamp', 'cardId', 'title', 'role', 'commit'], emptyMsg: 'No releases for this domain', source: { kind: 'derived', from: 'git log / acp commits' } },
  { key: 'infra', title: 'Infrastructure', endpoint: function(id) { return DOMAIN_API + '/' + id + '/infra'; }, listKey: 'environments', columns: ['name', 'port', 'engine', 'host'], emptyMsg: 'No infrastructure for this domain', source: { kind: 'derived', from: 'infra config / launchd services' } },
  { key: 'priorArt', title: 'Prior Art', endpoint: function(id) { return ATHENA + '/subdomains/' + id + '/prior-art'; }, listKey: 'items', columns: ['label', 'path', 'description'], emptyMsg: 'No prior art for this domain', source: { kind: 'authored', from: 'POST /api/athena/subdomains/:id/prior-art' } },
  { key: 'decisions', title: 'Decisions', endpoint: function(id) { return DOMAIN_API + '/' + id + '/decisions'; }, listKey: 'decisions', columns: ['id', 'title', 'type', 'enforcement', 'date'], emptyMsg: 'No decisions for this domain', source: { kind: 'derived', from: 'DEC/ADR harvest filtered to this domain' } },
  { key: 'logs', title: 'Logs', endpoint: function(id) { return DOMAIN_API + '/' + id + '/logs'; }, listKey: 'logs', columns: ['label', 'location', 'retention', 'status'], emptyMsg: 'No logs for this domain', source: { kind: 'derived', from: 'log config / promtail jobs' } },
  { key: 'alerts', title: 'Alerts', endpoint: function(id) { return DOMAIN_API + '/' + id + '/alerts'; }, listKey: 'alerts', columns: ['name', 'description', 'severity'], emptyMsg: 'No alerts for this domain', source: { kind: 'derived', from: 'proving/domains/alerts/*.yml' } },
  { key: 'gaps', title: 'Gaps & Status', endpoint: function(id) { return ATHENA + '/subdomains/' + id + '/gaps'; }, listKey: 'gaps',
    customRender: function(items) {
      return items.map(function(g) {
        var cls = g.type === 'resolved' ? 'resolved' : 'gap';
        var prefix = g.type === 'resolved' ? 'RESOLVED' : 'GAP';
        return '<div class="' + cls + '"><strong>' + prefix + ':</strong> ' + (g.description || g.label || '') + (g.severity ? ' <em>(' + g.severity + ')</em>' : '') + '</div>';
      }).join('');
    },
    emptyMsg: 'No gaps or status items for this domain', source: { kind: 'derived', from: 'completeness API: missing-from-lifecycle' } },
];

function renderHeraldFacet(facetDef, raw, ctx) {
  // raw is the body.data for this facet's endpoint (or {} when fetch failed).
  // customRender gets (items, ctx, raw); column path uses items only.
  var data = raw || {};
  var items;
  var count;
  if (facetDef.extract) {
    var ex = facetDef.extract(data);
    items = [];
    count = (ex && typeof ex.count === 'number') ? ex.count : 0;
  } else {
    items = data[facetDef.listKey] || (facetDef.altKey ? data[facetDef.altKey] : null) || [];
    if (!Array.isArray(items)) items = [];
    count = items.length;
  }
  // #2502 — source-of-data label so empty=0 reads honestly: "discovery returned nothing"
  // vs "no one authored it" vs "blended". Tooltip on hover shows the exact discoverer.
  var srcLabel = '';
  if (facetDef.source) {
    var srcText = facetDef.source.kind || '';
    var srcTitle = facetDef.source.from ? ' title="from: ' + facetDef.source.from + '"' : '';
    srcLabel = '<span class="herald-src" data-source="' + srcText + '"' + srcTitle + ' style="margin-left:8px;font-size:0.7em;font-weight:400;color:#6b7280;text-transform:uppercase;letter-spacing:0.05em;">' + srcText + '</span>';
  }
  var html = '<details><summary style="cursor:pointer;font-size:1.2em;font-weight:600;padding:8px 0;">' + facetDef.title + (count ? ' (' + count + ')' : '') + srcLabel + '</summary>';
  if (count > 0) {
    if (facetDef.customRender) {
      html += facetDef.customRender(items, ctx || {}, data);
    } else {
      html += '<table><tr>';
      facetDef.columns.forEach(function(col) { html += '<th>' + col.charAt(0).toUpperCase() + col.slice(1) + '</th>'; });
      html += '</tr>';
      items.forEach(function(item) {
        html += '<tr>';
        facetDef.columns.forEach(function(col) {
          var val = item[col];
          if (val === undefined || val === null) val = '';
          if (col === 'route' && typeof val === 'string' && val.startsWith('/')) val = '<a href="' + val + '">' + val + '</a>';
          else if (col === 'path') val = '<code>' + val + '</code>';
          else if (col === 'commit' && val) val = '<code>' + val + '</code>';
          else if (col === 'cardId' && val) val = '#' + val;
          else if (col === 'timestamp' && typeof val === 'string') val = val.slice(0, 10);
          html += '<td>' + val + '</td>';
        });
        html += '</tr>';
      });
      html += '</table>';
    }
  } else {
    html += '<p class="placeholder">' + facetDef.emptyMsg + '</p>';
  }
  html += '</details>';
  return html;
}

async function init() {
  try {
    const res = await fetch(ATHENA + '/subdomains/' + domainId);
    if (res.status === 404) {
      document.getElementById('content-sections').innerHTML = '<div class="gap"><strong>ERROR:</strong> Domain "' + domainId + '" not found. <a href="value-stream.html">Browse all domains</a></div>';
      return;
    }
    if (!res.ok) throw new Error('Athena ' + res.status);
    const body = await res.json();
    const d = body.data;

    const [brRes, cardsRes, codeRes, testsRes, alertsRes, compRes, actorsRes, scenariosRes, contractRes, priorArtRes, pagesRes, integrationsRes, persistenceRes, servicesRes, pipelineRes, logsRes, gapsRes, docsRes, infraRes, depsRes, relRes, decRes] = await Promise.all([
      tracedFetch(ATHENA + '/subdomains/' + domainId + '/blast-radius'),
      tracedFetch(ATHENA + '/subdomains/' + domainId + '/cards'),
      isAggregation && domainId === 'code-domain' ? tracedFetch('/api/chorus/domain/chorus-domain/code') : tracedFetch(DOMAIN_API + '/' + domainId + '/code'),
      isAggregation && domainId === 'tests-domain' ? tracedFetch('/api/quality/scan') : tracedFetch(DOMAIN_API + '/' + domainId + '/tests'),
      tracedFetch(DOMAIN_API + '/' + domainId + '/alerts'),
      tracedFetch(ATHENA + '/subdomains/' + domainId + '/completeness'),
      tracedFetch(ATHENA + '/subdomains/' + domainId + '/actors'),
      tracedFetch(ATHENA + '/subdomains/' + domainId + '/scenarios'),
      tracedFetch(ATHENA + '/subdomains/' + domainId + '/contract'),
      tracedFetch(ATHENA + '/subdomains/' + domainId + '/prior-art'),
      tracedFetch(ATHENA + '/subdomains/' + domainId + '/pages'),
      tracedFetch(ATHENA + '/subdomains/' + domainId + '/integrations'),
      tracedFetch(ATHENA + '/subdomains/' + domainId + '/persistence'),
      tracedFetch(DOMAIN_API + '/' + domainId + '/services'),
      tracedFetch(DOMAIN_API + '/' + domainId + '/pipeline'),
      tracedFetch(DOMAIN_API + '/' + domainId + '/logs'),
      tracedFetch(ATHENA + '/subdomains/' + domainId + '/gaps'),
      tracedFetch(DOMAIN_API + '/' + domainId + '/docs'),
      tracedFetch(DOMAIN_API + '/' + domainId + '/infra'),
      tracedFetch(DOMAIN_API + '/' + domainId + '/dependencies'),
      tracedFetch(DOMAIN_API + '/' + domainId + '/releases'),
      tracedFetch(DOMAIN_API + '/' + domainId + '/decisions'),
    ]);
    const brBody = brRes.ok ? await brRes.json() : { data: { consumers: [] } };
    const cardsBody = cardsRes.ok ? await cardsRes.json() : { data: [] };
    const codeBody = codeRes.ok ? await codeRes.json() : { data: { files: [] } };
    const testsRaw = testsRes.ok ? await testsRes.json() : { data: { tests: [] } };
    // For aggregation domains, /api/quality/scan returns {pyramid[], total} directly (#2098)
    var testsBody;
    if (isAggregation && domainId === 'tests-domain' && testsRaw.pyramid) {
      var allTestFiles = testsRaw.pyramid.flatMap(function(l) { return (l.files || []).map(function(f) { return { path: f.name, type: f.kind, layer: l.name }; }); });
      testsBody = { data: { tests: allTestFiles, total: testsRaw.total } };
    } else {
      testsBody = testsRaw;
    }
    // Merge code + tests into shape renderDomain expects: { files: [], tests: [] }
    const mergedCode = { files: codeBody.data.files || [], tests: testsBody.data.tests || [], byType: codeBody.data.byType || {} };
    const alertsBody = alertsRes.ok ? await alertsRes.json() : { data: [] };
    const compBody = compRes.ok ? await compRes.json() : { data: null };
    const actorsBody = actorsRes.ok ? await actorsRes.json() : { data: { actors: [] } };
    const scenariosBody = scenariosRes.ok ? await scenariosRes.json() : { data: { scenarios: [] } };
    const contractBody = contractRes.ok ? await contractRes.json() : { data: { endpoints: [] } };
    const priorArtBody = priorArtRes.ok ? await priorArtRes.json() : { data: { items: [] } };
    const pagesBody = pagesRes.ok ? await pagesRes.json() : { data: { pages: [] } };
    const integrationsBody = integrationsRes.ok ? await integrationsRes.json() : { data: { integrations: [] } };
    const persistenceBody = persistenceRes.ok ? await persistenceRes.json() : { data: { stores: [] } };
    const servicesBody = servicesRes.ok ? await servicesRes.json() : { data: { services: [] } };
    const pipelineBody = pipelineRes.ok ? await pipelineRes.json() : { data: { pipelines: [] } };
    const logsBody = logsRes.ok ? await logsRes.json() : { data: { logs: [] } };
    const gapsBody = gapsRes.ok ? await gapsRes.json() : { data: { gaps: [] } };
    const docsBody = docsRes.ok ? await docsRes.json() : { governs: [] };
    const infraBody = infraRes.ok ? await infraRes.json() : { data: { environments: [] } };
    const depsBody = depsRes.ok ? await depsRes.json() : { data: { direct: { consumes: [], consumedBy: [] }, shared: [] } };
    const relBody = relRes.ok ? await relRes.json() : { data: { releases: [] } };
    const decBody = decRes.ok ? await decRes.json() : { data: { decisions: [] } };

    const cardsList = cardsBody.data?.cards || cardsBody.data || [];
    // #2431 — enrich instances with type-specific fields before render so
    // Principle/Policy/Decision renderers can read techReading, enforces,
    // decisionDate etc. Safe on subdomains with no instances (resolves ∅).
    await loadInstanceEnrichment(d.instances || []);
    renderDomain(d, brBody.data.consumers, cardsList, mergedCode, alertsBody.data, compBody.data, actorsBody.data, scenariosBody.data, contractBody.data, priorArtBody.data, pagesBody.data, integrationsBody.data, persistenceBody.data, servicesBody.data, pipelineBody.data, logsBody.data, gapsBody.data, docsBody, infraBody.data, depsBody.data, relBody.data, decBody.data);
  } catch (err) {
    document.getElementById('content-sections').innerHTML = '<div class="gap"><strong>ERROR:</strong> Could not load from Athena: ' + err.message + '</div>';
  }
}

function renderDomain(d, blastConsumers, cards, codeFiles, alerts, completeness, actorsData, scenariosData, contractData, priorArtData, pagesData, integrationsData, persistenceData, servicesData, pipelineData, logsData, gapsData, docsData, infraData, depsData, relData, decData) {
  document.title = d.label + ' — Athena';
  document.getElementById('domain-title').innerHTML = d.label + ' <span>— Domain</span>'; document.getElementById('domain-title').style.visibility = 'visible'; document.getElementById('stats-bar').style.visibility = 'visible';

  // Breadcrumb
  document.getElementById('bc-step').textContent = d.step || 'Unknown';
  document.getElementById('bc-step').href = 'step-detail.html?step=' + (d.step || '');
  document.getElementById('bc-domain').textContent = d.label;

  const oc = ownerClasses[d.owner] || '';
  const sc = stepClasses[d.step] || '';

  // Stats bar
  document.getElementById('stats-bar').innerHTML =
    '<div class="stat"><div class="stat-value owner"><span class="owner-dot ' + oc + '"></span> ' + (d.owner || '?') + '</div><div class="stat-label">Owner</div></div>' +
    '<div class="stat"><div class="stat-value step"><span class="step-badge ' + sc + '">' + (d.step || '?') + '</span></div><div class="stat-label">Primary Step</div></div>' +
    '<div class="stat"><div class="stat-value">' + d.consumedBy.length + '</div><div class="stat-label">Consumers</div></div>' +
    '<div class="stat"><div class="stat-value">' + (cards ? cards.length : 0) + '</div><div class="stat-label">Active Cards</div></div>';

  // Promise block (from description/comment)
  if (d.comment) {
    document.getElementById('promise-block').innerHTML = '<div class="promise">' + d.comment + '</div>';
  }

  // Completeness
  if (completeness) {
    const pct = completeness.percentage || 0;
    const barColor = pct >= 80 ? '#16a34a' : pct >= 50 ? '#d97706' : '#dc2626';
    let compHtml = '<h2>Completeness — ' + pct + '%</h2>';
    compHtml += '<div style="background:#e5e7eb;border-radius:6px;height:10px;margin:8px 0 12px;overflow:hidden;">';
    compHtml += '<div style="background:' + barColor + ';height:100%;width:' + pct + '%;border-radius:6px;transition:width 0.3s;"></div></div>';
    compHtml += '<div>';
    (completeness.present || []).forEach(function(s) { compHtml += '<span class="chip-present">' + s + '</span>'; });
    (completeness.missing || []).forEach(function(s) { compHtml += '<span class="chip-missing">' + s + '</span>'; });
    compHtml += '</div>';
    document.getElementById('completeness-block').innerHTML = compHtml;
  }

  // Build all content sections
  let html = '';

  // #2485 round 2 — Herald-driven rendering. dataMap is keyed by HERALD_FACETS.key;
  // each entry maps to the body.data envelope from its endpoint. Empty → renders
  // section header + emptyMsg per Jeff's "tabs-must-render-empty-not-missing" rule.
  var heraldCtx = { label: d.label, domainId: d.id || (d.uri ? d.uri.split('#').pop() : '') };
  var dataMap = {
    actors: actorsData || { actors: [] },
    scenarios: scenariosData || { scenarios: [] },
    dependencies: depsData || { direct: { consumes: [], consumedBy: [] }, shared: [] },
    pages: pagesData || { pages: [] },
    integrations: integrationsData || { integrations: [] },
    endpoints: { endpoints: (servicesData && (servicesData.endpoints || servicesData.services)) || (contractData && contractData.endpoints) || [] },
    code: { files: (codeFiles && codeFiles.files) || [] },
    tests: { tests: (codeFiles && codeFiles.tests) || [] },
    persistence: persistenceData || { stores: [] },
    pipeline: pipelineData || { stages: [] },
    releases: relData || { releases: [] },
    infra: infraData || { environments: [] },
    priorArt: { items: ((priorArtData && priorArtData.items) || []).concat(((docsData && docsData.governs) || []).map(function(doc) {
      return { label: doc.title || '', path: doc.href || '', description: doc.type || '' };
    })) },
    decisions: decData || { decisions: [] },
    logs: logsData || { logs: [] },
    alerts: { alerts: Array.isArray(alerts) ? alerts : ((alerts && alerts.alerts) || []) },
    gaps: gapsData || { gaps: [] },
  };

  // First wave — render facets that come before the structural sections
  // (Child Domains, Instances, Cards). Order matches original page layout.
  ['actors', 'scenarios', 'dependencies', 'pages', 'integrations', 'endpoints', 'code', 'tests', 'persistence', 'pipeline', 'releases', 'infra', 'priorArt', 'decisions'].forEach(function(key) {
    var facet = HERALD_FACETS.find(function(f) { return f.key === key; });
    if (facet) html += renderHeraldFacet(facet, dataMap[key], heraldCtx);
  });

  // --- ACTORS_REMOVED — replaced by herald loop above ---
  if (false) {
  const actors = actorsData ? actorsData.actors || [] : [];
  html += '<details><summary style=\"cursor:pointer;font-size:1.2em;font-weight:600;padding:8px 0;\">Actors' + (actors.length ? ' (' + actors.length + ')' : '') + '</summary>';
  if (actors.length > 0) {
    const mermaidId = 'mermaid-actors-' + Date.now();
    let chart = 'graph LR\n';
    chart += '  DOMAIN["' + d.label + '"]\n';
    actors.forEach(function(a, i) {
      const roleLabel = (a.role || '').split('#').pop() || 'unknown';
      var safeLabel = (a.label || roleLabel).replace(/[^a-zA-Z0-9 .]/g, '');
      var safeAction = (a.action || 'interacts').replace(/[^a-zA-Z0-9 ,./]/g, '').substring(0, 50);
      chart += '  A' + i + '["' + safeLabel + '"] -->|"' + safeAction + '"| DOMAIN\n';
    });
    html += '<div class="mermaid" id="' + mermaidId + '">' + chart + '</div>';
  } else {
    html += '<p class="placeholder">No actors defined — <span class="tag-mock">API GAP</span> POST /api/athena/subdomains/:id/actors</p>';
  }

  // --- BDD SCENARIOS ---
  const scenarios = scenariosData ? scenariosData.scenarios || [] : [];

  html += '</details>';

  html += '<details><summary style=\"cursor:pointer;font-size:1.2em;font-weight:600;padding:8px 0;\">Scenarios' + (scenarios.length ? ' (' + scenarios.length + ')' : '') + '</summary>';
  if (scenarios.length > 0) {
    scenarios.forEach(function(s) {
      html += '<details style="margin:4px 0;"><summary style="cursor:pointer;padding:6px 0;font-weight:500;color:#0369a1;">' + (s.label || s.title || 'Untitled') + '</summary>';
      html += '<div style="padding:4px 0 8px 16px;">';
      if (s.given) html += '<div><strong style="color:#16a34a;">Given</strong> ' + s.given + '</div>';
      if (s.when) html += '<div><strong style="color:#d97706;">When</strong> ' + s.when + '</div>';
      if (s.then) html += '<div><strong style="color:#2563eb;">Then</strong> ' + s.then + '</div>';
      if (s.notes) html += '<details style="margin-top:4px;"><summary style="cursor:pointer;font-size:0.85em;color:#666;">Implementation notes</summary><p style="padding:4px 0 0 8px;font-size:0.85em;color:#999;">' + s.notes + '</p></details>';
      html += '</div></details>';
    });
  } else {
    html += '<p class="placeholder">No BDD scenarios defined — <span class="tag-mock">API GAP</span> POST /api/athena/subdomains/:id/scenarios</p>';
  }

  // --- DEPENDENCIES (#2082 — two layers: direct + shared infra) ---
  var directDeps = depsData ? depsData.direct || { consumes: [], consumedBy: [] } : { consumes: [], consumedBy: [] };
  var sharedDeps = depsData ? depsData.shared || [] : [];
  var totalDeps = directDeps.consumes.length + directDeps.consumedBy.length + sharedDeps.length;

  html += '</details>';

  html += '<details><summary style="cursor:pointer;font-size:1.2em;font-weight:600;padding:8px 0;">Dependencies' + (totalDeps ? ' (' + totalDeps + ')' : '') + '</summary>';

  // Direct edges
  if (directDeps.consumes.length > 0 || directDeps.consumedBy.length > 0) {
    html += '<h3 style="font-size:0.9em;color:#444;margin:8px 0 4px;">Direct</h3>';
    html += '<div style="display:grid;grid-template-columns:1fr 1fr;gap:24px;">';
    html += '<div><strong style="font-size:0.8em;color:#666;">Depends On</strong>';
    if (directDeps.consumes.length > 0) {
      directDeps.consumes.forEach(function(dep) {
        html += '<div style="padding:3px 0;"><a href="domain-detail.html?id=' + dep.id + '">' + dep.label + '</a></div>';
      });
    } else { html += '<p class="placeholder" style="margin:4px 0;">None</p>'; }
    html += '</div>';
    html += '<div><strong style="font-size:0.8em;color:#666;">Consumed By</strong>';
    if (directDeps.consumedBy.length > 0) {
      directDeps.consumedBy.forEach(function(c) {
        html += '<div style="padding:3px 0;"><a href="domain-detail.html?id=' + c.id + '">' + c.label + '</a></div>';
      });
    } else { html += '<p class="placeholder" style="margin:4px 0;">None</p>'; }
    html += '</div></div>';
  }

  // Shared infrastructure
  if (sharedDeps.length > 0) {
    html += '<h3 style="font-size:0.9em;color:#444;margin:12px 0 4px;">Shared Infrastructure</h3>';
    html += '<table><tr><th>Domain</th><th>Shared Via</th></tr>';
    sharedDeps.forEach(function(s) {
      html += '<tr><td><a href="domain-detail.html?id=' + s.domain + '">' + s.label + '</a></td><td>' + s.sharedVia.join(', ') + '</td></tr>';
    });
    html += '</table>';
  }

  if (totalDeps === 0) {
    html += '<p class="placeholder">No dependencies detected</p>';
  }

  html += '</details>';

  // --- UI PAGES ---
  const pages = pagesData ? pagesData.pages || [] : [];

  html += '<details><summary style="cursor:pointer;font-size:1.2em;font-weight:600;padding:8px 0;">UI Pages' + (pages.length ? ' (' + pages.length + ')' : '') + '</summary>';
  if (pages.length > 0) {
    // Type summary badges
    var pageTypes = {};
    pages.forEach(function(p) { var t = p.pageType || 'page'; pageTypes[t] = (pageTypes[t] || 0) + 1; });
    var typeColors = { collection: '#d1fae5;color:#065f46', detail: '#dbeafe;color:#1e40af', admin: '#fef3c7;color:#92400e', ontology: '#ede9fe;color:#5b21b6', doc: '#f3f4f6;color:#374151', 'service-design': '#fce7f3;color:#9d174d' };
    html += '<div style="display:flex;gap:12px;margin-bottom:16px;">';
    for (var pt in pageTypes) {
      var col = typeColors[pt] || '#f3f4f6;color:#374151';
      html += '<span style="background:' + col + ';padding:4px 12px;border-radius:6px;font-size:13px;">' + pt + ': ' + pageTypes[pt] + '</span>';
    }
    html += '</div>';
    html += '<table><tr><th>Route</th><th>File</th><th>Type</th></tr>';
    pages.forEach(function(p) {
      var route = p.route || '';
      var routeHtml = route.startsWith('/') ? '<a href="' + route + '">' + route + '</a>' : '<code>' + route + '</code>';
      var col = typeColors[p.pageType] || '#f3f4f6;color:#374151';
      html += '<tr><td>' + routeHtml + '</td><td><code>' + (p.path || '') + '</code></td><td><span style="background:' + col + ';padding:1px 6px;border-radius:3px;font-size:11px;">' + (p.pageType || 'page') + '</span></td></tr>';
    });
    html += '</table>';
  } else {
    html += '<p class="placeholder">No UI pages discovered — run POST /api/athena/discover-pages</p>';
  }

  // --- INTEGRATION ---
  const integrations = integrationsData ? integrationsData.integrations || [] : [];

  html += '</details>';

  html += '<details><summary style=\"cursor:pointer;font-size:1.2em;font-weight:600;padding:8px 0;\">Integration' + (integrations.length ? ' (' + integrations.length + ')' : '') + '</summary>';
  if (integrations.length > 0) {
    html += '<table><tr><th>Pipeline</th><th>Source</th><th>Path</th><th>Status</th></tr>';
    integrations.forEach(function(ig) {
      var tag = ig.status === 'real' ? 'tag-real' : ig.status === 'partial' ? 'tag-partial' : 'tag-design';
      html += '<tr><td>' + ig.label + '</td><td>' + (ig.source || '') + '</td><td>' + (ig.path || '') + '</td><td><span class="' + tag + '">' + (ig.status || 'design').toUpperCase() + '</span></td></tr>';
    });
    html += '</table>';
  } else {
    html += '<p class="placeholder">No integrations defined — trace data loads below</p>';
  }
  // Enrich integration section from trace data (#2101)
  html += '<div id="trace-integrations" style="margin-top:8px"></div>';

  // --- SERVICES ---
  const apiEndpoints = servicesData ? servicesData.endpoints || servicesData.services || [] : [];

  html += '</details>';

  html += '<details><summary style="cursor:pointer;font-size:1.2em;font-weight:600;padding:8px 0;">API Contract' + (apiEndpoints.length ? ' (' + apiEndpoints.length + ' endpoints)' : '') + '</summary>';
  if (apiEndpoints.length > 0) {
    // Method summary badges
    var methodCounts = {};
    apiEndpoints.forEach(function(ep) { methodCounts[ep.method] = (methodCounts[ep.method] || 0) + 1; });
    var methodColors = { GET: '#d1fae5;color:#065f46', POST: '#dbeafe;color:#1e40af', PUT: '#fef3c7;color:#92400e', DELETE: '#fee2e2;color:#991b1b', PATCH: '#ede9fe;color:#5b21b6' };
    html += '<div style="display:flex;gap:12px;margin-bottom:16px;">';
    for (var m in methodCounts) {
      var col = methodColors[m] || '#f3f4f6;color:#374151';
      html += '<span style="background:' + col + ';padding:4px 12px;border-radius:6px;font-size:13px;">' + m + ': ' + methodCounts[m] + '</span>';
    }
    html += '</div>';
    html += '<table><tr><th>Method</th><th>Route</th><th>Handler</th></tr>';
    apiEndpoints.forEach(function(ep) {
      var col = methodColors[ep.method] || '#f3f4f6;color:#374151';
      html += '<tr><td><span style="background:' + col + ';padding:1px 8px;border-radius:3px;font-size:11px;font-weight:600;">' + ep.method + '</span></td><td><code>' + ep.path + '</code></td><td><code>' + (ep.handler || '') + '</code></td></tr>';
    });
    html += '</table>';
  } else {
    html += '<p class="placeholder">No API endpoints discovered — run POST /api/athena/discover-endpoints</p>';
  }

  // --- CODE + TESTS (#1932, #2054) ---
  var sourceFiles = Array.isArray(codeFiles) ? codeFiles : (codeFiles ? codeFiles.files || [] : []);
  var testFiles = Array.isArray(codeFiles) ? [] : (codeFiles ? codeFiles.tests || [] : []);

  html += '</details>';

  html += '<details><summary style="cursor:pointer;font-size:1.2em;font-weight:600;padding:8px 0;">Code (' + sourceFiles.length + ')</summary>';
  if (sourceFiles.length > 0) {
    html += '<table><tr><th>File</th><th>Type</th></tr>';
    sourceFiles.forEach(function(f) {
      html += '<tr><td><code>' + (f.path || f.file || '') + '</code></td><td>' + (f.type || '') + '</td></tr>';
    });
    html += '</table>';
  } else {
    html += '<p class="placeholder">No source files mapped</p>';
  }

  html += '</details>';

  if (testFiles.length > 0) {
    // Classify tests by pyramid level from path convention (#2054)
    var classifyTest = function(p) {
      if (/\/e2e\//i.test(p) || /\.e2e\./i.test(p)) return 'e2e';
      if (/\/integration\//i.test(p)) return 'integration';
      if (/\.bats$/i.test(p) || /\.feature$/i.test(p)) return 'bdd';
      return 'unit';
    };
    var pyramid = { unit: 0, integration: 0, e2e: 0, bdd: 0 };
    testFiles.forEach(function(f) { pyramid[classifyTest(f.path || f.file || '')] += 1; });

    html += '<details><summary style="cursor:pointer;font-size:1.2em;font-weight:600;padding:8px 0;">Tests (' + testFiles.length + ')</summary>';

    // Pyramid summary
    html += '<div style="display:flex;gap:16px;margin-bottom:16px;">';
    if (pyramid.unit) html += '<span style="background:#d1fae5;color:#065f46;padding:4px 12px;border-radius:6px;font-size:13px;">Unit: ' + pyramid.unit + '</span>';
    if (pyramid.integration) html += '<span style="background:#dbeafe;color:#1e40af;padding:4px 12px;border-radius:6px;font-size:13px;">Integration: ' + pyramid.integration + '</span>';
    if (pyramid.e2e) html += '<span style="background:#fef3c7;color:#92400e;padding:4px 12px;border-radius:6px;font-size:13px;">E2E: ' + pyramid.e2e + '</span>';
    if (pyramid.bdd) html += '<span style="background:#ede9fe;color:#5b21b6;padding:4px 12px;border-radius:6px;font-size:13px;">BDD: ' + pyramid.bdd + '</span>';
    html += '</div>';

    html += '<table><tr><th>Test File</th><th>Level</th></tr>';
    testFiles.forEach(function(f) {
      var p = f.path || f.file || '';
      html += '<tr><td><code>' + p + '</code></td><td>' + classifyTest(p) + '</td></tr>';
    });
    html += '</table>';
    html += '</details>';
  }

  // --- PERSISTENCE ---
  const stores = persistenceData ? persistenceData.stores || persistenceData.persistence || [] : [];

  html += '</details>';

  html += '<details><summary style=\"cursor:pointer;font-size:1.2em;font-weight:600;padding:8px 0;\">Persistence' + (stores.length ? ' (' + stores.length + ')' : '') + '</summary>';
  if (stores.length > 0) {
    html += '<table><tr><th>Store</th><th>Namespace</th><th>Records</th><th>Status</th></tr>';
    stores.forEach(function(st) {
      var tag = st.status === 'real' ? 'tag-real' : st.status === 'partial' ? 'tag-partial' : 'tag-design';
      html += '<tr><td>' + st.label + '</td><td><code>' + (st.namespace || '') + '</code></td><td>' + (st.records || '') + '</td><td><span class="' + tag + '">' + (st.status || 'design').toUpperCase() + '</span></td></tr>';
    });
    html += '</table>';
  } else {
    html += '<p class="placeholder">No persistence stores defined</p>';
  }

  // --- PIPELINE (value stream lifecycle #2069) ---
  const stages = pipelineData ? pipelineData.stages || [] : [];

  html += '</details>';

  html += '<details><summary style="cursor:pointer;font-size:1.2em;font-weight:600;padding:8px 0;">Pipeline' + (stages.length ? ' — Value Stream' : '') + '</summary>';
  if (stages.length > 0) {
    var stageColors = { complete: '#16a34a', in_progress: '#d97706', not_started: '#9ca3af' };
    var stageLabels = { shape: 'Shape', design: 'Design', build: 'Build', prove: 'Prove', ship: 'Ship' };
    html += '<div style="display:flex;gap:4px;align-items:center;margin:12px 0 16px;">';
    stages.forEach(function(s, i) {
      var color = stageColors[s.status] || '#9ca3af';
      var bgColor = s.status === 'complete' ? color : s.status === 'in_progress' ? '#fef3c7' : '#f3f4f6';
      var textColor = s.status === 'complete' ? '#fff' : s.status === 'in_progress' ? '#92400e' : '#6b7280';
      html += '<div style="flex:1;text-align:center;padding:10px 6px;border-radius:6px;background:' + bgColor + ';border:2px solid ' + color + ';">';
      html += '<div style="font-weight:700;font-size:0.85em;color:' + textColor + ';">' + (stageLabels[s.name] || s.name) + '</div>';
      html += '<div style="font-size:1.4em;font-weight:800;color:' + textColor + ';">' + s.evidence + '</div>';
      html += '<div style="font-size:0.72em;color:' + textColor + ';opacity:0.8;">' + s.status.replace('_', ' ') + '</div>';
      html += '</div>';
      if (i < stages.length - 1) html += '<div style="color:#d1d5db;font-size:1.2em;">→</div>';
    });
    html += '</div>';
    // Detail table
    html += '<table><tr><th>Stage</th><th>Status</th><th>Evidence</th><th>Detail</th></tr>';
    stages.forEach(function(s) {
      var badge = s.status === 'complete' ? '<span style="color:#16a34a;font-weight:600;">●</span>' : s.status === 'in_progress' ? '<span style="color:#d97706;font-weight:600;">●</span>' : '<span style="color:#9ca3af;">○</span>';
      html += '<tr><td><strong>' + (stageLabels[s.name] || s.name) + '</strong></td><td>' + badge + ' ' + s.status.replace('_', ' ') + '</td><td>' + s.evidence + '</td><td>' + s.summary + '</td></tr>';
    });
    html += '</table>';
  } else {
    html += '<p class="placeholder">No pipeline data</p>';
  }

  // --- RELEASE HISTORY (#1910) ---
  var releases = relData ? relData.releases || [] : [];

  html += '</details>';

  html += '<details><summary style="cursor:pointer;font-size:1.2em;font-weight:600;padding:8px 0;">Release History' + (releases.length ? ' (' + releases.length + ')' : '') + '</summary>';
  if (releases.length > 0) {
    html += '<table><tr><th>Date</th><th>Card</th><th>What Shipped</th><th>Role</th><th>Commit</th></tr>';
    releases.forEach(function(r) {
      var date = r.timestamp ? r.timestamp.slice(0, 10) : '';
      html += '<tr><td>' + date + '</td><td>#' + r.cardId + '</td><td>' + r.title + '</td><td>' + r.role + '</td><td><code>' + r.commit + '</code></td></tr>';
    });
    html += '</table>';
  } else {
    html += '<p class="placeholder">No releases for this domain</p>';
  }

  // --- INFRASTRUCTURE (#2080) ---
  var envs = infraData ? infraData.environments || [] : [];

  html += '</details>';

  html += '<details><summary style="cursor:pointer;font-size:1.2em;font-weight:600;padding:8px 0;">Infrastructure' + (envs.length ? ' (' + envs.length + ' environments)' : '') + '</summary>';
  if (envs.length > 0) {
    html += '<table><tr><th>Service</th><th>Port</th><th>Engine</th><th>Host</th><th>Depends On</th><th>Health</th></tr>';
    envs.forEach(function(e) {
      var deps = (e.dependsOn && e.dependsOn.length > 0) ? e.dependsOn.join(', ') : '—';
      var healthLink = e.health ? '<a href="' + e.health + '" target="_blank" style="font-size:0.8em;">check</a>' : '—';
      html += '<tr><td><strong>' + e.name + '</strong></td><td>' + (e.port || '—') + '</td><td>' + (e.engine || '—') + '</td><td>' + (e.host || '—') + '</td><td>' + deps + '</td><td>' + healthLink + '</td></tr>';
    });
    html += '</table>';
  } else {
    html += '<p class="placeholder">No infrastructure data</p>';
  }

  // --- PRIOR ART (merged: doc-catalog #2078 + ontology prior-art) ---
  const docs = docsData ? docsData.governs || [] : [];
  const priorArtItems = priorArtData ? priorArtData.items || [] : [];
  const totalPriorArt = docs.length + priorArtItems.length;

  html += '</details>';

  html += '<details><summary style="cursor:pointer;font-size:1.2em;font-weight:600;padding:8px 0;">Prior Art' + (totalPriorArt ? ' (' + totalPriorArt + ')' : '') + '</summary>';
  if (docs.length > 0) {
    html += '<table><tr><th>Title</th><th>Type</th><th>Owner</th><th>Step</th></tr>';
    docs.forEach(function(doc) {
      var owners = (doc.tags && doc.tags.owner) ? doc.tags.owner.join(', ') : '';
      var step = (doc.tags && doc.tags.valueStreamStep) ? doc.tags.valueStreamStep.join(', ') : '';
      html += '<tr><td><a href="' + doc.href + '" target="_blank">' + doc.title + '</a></td><td>' + (doc.type || '') + '</td><td>' + owners + '</td><td>' + step + '</td></tr>';
    });
    html += '</table>';
  }
  if (priorArtItems.length > 0) {
    html += '<table><tr><th>Artifact</th><th>Path</th><th>Description</th></tr>';
    priorArtItems.forEach(function(item) {
      html += '<tr><td>' + item.label + '</td><td><code>' + (item.path || '') + '</code></td><td>' + (item.description || '') + '</td></tr>';
    });
    html += '</table>';
  }
  if (totalPriorArt === 0) {
    html += '<p class="placeholder">No prior art tagged for this domain</p>';
  }

  // --- DECISIONS (#2040) ---
  var decisions = decData ? decData.decisions || [] : [];
  // Sort: HARD-Rust first, NONE last
  var enfOrder = { 'HARD-Rust': 0, 'HARD-Shell': 1, 'SOFT': 2, 'NONE': 3 };
  decisions.sort(function(a, b) {
    var ea = enfOrder[a.enforcement] !== undefined ? enfOrder[a.enforcement] : 9;
    var eb = enfOrder[b.enforcement] !== undefined ? enfOrder[b.enforcement] : 9;
    return ea - eb || (a.id || '').localeCompare(b.id || '');
  });

  html += '</details>';  // close previous section

  html += '<details><summary style="cursor:pointer;font-size:1.2em;font-weight:600;padding:8px 0;">Decisions' + (decisions.length ? ' (' + decisions.length + ')' : '') + '</summary>';
  if (decisions.length > 0) {
    var enfColors = { 'HARD-Rust': '#dc2626', 'HARD-Shell': '#ea580c', 'SOFT': '#d97706', 'NONE': '#9ca3af' };
    html += '<table><tr><th>ID</th><th>Title</th><th>Type</th><th>Enforcement</th><th>Date</th></tr>';
    decisions.forEach(function(dec) {
      var enfColor = enfColors[dec.enforcement] || '#9ca3af';
      var badge = '<span style="background:' + enfColor + ';color:#fff;padding:1px 6px;border-radius:3px;font-size:0.75em;">' + (dec.enforcement || 'NONE') + '</span>';
      html += '<tr><td><strong>' + (dec.id || '') + '</strong></td><td>' + (dec.title || '') + '</td><td>' + (dec.type || '') + '</td><td>' + badge + '</td><td>' + (dec.date || '') + '</td></tr>';
    });
    html += '</table>';
  } else {
    html += '<p class="placeholder">No decisions for this domain</p>';
  }
  html += '</details>';  // #2431 — close Decisions unconditionally (was inside d.domains branch; broke on domains with no children)
  } // close if (false) — old inline blocks superseded by herald loop above (#2485 round 2)

  // --- CHILD DOMAINS ---
  if (d.domains && d.domains.length > 0) {
    html += '<details><summary style="cursor:pointer;font-size:1.2em;font-weight:600;padding:8px 0;">Child Domains (' + d.domains.length + ')</summary>';
    d.domains.forEach(function(child) {
      const childId = child.uri ? child.uri.split('#').pop() : '';
      html += '<details class="child-domain" data-child-id="' + childId + '">';
      html += '<summary style="cursor:pointer;padding:6px 0;color:#0369a1;font-weight:500;">' + child.label + ' <a href="domain-detail.html?id=' + childId + '" style="font-size:0.8em;color:#999;margin-left:8px;" onclick="event.stopPropagation();">→ full page</a></summary>';
      html += '<div class="child-detail" style="padding:0 0 12px 16px;"><p class="placeholder">Loading...</p></div>';
      html += '</details>';  // #2431 — close child-domain per iteration
    });
    html += '</details>';  // #2431 — close outer Child Domains
  }

  // --- INSTANCES (#2431 — type-dispatch renderer) ---
  // Dispatch map; renderers defined below. Generic fallback is the default;
  // types without a custom renderer (Practice, RCA, Metric, Scenario,
  // Contract, ServiceDesign, PriorArt, Gap) fall to renderGeneric until
  // their data surface is rich enough to justify a custom body.
  if (d.instances && d.instances.length > 0) {
    var typeGroups = {};
    d.instances.forEach(function(inst) {
      var t = inst.type || 'Instance';
      if (!typeGroups[t]) typeGroups[t] = [];
      typeGroups[t].push(inst);
    });
    // #2431 — pluralize type names for group headers.
    // Irregulars + y→ies rule; default is + 's'.
    // #2485 round 2 — Decision instance group disambiguates from the protocol
    // Decisions herald (which filters DECs to this domain via the aggregation
    // route). Instance group is the chorus:Decision class membership view.
    var PLURAL_OVERRIDES = { AlertRule: 'Alerts', Decision: 'Decision Instances' };
    function pluralize(name) {
      if (PLURAL_OVERRIDES[name]) return PLURAL_OVERRIDES[name];
      if (/[^aeiou]y$/i.test(name)) return name.slice(0, -1) + 'ies';  // Policy → Policies
      return name + 's';
    }
    Object.keys(typeGroups).forEach(function(typeName) {
      var group = typeGroups[typeName];
      var headerName = pluralize(typeName);
      var collapsed = group.length > 20;
      var groupId = 'instgroup-' + typeName.toLowerCase().replace(/[^a-z0-9]/g, '');
      html += '<details><summary style="cursor:pointer;font-size:1.2em;font-weight:600;padding:8px 0;">' + headerName + ' (' + group.length + ')</summary>';
      html += '<div style="padding:0 0 12px 16px;" data-instance-group="' + typeName + '" id="' + groupId + '">';
      if (collapsed) {
        html += '<button class="instance-expand-all" data-group-id="' + groupId + '" style="font-size:0.85em;background:#1e293b;color:#93c5fd;border:1px solid #334155;border-radius:4px;padding:2px 8px;margin-bottom:8px;cursor:pointer;">Expand all</button>';
      }
      // #2431 — Principle type uses nested parent→child tree (mirrors
      // /loom/principles.html prior-art shape). Other types render flat.
      if (typeName === 'Principle') {
        html += renderPrincipleTree(group);
      } else {
        group.forEach(function(inst) {
          html += renderInstanceBody(inst, typeName);
        });
      }
      html += '</div></details>';
    });
  }

  // --- ACTIVE CARDS (foldable #1932) ---
  // (#2485 round 2) — strays from the old leave-open pattern removed; Cards
  // now self-closes so the second-wave herald loop is not nested under Cards.

  html += '<details><summary style="cursor:pointer;font-size:1.2em;font-weight:600;padding:8px 0;">Cards (' + (cards ? cards.length : 0) + ')</summary>';
  if (cards && cards.length > 0) {
    html += '<div class="card-sort-controls" style="margin-bottom:8px;font-size:0.85em;">';
    html += 'Sort: <button data-sort="status" style="cursor:pointer;background:#1e293b;color:#93c5fd;border:1px solid #334155;border-radius:4px;padding:2px 8px;margin:0 4px;">Status</button>';
    html += '<button data-sort="priority" style="cursor:pointer;background:#1e293b;color:#93c5fd;border:1px solid #334155;border-radius:4px;padding:2px 8px;margin:0 4px;">Priority</button>';
    html += '<button data-sort="owner" style="cursor:pointer;background:#1e293b;color:#93c5fd;border:1px solid #334155;border-radius:4px;padding:2px 8px;margin:0 4px;">Owner</button>';
    html += '<button data-sort="id" style="cursor:pointer;background:#1e293b;color:#93c5fd;border:1px solid #334155;border-radius:4px;padding:2px 8px;margin:0 4px;">#</button>';
    html += '</div>';
    _allCards = cards;
    html += '<div id="cards-list">';
    html += renderCardRows(cards);
    html += '</div>';
  } else {
    html += '<p class="placeholder">No cards for this domain</p>';
  }
  html += '</details>';  // close Cards

  // Second wave — facets that come after Cards. Logs/Alerts/Gaps via heralds;
  // Blast Radius render-empty inline (data shape is just an array, not facet-shaped).
  ['logs', 'alerts', 'gaps'].forEach(function(key) {
    var facet = HERALD_FACETS.find(function(f) { return f.key === key; });
    if (facet) html += renderHeraldFacet(facet, dataMap[key], heraldCtx);
  });

  // Blast Radius — always render (Jeff's rule); empty placeholder if no consumers.
  var blast = blastConsumers || [];
  html += '<details><summary style="cursor:pointer;font-size:1.2em;font-weight:600;padding:8px 0;">Blast Radius' + (blast.length ? ' (' + blast.length + ')' : '') + '</summary>';
  if (blast.length > 0) {
    html += '<div class="blast"><h3>If ' + d.label + ' fails:</h3>';
    blast.forEach(function(c) {
      html += '<div style="padding:4px 0;">' + c.label + ' — consumes this service</div>';
    });
    html += '</div>';
  } else {
    html += '<p class="placeholder">No consumers depend on this domain</p>';
  }
  html += '</details>';

  document.getElementById('content-sections').innerHTML = html;

  // --- Herald auto-wiring: enrich empty facets with herald data (#2104) ---
  // #2683: skip facets that use extract/customRender — those are special-cased
  // (e.g., dependencies has direct.consumes/consumedBy shape, not a flat list).
  // The first-pass herald loop above already rendered them correctly via
  // customRender; this auto-wiring loop only handles the flat-list listKey
  // shape. Without the skip, this loop overwrites the correct render with
  // count=0 and an undefined emptyMsg.
  HERALD_FACETS.forEach(function(facet) {
    if (facet.extract || facet.customRender) return;
    var ep = facet.endpoint(domainId);
    tracedFetch(ep).then(function(r) { return r.ok ? r.json() : null; }).then(function(raw) {
      if (!raw) return;
      var data = raw.data || raw;
      var items = data[facet.listKey] || (facet.altKey ? data[facet.altKey] : null) || [];
      // Find the existing section by title and update its content
      var details = document.querySelectorAll('details');
      for (var i = 0; i < details.length; i++) {
        var summary = details[i].querySelector('summary');
        if (summary && summary.textContent.indexOf(facet.title) === 0) {
          // Update count in summary
          summary.textContent = facet.title + ' (' + items.length + ')';
          // Replace content after summary
          var existingContent = details[i].innerHTML;
          var summaryEnd = existingContent.indexOf('</summary>') + '</summary>'.length;
          var newContent = existingContent.substring(0, summaryEnd);
          if (items.length > 0) {
            newContent += '<table><tr>';
            facet.columns.forEach(function(col) { newContent += '<th>' + col.charAt(0).toUpperCase() + col.slice(1) + '</th>'; });
            newContent += '</tr>';
            items.forEach(function(item) {
              newContent += '<tr>';
              facet.columns.forEach(function(col) {
                var val = item[col] || '';
                if (col === 'route' && val.startsWith('/')) val = '<a href="' + val + '">' + val + '</a>';
                else if (col === 'path') newContent += '<td><code>' + val + '</code></td>';
                else newContent += '<td>' + val + '</td>';
                if (col === 'path') return; // already added
              });
              newContent += '</tr>';
            });
            newContent += '</table>';
          } else {
            newContent += '<p class="placeholder">' + facet.emptyMsg + '</p>';
          }
          details[i].innerHTML = newContent;
          break;
        }
      }
    }).catch(function() {});
  });

  // Load trace integrations (#2101)
  var domainLabel = domainId.replace(/-(?:domain|service|analytics)$/, '');
  fetch('http://localhost:3340/api/chorus/trace/integrations/' + domainLabel)
    .then(function(r) { return r.ok ? r.json() : null; })
    .then(function(data) {
      var el = document.getElementById('trace-integrations');
      if (!el || !data || !data.integrations || data.integrations.length === 0) return;
      var h = '<div style="margin-top:8px;font-size:0.85em;color:#64748b;">Observed from traces:</div>';
      h += '<table><tr><th>Source</th><th>Destination</th><th>Calls</th><th>Avg Latency</th></tr>';
      data.integrations.forEach(function(ig) {
        h += '<tr><td>' + ig.source + '</td><td>' + ig.destination + '</td><td>' + ig.count + '</td><td>' + ig.avgLatencyMs + 'ms</td></tr>';
      });
      h += '</table>';
      el.innerHTML = h;
    })
    .catch(function() {});

  // Render mermaid
  if (typeof mermaid !== 'undefined') {
    mermaid.run({ nodes: document.querySelectorAll('.mermaid') }).catch(function() {});
  }
}

// Lazy-load card detail on expand
document.addEventListener('toggle', async function(e) {
  const details = e.target;
  if (!details.classList.contains('card-expand') || !details.open) return;
  const cardId = details.dataset.cardId;
  const container = details.querySelector('.card-expand-body');
  if (container.dataset.loaded) return;
  container.dataset.loaded = '1';
  try {
    const res = await fetch(ATHENA + '/card/' + cardId);
    if (!res.ok) throw new Error('API ' + res.status);
    const body = await res.json();
    const c = body.data || {};
    container.innerHTML = '<p>' + (c.description || 'No description') + '</p>';
  } catch (err) {
    container.innerHTML = '<p style="color:#dc2626;">Could not load card: ' + err.message + '</p>';
  }
}, true);

// Lazy-load child domain detail on expand
document.addEventListener('toggle', async function(e) {
  const details = e.target;
  if (!details.classList.contains('child-domain') || !details.open) return;
  const childId = details.dataset.childId;
  const container = details.querySelector('.child-detail');
  if (container.dataset.loaded) return;
  container.dataset.loaded = '1';
  try {
    const [detailRes, actorsRes] = await Promise.all([
      fetch(ATHENA + '/subdomains/' + childId),
      fetch(ATHENA + '/subdomains/' + childId + '/actors'),
    ]);
    const cd = detailRes.ok ? (await detailRes.json()).data : null;
    const actorsBody = actorsRes.ok ? await actorsRes.json() : { data: { actors: [] } };
    if (!cd) { container.innerHTML = '<p style="color:#dc2626;">Could not load</p>'; return; }
    let inner = '';
    if (cd.comment) inner += '<p>' + cd.comment + '</p>';
    const actors = actorsBody.data ? actorsBody.data.actors || [] : [];
    if (actors.length > 0) {
      const mId = 'mermaid-child-' + childId + '-' + Date.now();
      let chart = 'graph LR\n';
      actors.forEach(function(a, i) {
        const safe = (a.label || 'Actor').replace(/[^a-zA-Z0-9 ]/g, '');
        chart += '  A' + i + '["' + safe + '"] -->|"' + (a.action || '').substring(0, 60) + '"| D["' + cd.label + '"]\n';
      });
      inner += '<div class="mermaid" id="' + mId + '">' + chart + '</div>';
      setTimeout(function() { mermaid.run({ nodes: [document.getElementById(mId)] }); }, 50);
    }
    if (cd.consumedBy && cd.consumedBy.length > 0) {
      inner += '<div style="margin:4px 0;"><strong style="color:#999;font-size:0.85em;">Consumed By:</strong> ';
      inner += cd.consumedBy.map(function(c) { return '<a href="domain-detail.html?id=' + (c.uri ? c.uri.split('#').pop() : '') + '">' + c.label + '</a>'; }).join(', ');
      inner += '</div>';
    }
    container.innerHTML = inner || '<p class="placeholder">No additional detail</p>';
  } catch (err) {
    container.innerHTML = '<p style="color:#dc2626;">Error: ' + err.message + '</p>';
  }
}, true);

function openFile(filePath) {
  fetch('/api/chorus/open', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path: filePath })
  }).catch(function() {});
}

// --- Card sort + inline detail (#1933) ---

var _allCards = [];
var _currentSort = 'status';
var _sortAsc = true;

var statusOrder = { WIP: 0, Next: 1, Later: 2, Done: 3 };
var statusColors = { WIP: '#f59e0b', Next: '#3b82f6', Later: '#6b7280', Done: '#22c55e' };

function renderCardRows(cards) {
  var html = '';
  cards.forEach(function(c) {
    var color = statusColors[c.status] || '#6b7280';
    html += '<details class="card-expand" data-card-id="' + (c.id || '') + '">';
    html += '<summary style="cursor:pointer;padding:6px 0;display:flex;align-items:center;gap:8px;border-bottom:1px solid #1e293b;">';
    html += '<span style="color:#64748b;font-size:0.85em;min-width:40px;">#' + (c.id || '?') + '</span>';
    html += '<span style="flex:1;">' + (c.title || '') + '</span>';
    if (c.owner) html += '<span style="font-size:0.8em;color:#94a3b8;">' + c.owner + '</span>';
    if (c.priority) html += '<span style="font-size:0.75em;color:#94a3b8;background:#1e293b;padding:1px 6px;border-radius:3px;">' + c.priority + '</span>';
    html += '<span style="font-size:0.75em;padding:2px 8px;border-radius:4px;background:' + color + '22;color:' + color + ';border:1px solid ' + color + '44;">' + (c.status || '?') + '</span>';
    html += '</summary>';
    html += '<div class="card-detail-body" style="padding:8px 0 16px 48px;"><p class="placeholder" style="color:#64748b;">Loading...</p></div>';
    html += '</details>';
    });
  return html;
}

function sortCards(field) {
  if (_currentSort === field) { _sortAsc = !_sortAsc; } else { _sortAsc = true; }
  _currentSort = field;
  var dir = _sortAsc ? 1 : -1;
  _allCards.sort(function(a, b) {
    var cmp = 0;
    if (field === 'status') cmp = (statusOrder[a.status] || 9) - (statusOrder[b.status] || 9);
    else if (field === 'priority') cmp = (a.priority || 'P9').localeCompare(b.priority || 'P9');
    else if (field === 'owner') cmp = (a.owner || 'z').localeCompare(b.owner || 'z');
    else if (field === 'id') cmp = parseInt(a.id || '0') - parseInt(b.id || '0');
    return cmp * dir;
  });
  var el = document.getElementById('cards-list');
  if (el) el.innerHTML = renderCardRows(_allCards);
}

// Sort button delegation (#1933 — CSP blocks inline onclick)
document.addEventListener('click', function(e) {
  var btn = e.target;
  if (btn.tagName === 'BUTTON' && btn.dataset.sort) {
    sortCards(btn.dataset.sort);
  }
  // #2431 — expand-all within a collapsed instance group
  if (btn.tagName === 'BUTTON' && btn.classList.contains('instance-expand-all')) {
    var groupId = btn.dataset.groupId;
    var group = document.getElementById(groupId);
    if (group) {
      group.querySelectorAll('details.instance-body').forEach(function(d) { d.open = true; });
      btn.textContent = 'Collapse all';
      btn.classList.remove('instance-expand-all');
      btn.classList.add('instance-collapse-all');
    }
    return;
  }
  if (btn.tagName === 'BUTTON' && btn.classList.contains('instance-collapse-all')) {
    var gId = btn.dataset.groupId;
    var g = document.getElementById(gId);
    if (g) {
      g.querySelectorAll('details.instance-body').forEach(function(d) { d.open = false; });
      btn.textContent = 'Expand all';
      btn.classList.remove('instance-collapse-all');
      btn.classList.add('instance-expand-all');
    }
  }
});

// Lazy-load card detail on expand (#1933)
document.addEventListener('toggle', async function(e) {
  var details = e.target;
  if (!details.classList || !details.classList.contains('card-expand') || !details.open) return;
  var cardId = details.dataset.cardId;
  var container = details.querySelector('.card-detail-body');
  if (!container || container.dataset.loaded) return;
  container.dataset.loaded = '1';

  try {
    var res = await fetch(ATHENA + '/card/' + cardId);
    if (!res.ok) { container.innerHTML = '<p style="color:#ef4444;">Could not load card</p>'; return; }
    var body = await res.json();
    var cd = body.data;
    var inner = '';

    // AC items
    if (cd.ac_items && cd.ac_items.length > 0) {
      inner += '<div style="margin-bottom:8px;"><strong style="color:#94a3b8;font-size:0.85em;">AC (' + cd.ac_items.filter(function(a){return a.checked}).length + '/' + cd.ac_items.length + ')</strong><ul style="margin:4px 0;padding-left:20px;list-style:none;">';
      cd.ac_items.forEach(function(ac) {
        var check = ac.checked ? '☑' : '☐';
        var style = ac.checked ? 'color:#16a34a;' : 'color:#1e293b;';
        inner += '<li style="' + style + 'margin:2px 0;font-size:0.9em;">' + check + ' ' + ac.text + '</li>';
      });
      inner += '</ul></div>';
    }

    // Domain labels as chips
    if (cd.domains && cd.domains.length > 0) {
      inner += '<div style="margin-bottom:8px;display:flex;flex-wrap:wrap;gap:4px;">';
      cd.domains.forEach(function(d) {
        inner += '<span style="font-size:0.75em;padding:2px 8px;border-radius:4px;background:#1e293b;color:#94a3b8;border:1px solid #334155;">' + d + '</span>';
      });
      inner += '</div>';
    }

    // Metadata
    inner += '<div style="font-size:0.8em;color:#64748b;">';
    if (cd.owner) inner += 'Owner: <strong>' + cd.owner + '</strong> · ';
    if (cd.priority) inner += '<span style="font-size:0.85em;padding:1px 6px;border-radius:3px;background:#e0f2fe;color:#0369a1;margin-right:4px;">' + cd.priority + '</span> · ';
    if (cd.created) inner += 'Created: ' + cd.created.slice(0, 10);
    inner += '</div>';

    // Comments
    if (cd.comments && cd.comments.length > 0) {
      inner += '<div style="margin-top:8px;"><strong style="color:#94a3b8;font-size:0.85em;">Comments (' + cd.comments.length + ')</strong>';
      cd.comments.forEach(function(comment) {
        inner += '<div style="margin:4px 0;padding:4px 8px;background:#f1f5f9;border-radius:4px;font-size:0.85em;border-left:2px solid #cbd5e1;">';
        inner += '<span style="color:#64748b;">[' + comment.author + ']</span> ' + comment.text.slice(0, 200);
        if (comment.text.length > 200) inner += '…';
        inner += '</div>';
      });
      inner += '</div>';
    }

    container.innerHTML = inner || '<p style="color:#64748b;">No additional detail</p>';
  } catch (err) {
    container.innerHTML = '<p style="color:#ef4444;">Error: ' + err.message + '</p>';
  }
}, true);

init();
