// #3635 — shared runtime for the Athena page family (products / value-stream /
// domains / product / domain). Every page fetches owl-api (:3360) LIVE on load;
// an unreachable API renders an honest failure state, never an empty-but-confident
// page (#3627 contract). Multi-valued fields arrive as string-or-array (ADR-047
// additive) — asArray() normalizes.
// #3644 — same-origin via the chorus-api /owl proxy: one base that works on the
// LAN and through any share/tunnel origin (the old `hostname:3360` broke off-LAN).
// #3807 — through the configured base, because /chorus IS the root of the project
// and not a prefix each page compensates for individually. From
// lightlifeurbangardens.com/chorus a bare '/owl' resolves against the APEX — the
// garden site — which correctly answers not-found, so every page drew its frame
// and none of its content: "loading the stream live from the model…" forever.
// Jeff signed in and got an empty shell that read as a page somebody invented.
//
// Resolved PER CALL, never captured at load: six of the eight pages that use this
// file load it BEFORE base-path.js, so a constant computed here would read an
// undefined helper, fall back to '/owl', and go on quietly serving the empty frame
// on exactly the pages nobody re-checked — a fix that works where you tested it and
// nowhere else. Asking at call time removes the ordering dependency instead of
// asking eight files to remember it. At root the base is empty and every URL is
// byte-for-byte what it was.
function owlBase() {
  return (typeof window !== 'undefined' && typeof window.basePath === 'function')
    ? window.basePath('/owl')
    : '/owl';
}

async function fetchJSON(path) {
  const r = await fetch(owlBase() + path);
  if (!r.ok) throw new Error(`${r.status} on ${path}`);
  return r.json();
}

function fetchFailed(el, e) {
  el.innerHTML = `<div class="err"><strong>Cannot reach the model API</strong> (${owlBase()}) — ` +
    `this is a fetch failure, not an empty model. ${esc(String(e.message || e))}</div>`;
}

// #4472 — does a crawled unit's runsService name this service? The door serves the edge
// as the target's IRI local name, which carries the class slug (service-werk), while the
// pages are opened by the row name (werk).
function runsServiceNames(v, s) { v = String(v || ''); return v === s || v === 'service-' + s || v.endsWith(':' + s); }

// #4472 — what a crawled unit is doing: an instance's runState, or how a job's last run
// ended (launchd's exit status). No code means none was recorded: the job has not
// run, or the crawl could not tell (bedroom reports only failures).
function unitState(u) {
  if (u.runState) return u.runState;
  const c = u.lastExitCode;
  return (c === '' || c == null) ? 'no exit recorded' : (String(c) === '0' ? 'last run ok' : `last run exit ${c}`);
}

// #4472 — a stored UTC timestamp, shown in the viewer's own time zone.
function localTime(iso) {
  const d = new Date(iso || '');
  return isNaN(d) ? '' : d.toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
}

// #5044 — how long ago a stored time was, in words a reader needs no clock for: "just now",
// "7 min ago", "3 h ago", "2 days ago". '' for a value that is not a time.
function agoText(iso, now) {
  const t = new Date(iso || '').getTime();
  if (isNaN(t)) return '';
  const min = Math.max(0, Math.round(((now || Date.now()) - t) / 60000));
  if (min < 1) return 'just now';
  if (min < 60) return min + ' min ago';
  if (min < 48 * 60) return Math.round(min / 60) + ' h ago';
  return Math.round(min / 1440) + ' days ago';
}

