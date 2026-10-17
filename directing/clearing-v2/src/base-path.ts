// @domain: messages
// #4571 — v2 is reached at lightlifeurbangardens.com/clearing-v2 (Jeff's path ruling) and the
// tunnel does not strip the prefix. Same rule as the current Clearing's /clearing (#3872):
// strip it before routing so every route serves under '/' and '/clearing-v2', and give the
// page the prefix to build its own URLs with.
export const CLEARING_V2_BASE = '/clearing-v2';

export function stripBase(url: string): { url: string; base: string } {
  if (url !== CLEARING_V2_BASE && !url.startsWith(`${CLEARING_V2_BASE}/`) && !url.startsWith(`${CLEARING_V2_BASE}?`)) {
    return { url, base: '' };
  }
  return { url: url.slice(CLEARING_V2_BASE.length) || '/', base: CLEARING_V2_BASE };
}

/** Root-absolute src/href/action in served HTML get the prefix; '//' URLs are left alone. */
export function withBase(html: string, base: string): string {
  if (!base) return html;
  return html
    .replace(/(src|href|action)="\/(?!\/)/g, `$1="${base}/`)
    // the importmap's targets ("@noble/curves/": "/vendor/...") and inline fetches
    .replace(/"\/vendor\//g, `"${base}/vendor/`)
    .replace(/fetch\('\/(?!\/)/g, `fetch('${base}/`);
}

/** The prefix the page's scripts build fetch/EventSource URLs with. */
export function injectBase(html: string, base: string): string {
  return html.includes('</head>') ? html.replace('</head>', `<script>window.CLEARING_BASE="${base}";</script></head>`) : html;
}
