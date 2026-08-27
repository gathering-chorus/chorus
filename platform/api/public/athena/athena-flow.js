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

function ownerLocal(v) {
  return String(v || '').split('#').pop().replace('role-', '');
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
