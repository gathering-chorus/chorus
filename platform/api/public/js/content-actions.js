/* content-actions — behavior for the per-page PDF / Share / Reflect bar (#3415).
 * Portable, dependency-free; pairs with the .content-actions component in
 * system.css. Inherited generatively like the CSS — no chorus runtime dep.
 * Wire: <script src="/js/content-actions.js" defer></script> after a
 * <div class="content-actions" data-title="..." data-url="..."> ... </div>. */
(function () {
  function toast(bar, msg) {
    var t = bar.querySelector('.action-toast');
    if (!t) { t = document.createElement('span'); t.className = 'action-toast muted'; bar.appendChild(t); }
    t.textContent = msg;
    setTimeout(function () { if (t.textContent === msg) t.textContent = ''; }, 2500);
  }
  function wire(bar) {
    bar.addEventListener('click', function (e) {
      var btn = e.target.closest('[data-btn]'); if (!btn) return;
      var kind = btn.getAttribute('data-btn');
      if (kind === 'print') {
        window.print();                       // @media print in system.css → clean PDF
      } else if (kind === 'share') {
        var url = bar.getAttribute('data-url') || location.pathname;
        var abs = new URL(url, location.origin).href;
        if (navigator.clipboard) navigator.clipboard.writeText(abs).then(function () { toast(bar, 'link copied'); });
        else toast(bar, abs);
      } else if (kind === 'reflect') {
        toast(bar, 'Reflect: wiring pending');  // downstream — AI send not yet wired
      }
    });
  }
  document.addEventListener('DOMContentLoaded', function () {
    document.querySelectorAll('.content-actions').forEach(wire);
  });
})();