// #5044 — a card number opens that card where Clearing v2 shows it (Jeff, 2026-11-25 15:50: "cant
// link to cards like clearing v2 does"). Clearing v2 is its own service, so its address depends
// on where the reader stands, measured 2026-11-25:
//   the public site, under /chorus      the same host, /clearing-v2/<n>
//   the public site, the chorus. host   that host answers 404 for it; the apex host has it
//   the home network (a port is named)  the same machine, Clearing v2's own port, /<n>
// `loc` is the page's location; a test hands in others. Not a number: no address (plain text).
function cardHref(id, loc) {
  const n = String(id == null ? '' : id).replace(/^#/, '');
  if (!/^\d+$/.test(n)) return null;
  loc = loc || window.location;
  if (loc.port) return `${loc.protocol}//${loc.hostname}:3472/${n}`;
  // a whole address, host included: the mount's link rewriter keeps every root-absolute href
  // inside /chorus, and Clearing v2 is not inside it
  const host = String(loc.hostname || '');
  return `${loc.protocol}//${host.indexOf('chorus.') === 0 ? host.slice(7) : host}/clearing-v2/${n}`;
}

// #5044 — a table wider than its box says so. Jeff's phone, 2026-11-25: file paths, release
// titles and the Pipeline summary ran off the right edge of five folds. Each table already
// scrolled inside its fold (#5041), but nothing showed that it did: it read as cut-off text.
// Every scroll box (.tbl) whose table is wider than the box gets one line ABOVE it (under a long
// table the line was a screen or more below the cut the reader was looking at); a table that
// fits gets none. Measured when a fold is drawn, opened, or the window changes width.
function markCutTables(root) {
  for (const box of (root || document).querySelectorAll('.tbl')) {
    const prev = box.previousElementSibling;
    const cue = prev && prev.classList.contains('cut-cue') ? prev : null;
    if (!box.clientWidth) continue;                       // inside a closed fold: measured when it opens
    const cut = box.scrollWidth > box.clientWidth + 1;
    if (cut && !cue) box.insertAdjacentHTML('beforebegin', '<p class="cut-cue">more to the right: slide the table sideways &rarr;</p>');
    if (!cut && cue) cue.remove();
  }
}
if (typeof document !== 'undefined') {
  let due = null;
  const soon = () => { if (!due) due = setTimeout(() => { due = null; markCutTables(document); }, 60); };
  document.addEventListener('DOMContentLoaded', () => {
    // the folds are drawn after their reads come back, one by one
    // (the line this adds is itself a change: the pass it causes finds nothing to do, and it ends there)
    new MutationObserver(soon)
      .observe(document.body, { childList: true, subtree: true });
    soon();
  });
  document.addEventListener('toggle', soon, true);   // a fold opening (toggle does not bubble: capture)
  window.addEventListener('resize', soon);
}

function asArray(v) {
  if (Array.isArray(v)) return v;
  if (v === undefined || v === null || v === '') return [];
  return [String(v)];
}

function esc(s) {
  return String(s ?? '').replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
}

function stepLocal(v) {
  return String(v || '').split('#').pop().replace('value-stream-step-', '').toLowerCase();
}

// #5041 — owners are principal rows now (principal-wren); the page showed the raw id
// (Jeff's phone, 2026-11-25). Both row prefixes come off, so the reader sees "wren".
function ownerLocal(v) {
  return String(v || '').split('#').pop().replace(/^chorus:/, '').replace(/^(role|principal)-/, '');
}

function ownerDot(owner) {
  const o = ownerLocal(owner);
  return o ? `<span class="owner-dot ${esc(o)}"></span>${esc(o)}` : '<span class="empty">no owner</span>';
}

function stepBadge(step) {
  const s = stepLocal(step);
  // #3702 — no hardcoded step list: the class IS the model's step name (CSS styles
  // the stages it knows; an unknown stage renders as a plain badge, never breaks).
  const cls = /^[a-z][a-z-]*$/.test(s) ? s : '';
  return s ? `<span class="badge ${cls}">${esc(s[0].toUpperCase() + s.slice(1))}</span>` : '<span class="empty">no step</span>';
}

// #4482 — one parent per row: a Product or Service stores no step of its own; the step is
// read up the chain. A product sits at the steps of its domains (hasDomain); a service at
// the step of the domain that hosts it (chorus:hosts names service-<name> or <name>).
function bareLocal(v) {
  return String(v || '').replace(/^.*[#/]/, '').replace(/^chorus:/, '');
}

function chainSteps(domainRows) {
  return [...new Set(domainRows.map(d => stepLocal(d && d.atStep)).filter(Boolean))].sort();
}

function productSteps(product, domains) {
  const byName = Object.fromEntries((domains || []).map(d => [d.name, d]));
  return chainSteps(asArray(product && product.hasDomain).map(n => byName[bareLocal(n)]));
}

function serviceSteps(serviceName, domains) {
  const s = bareLocal(serviceName);
  return chainSteps((domains || []).filter(d => asArray(d.hosts).map(bareLocal).some(h => h === s || h === `service-${s}`)));
}

function stepBadges(steps) {
  return steps && steps.length ? steps.map(stepBadge).join(' ') : stepBadge('');
}

function domainChips(hasDomain) {
  const doms = asArray(hasDomain);
  if (!doms.length) return '<span class="empty">no domains declared</span>';
  return doms.map(d => `<a class="chip" href="domain.html?d=${encodeURIComponent(d)}">${esc(d)}</a>`).join('');
}

// #3761 — the journey ladder, one shared nav on every integrated surface.
// "Where am I, up one level, the ladder": the crumb answers the first two;
// this answers the third. Rendered from one list so the rungs cannot drift
// between pages. current = the key of the page being viewed (or '' for none).
const JOURNEY_RUNGS = [
  { key: 'chorus', label: 'Chorus', href: '/' },
  { key: 'value-stream', label: 'Value stream', href: '/athena/value-stream.html' },
  { key: 'products', label: 'Products', href: '/athena/products.html' },
  { key: 'services', label: 'Services', href: '/athena/services.html' },   // #4010 — services had an API and no page for months
  { key: 'domains', label: 'Domains', href: '/athena/domains.html' },
  { key: 'model', label: 'Model (ERD)', href: '/athena/model.html' },
  { key: 'instances', label: 'Instances', href: '/domains' },
];

function journeyNav(current) {
  return '<nav class="ladder">' + JOURNEY_RUNGS.map(r =>
    r.key === current
      ? `<span class="rung here">${esc(r.label)}</span>`
      : `<a class="rung" href="${r.href}">${esc(r.label)}</a>`
  ).join('') + '</nav>';
}

// injects the ladder above the page's .crumb (or at top of .wrap) on DOM ready
function mountJourneyNav(current) {
  document.addEventListener('DOMContentLoaded', () => {
    const wrap = document.querySelector('.wrap') || document.body;
    const el = document.createElement('div');
    el.innerHTML = journeyNav(current);
    wrap.insertBefore(el.firstChild, wrap.firstChild);
  });
}

function srcNote(extra) {
  return `<p class="src-note">live from owl-api ${esc(owlBase())} · every field maps 1:1 to a shape property · nothing fabricated${extra ? ' · ' + extra : ''}</p>`;
}
