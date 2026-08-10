/**
 * #3798 — the base path, in one place.
 *
 * Chorus surfaces are moving from three hostnames to one host with paths
 * (Jeff, 2026-08-08): lightlifeurbangardens.com/chorus, /gathering, /borg.
 * cloudflared matches on path and does NOT rewrite it, so the guard receives
 * /chorus/athena and strips its own prefix before proxying — which means the
 * app still serves at root while the BROWSER's URL carries /chorus.
 *
 * That gap is why relative links are wrong here. From /chorus/domains/<name>,
 * a relative "athena/model.html" resolves to /chorus/domains/athena/model.html.
 * A configured base does not have that failure mode.
 *
 * Resolution order, most specific first:
 *   1. <meta name="chorus-base" content="/chorus">   — explicit, wins
 *   2. inferred from location.pathname when the page is served under a known
 *      prefix — so a page works under /chorus without being rebuilt
 *   3. "" — today's behaviour at root, byte-for-byte
 *
 * Deliberately NOT read from a build step: the same file must serve correctly
 * at BOTH root and /chorus during the move, so that Silas's tunnel change and
 * this one do not have to land in the same instant. A window where one is
 * deployed and the other is not is exactly the shape that broke this week.
 */
(function () {
  var KNOWN_PREFIXES = ['/chorus'];

  function fromMeta() {
    var m = document.querySelector('meta[name="chorus-base"]');
    return m && m.getAttribute('content') ? m.getAttribute('content') : null;
  }

  function fromLocation() {
    var p = (window.location && window.location.pathname) || '';
    for (var i = 0; i < KNOWN_PREFIXES.length; i++) {
      var pre = KNOWN_PREFIXES[i];
      if (p === pre || p.indexOf(pre + '/') === 0) return pre;
    }
    return null;
  }

  var BASE = fromMeta();
  if (BASE === null) BASE = fromLocation();
  if (BASE === null) BASE = '';
  // normalise: no trailing slash, so BASE + '/athena' is always well-formed
  BASE = BASE.replace(/\/+$/, '');

  /**
   * Absolute app path, base-aware. Pass the path as the app knows it —
   * '/athena/model.html' — never the browser-visible one.
   */
  function basePath(p) {
    if (!p) return BASE || '/';
    if (/^[a-z]+:\/\//i.test(p) || p.indexOf('//') === 0) return p; // external, untouched
    if (p.charAt(0) !== '/') p = '/' + p;
    return BASE + p;
  }

  window.CHORUS_BASE = BASE;
  window.basePath = basePath;

  /**
   * Rewrite root-absolute hrefs/srcs already in the DOM. This exists so pages
   * that have not yet been converted still work under a prefix — the move is
   * incremental and a half-converted page must not be a broken page.
   * Skips anything already carrying the base, and anything external.
   */
  window.applyBasePath = function applyBasePath(root) {
    if (!BASE) return 0;
    var scope = root || document;
    var n = 0;
    ['href', 'src'].forEach(function (attr) {
      var nodes = scope.querySelectorAll('[' + attr + '^="/"]');
      for (var i = 0; i < nodes.length; i++) {
        var v = nodes[i].getAttribute(attr);
        if (!v || v.indexOf('//') === 0) continue;          // protocol-relative: external
        if (v.indexOf(BASE + '/') === 0 || v === BASE) continue; // already based
        nodes[i].setAttribute(attr, BASE + v);
        n++;
      }
    });
    return n;
  };

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', function () { window.applyBasePath(); });
  } else {
    window.applyBasePath();
  }
})();
