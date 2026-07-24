/* eslint-disable security/detect-non-literal-fs-filename, security/detect-object-injection --
 * Clearing server. fs paths constructed from CHORUS_ROOT, CHORUS_HOME, and
 * server-controlled env constants (BRIDGE_TOKEN_FILE etc). Object indexing is
 * on internally-derived role keys (4-element ROLES tuple) and validated event
 * fields. Auth is enforced by BRIDGE_TOKEN before req-derived routing reaches
 * any sink.
 */
import express, { Request, Response, NextFunction } from 'express';
import { createServer } from 'http';
import { createServer as createHttpsServer } from 'https';
import { Server } from 'socket.io';
import path from 'path';
import { TilePoller } from './tiles';
import { MessageRouter } from './router';
import { ChorusLogTailer } from './tailer';
import { processJeffInput } from './jeff-input';
import { SessionTailer } from './session-tailer';
import { ClearingChat } from './chat';
import { lanAddress, bonjourHost, startupLanLines, detectIpDrift } from './lan-url';
import { isLocalConnection, isTunneled } from './connection-auth';
import { isWebIdAllowed } from './solid-auth';
import {
  makePkce, makeState, signCookie, verifyCookie, safeReturnPath, buildAuthUrl,
  exchangeCodeForWebId, type OidcConfig,
} from './solid-oidc';
import { gateDecision } from './server-auth';

const PORT = parseInt(process.env.COMMAND_CHANNEL_PORT || '3470');
// #2575: fail-loud on missing CHORUS_ROOT. Earlier silent fallback to
// '/Users/jeffbridwell/CascadeProjects' (note: missing /chorus suffix) would
// produce broken paths for every CHORUS_ROOT-derived constant if the env was
// ever unset. com.chorus.clearing.plist sets it in production; manual launches
// must too. Same family as #2505 prod-tier fallback discussion.
const CHORUS_ROOT = process.env.CHORUS_ROOT;
if (!CHORUS_ROOT) {
  throw new Error('CHORUS_ROOT must be set; expected /Users/jeffbridwell/CascadeProjects/chorus');
}

// Team-scan dir (role observations + pids). Env-overridable so tests isolate it to a
// fixture dir — same convention as tiles.ts. Default unchanged: prod reads the live
// /tmp/claude-team-scan. Hardcoding it made /api/stream merge live observations into
// test fixtures → non-deterministic SSE-test flake under load (#3528 green-main).
const SCAN_DIR = process.env.CLEARING_SCAN_DIR || '/tmp/claude-team-scan';

// #3604 — extracted from GET /api/commands/:role so the digest formatting is a pure,
// unit-testable function (dedup + nudge-skip + action→emoji map + truncate). Takes the
// raw observations-jsonl content, returns formatted display lines (last 60, deduped).
export function formatObserverDigest(content: string): string[] {
  const out: string[] = [];
  const obsLines = content.trim().split('\n').filter(Boolean);
  const seen = new Set<string>();
  for (const line of obsLines.slice(-60)) {
    try {
      const obs = JSON.parse(line);
      const key = `${obs.ts}|${obs.digest}`;   // dedup the observer double-write
      if (seen.has(key)) continue;
      seen.add(key);
      const ts = new Date(obs.ts).toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit', timeZone: 'America/New_York' });
      const card = obs.card ? ` #${obs.card}` : '';
      let short = obs.digest || '';
      if (short.startsWith('nudging:')) continue;   // role-to-role coordination (#1675)
      short = short.replace(/^bash: bash .*\/scripts\//, '→ ')
                    .replace(/^bash: cd .*? && /, '→ ')
                    .replace(/^bash: /, '→ ')
                    .replace(/^board op: bash .*\/cards /, '📋 ')
                    .replace(/^committing changes$/, '📦 commit')
                    .replace(/^editing /, '✏️ ')
                    .replace(/^writing /, '📝 ')
                    .replace(/^service op: /, '⚙️ ')
                    .replace(/^state change: /, '🔄 ')
                    .replace(/^running tests: /, '🧪 ')
                    .replace(/^building: /, '🔨 ')
                    .replace(/^agent: /, '🤖 ')
                    .replace(/^skill: /, '⚡ ');
      if (short.length > 70) short = short.substring(0, 67) + '...';
      out.push(`${ts}  ${short}${card}`);
    } catch { /* skip malformed line */ }
  }
  return out;
}

// Auth token for remote access (#1719)
// Generate a stable token per machine — persists across restarts
const crypto = require('crypto');
const CHORUS_HOME = `${require('os').homedir()}/.chorus`;
const BRIDGE_TOKEN_FILE = `${CHORUS_HOME}/bridge-auth-token`;
let BRIDGE_TOKEN: string;
try {
  BRIDGE_TOKEN = require('fs').readFileSync(BRIDGE_TOKEN_FILE, 'utf-8').trim();
} catch {
  BRIDGE_TOKEN = crypto.randomBytes(16).toString('hex');
  require('fs').mkdirSync(CHORUS_HOME, { recursive: true });
  require('fs').writeFileSync(BRIDGE_TOKEN_FILE, BRIDGE_TOKEN);
}
console.log(`[clearing] remote access token: ${BRIDGE_TOKEN}`);

// #3669 lane 3 — human browser login (Solid-OIDC). Persistent HMAC secret for the
// signed login/session cookies (own secret, not BRIDGE_TOKEN — different lifetime
// and blast radius). 0600, generated once.
const SESSION_SECRET_FILE = `${CHORUS_HOME}/clearing-session-secret`;
let SESSION_SECRET: string;
try {
  SESSION_SECRET = require('fs').readFileSync(SESSION_SECRET_FILE, 'utf-8').trim();
} catch {
  SESSION_SECRET = crypto.randomBytes(32).toString('hex');
  require('fs').mkdirSync(CHORUS_HOME, { recursive: true });
  require('fs').writeFileSync(SESSION_SECRET_FILE, SESSION_SECRET, { mode: 0o600 });
}
// The public issuer is browser-facing; the token exchange runs LOCALLY with the
// Host-override (hardening 3) so it never hairpins Cloudflare or dies with the LAN.
const CSS_PUBLIC_ISSUER = process.env.CSS_ISSUER || 'https://id.lightlifeurbangardens.com';
const CSS_LOCAL_TOKEN = process.env.CSS_LOCAL_TOKEN || 'http://localhost:3001/.oidc/token';
const CSS_TOKEN_HOST = new URL(CSS_PUBLIC_ISSUER).host;
const CLIENT_ID = 'https://clearing.lightlifeurbangardens.com/clientid.jsonld';
// #3669 — the flag Wren named the finish line: default OFF ships the login redirect
// as the default tunneled experience with the bridge token still accepted as the
// migration fallback; flip ON to retire the token (login required, token refused).
const REQUIRE_DPOP = process.env.CHORUS_CLEARING_REQUIRE_DPOP === '1';
// #3669 (Wren) — server-side session lifetime. The signed cookie is otherwise
// valid forever on signature alone (maxAge is only browser-advisory), so a
// captured cookie would never die. The gate enforces this against the payload iat.
const SESSION_MAX_AGE_MS = 30 * 24 * 60 * 60 * 1000;

/** Per-request OIDC config — redirect_uri matches the origin the browser is on. */
function oidcCfg(req: Request): OidcConfig {
  const onTunnel = isTunneled(req.headers);
  const redirectUri = onTunnel
    ? 'https://clearing.lightlifeurbangardens.com/auth/callback'
    : `http://localhost:${PORT}/auth/callback`;
  return {
    issuer: CSS_PUBLIC_ISSUER,
    clientId: CLIENT_ID,
    redirectUri,
    scope: 'openid webid offline_access',
    tokenEndpoint: CSS_LOCAL_TOKEN,
    tokenHost: CSS_TOKEN_HOST,
  };
}

// #3366: LAN URLs derived at boot, never hardcoded — DHCP moved this machine
// off 192.168.86.36 and every printed URL died. The .local Bonjour name is
// canonical (IP-proof); the numeric IP is the fallback. A from→to address
// change emits clearing.lan.ip.drifted so drift is visible, not silent.
function readLocalHostName(): string | null {
  try {
    return require('child_process').execFileSync('scutil', ['--get', 'LocalHostName'],
      { encoding: 'utf-8', timeout: 2000 }).trim() || null;
  } catch {
    return require('os').hostname() || null;
  }
}
const LOCAL_HOST_NAME = readLocalHostName();
for (const line of startupLanLines(PORT, undefined, LOCAL_HOST_NAME)) console.log(line);

const LAN_IP_FILE = `${CHORUS_HOME}/clearing-lan-ip`;
{
  const currentLanIp = lanAddress();
  let previousLanIp: string | null = null;
  try { previousLanIp = require('fs').readFileSync(LAN_IP_FILE, 'utf-8').trim() || null; } catch { /* first boot */ }
  const drift = detectIpDrift(previousLanIp, currentLanIp);
  if (drift.drifted) {
    console.log(`[clearing] LAN IP drifted: ${drift.from} -> ${drift.to} (bookmarks pinned to the old IP are dead; .local URL unaffected)`);
    require('child_process').execFile(`${CHORUS_ROOT}/platform/scripts/chorus-log`,
      ['clearing.lan.ip.drifted', 'system', `from=${drift.from}`, `to=${drift.to}`],
      () => { /* fire-and-forget breadcrumb */ });
  }
  if (currentLanIp) {
    try { require('fs').writeFileSync(LAN_IP_FILE, currentLanIp); } catch { /* non-fatal */ }
  }
}

const app = express();
const server = createServer(app);
const io = new Server(server, {
  cors: { origin: '*' },
  pingInterval: 60000,   // 60s — subscribers are passive listeners, don't need 25s heartbeats
  pingTimeout: 30000,    // 30s tolerance for pong response (#1964)
});

// Cookie parser (minimal — just need bridge_token) — must be before auth
app.use((req: Request, _res, next) => {
  const r = req as Request & { cookies?: Record<string, string> };
  if (!r.cookies) {
    r.cookies = {};
    const cookieHeader = r.headers.cookie ?? '';
    for (const pair of cookieHeader.split(';')) {
      const [key, val] = pair.trim().split('=');
      if (key && val) r.cookies[key] = decodeURIComponent(val);
    }
  }
  next();
});

// Auth middleware — local/LAN requests pass, remote requests need token (#1719).
// #3669 — the tunnel + address logic now lives in one classifier shared with the
// Socket.IO gate below, so the two transports can never drift again (the WS gate
// had drifted: it skipped the cf-header tunnel check and was bypassable).
function isLocal(req: express.Request): boolean {
  const ip = req.ip || req.socket.remoteAddress || '';
  return isLocalConnection(req.headers, ip);
}

// Body parsers BEFORE auth — login form needs req.body parsed (#1782)
app.use(express.json());
app.use(express.urlencoded({ extended: false }));

const TOKEN_COOKIE_OPTS = {
  maxAge: 365 * 24 * 60 * 60 * 1000,
  httpOnly: true,
  sameSite: 'lax' as const,
};

function extractToken(req: Request): string | undefined {
  return (req.query.token as string)
    || req.cookies?.bridge_token
    || req.headers.authorization?.replace('Bearer ', '');
}

function handleAuthenticated(req: Request, res: Response, next: NextFunction) {
  if (req.query.token && !req.cookies?.bridge_token) {
    res.cookie('bridge_token', BRIDGE_TOKEN, TOKEN_COOKIE_OPTS);
  }
  const hasName = req.cookies?.bridge_name;
  if (!hasName && req.path !== '/set-name' && req.path !== '/bridge-og.jpg'
      && (req.path === '/' || req.path === '/index.html')) {
    return res.send(namePage());
  }
  return next();
}

function handleLoginPost(req: Request, res: Response) {
  const { token: submittedToken } = req.body || {};
  if (submittedToken === BRIDGE_TOKEN) {
    res.cookie('bridge_token', BRIDGE_TOKEN, TOKEN_COOKIE_OPTS);
    return res.redirect('/');
  }
  return res.status(401).send(loginPage('Wrong token'));
}

// #3669 — the Solid-OIDC public client-id document. CSS dereferences this URL
// (client_id) UNAUTHENTICATED during the auth-code flow, so it must be served
// before the token gate. Content is Wren's verified draft, verbatim; the
// client_id/redirect_uris are the tunnel origin + localhost. token_endpoint_auth
// _method:none = public client (PKCE, no client secret).
const CLIENTID_DOC = JSON.stringify({
  '@context': 'https://www.w3.org/ns/solid/oidc-context.jsonld',
  client_id: 'https://clearing.lightlifeurbangardens.com/clientid.jsonld',
  client_name: 'The Clearing',
  redirect_uris: [
    'http://localhost:3470/auth/callback',
    'https://clearing.lightlifeurbangardens.com/auth/callback',
  ],
  post_logout_redirect_uris: [
    'http://localhost:3470/',
    'https://clearing.lightlifeurbangardens.com/',
  ],
  grant_types: ['authorization_code', 'refresh_token'],
  response_types: ['code'],
  scope: 'openid webid offline_access',
  token_endpoint_auth_method: 'none',
}, null, 2);

app.use((req, res, next) => {
  void gate(req, res, next).catch(() => {
    if (!res.headersSent) res.status(500).send(errorPage('Something went wrong reaching the room — try again in a moment.'));
  });
});

async function gate(req: Request, res: Response, next: NextFunction): Promise<unknown> {
  if (req.path === '/bridge-og.jpg') return next();
  // #3669 — client-id doc is public (CSS fetches it during OIDC); serve pre-gate.
  if (req.path === '/clientid.jsonld') {
    res.type('application/ld+json').send(CLIENTID_DOC);
    return;
  }
  // #3669 — the login round-trip is pre-auth (the user has no session yet).
  if (req.path === '/auth/login') return handleAuthLogin(req, res);
  if (req.path === '/auth/callback') return handleAuthCallback(req, res);

  const local = isLocal(req);

  // #3669 — "authenticated" now means EITHER the CSS session cookie (the human's
  // primary tunneled auth — signed typ:session, WebID re-checked against the live
  // allow-set every request so a revoked identity loses access within one TTL) OR
  // the static bridge token (migration fallback, retired when REQUIRE_DPOP flips).
  // Both feed the #3667 gateDecision policy, which owns the admin-forbid + the
  // read-pair GET-only rules — so those survive the human-login path unchanged.
  const session = verifyCookie<{ webid?: string; iat?: number }>(req.cookies?.clearing_session, SESSION_SECRET, 'session');
  const sessionFresh = !!session?.iat && Date.now() - session.iat <= SESSION_MAX_AGE_MS;
  const sessionAuthed = !!(session?.webid && sessionFresh && (await isWebIdAllowed(session.webid, Date.now())));
  // eslint-disable-next-line security/detect-possible-timing-attacks -- BRIDGE_TOKEN is a long random value; tunnel auth gate, migration fallback only.
  const tokenAuthed = !REQUIRE_DPOP && extractToken(req) === BRIDGE_TOKEN;

  const outcome = gateDecision(req.path, req.method, local, sessionAuthed || tokenAuthed);
  if (outcome === 'forbid') return res.status(403).json({ error: 'forbidden' });
  if (outcome === 'pass') return local ? next() : handleAuthenticated(req, res, next);

  // auth-required — the migration token login POST still works until the flag flips.
  if (!REQUIRE_DPOP && req.path === '/login' && req.method === 'POST') return handleLoginPost(req, res);
  // #3669 spec (Wren) — the DEFAULT unauth experience on the public arm is the
  // clean login interstitial, no token language. Preserve where the user was
  // heading so the callback lands them in that room.
  res.status(401).send(interstitialPage(safeReturnPath(req.originalUrl)));
}

// #3669 — kick off CSS login: PKCE + CSRF state, stashed in a SIGNED short-lived
// cookie (survives a Clearing restart mid-login), then redirect to CSS.
function handleAuthLogin(req: Request, res: Response): void {
  const cfg = oidcCfg(req);
  const { verifier, challenge } = makePkce();
  const state = makeState();
  const returnPath = safeReturnPath((req.query.return as string) || '/');
  // #3669 (Wren) — carry the redirect_uri in the signed cookie so the exchange
  // presents the EXACT value the auth request used; re-deriving it on the callback
  // risks a random 400 if request classification drifts between the two legs.
  const loginCookie = signCookie(
    { typ: 'login', verifier, state, returnPath, redirectUri: cfg.redirectUri }, SESSION_SECRET);
  res.cookie('clearing_login', loginCookie, {
    httpOnly: true, sameSite: 'lax', secure: isTunneled(req.headers), maxAge: 10 * 60 * 1000, path: '/',
  });
  res.redirect(buildAuthUrl(cfg, state, challenge));
}

// #3669 — CSS redirect-back: verify state (CSRF), exchange the code locally for the
// WebID, check the allow-set, issue the long-lived session cookie, land in the room.
async function handleAuthCallback(req: Request, res: Response): Promise<void> {
  const login = verifyCookie<{ verifier: string; state: string; returnPath: string; redirectUri: string }>(
    req.cookies?.clearing_login, SESSION_SECRET, 'login');
  if (req.query.error) {
    res.status(400).send(errorPage('Login was cancelled or refused. Tap Log in to try again.'));
    return;
  }
  if (!login || !req.query.state || req.query.state !== login.state) {
    res.status(400).send(errorPage('That login attempt expired or didn’t match — tap Log in to try again.'));
    return;
  }
  const code = String(req.query.code || '');
  if (!code) {
    res.status(400).send(errorPage('The login didn’t complete — tap Log in to try again.'));
    return;
  }
  // Exchange with the redirect_uri from the LOGIN leg (cookie), never re-derived.
  const webid = await exchangeCodeForWebId({ ...oidcCfg(req), redirectUri: login.redirectUri }, code, login.verifier);
  if (!webid) {
    res.status(502).send(errorPage('The identity server isn’t answering — the room is fine, try again in a minute.'));
    return;
  }
  if (!(await isWebIdAllowed(webid, Date.now()))) {
    res.status(403).send(errorPage('This identity isn’t on the team list.'));
    return;
  }
  const sessionCookie = signCookie({ typ: 'session', webid, iat: Date.now() }, SESSION_SECRET);
  res.cookie('clearing_session', sessionCookie, {
    httpOnly: true, sameSite: 'lax', secure: isTunneled(req.headers), maxAge: 30 * 24 * 60 * 60 * 1000, path: '/',
  });
  res.clearCookie('clearing_login', { path: '/' });
  res.redirect(login.returnPath);
}

// #3669 — the login interstitial: the DEFAULT unauth experience on the public arm.
// One button → CSS login. NO token language anywhere (Wren spec item 1). Preserves
// where the user was heading so the callback lands them in that room (spec item 2).
function interstitialPage(returnPath: string): string {
  const q = returnPath && returnPath !== '/' ? `?return=${encodeURIComponent(returnPath)}` : '';
  return authShell('Where the team gathers.',
    `<a href="/auth/login${q}" style="text-decoration:none"><button type="button">Log in</button></a>`);
}

// #3669 — errors are sentences, not codes (Wren spec item 4).
function errorPage(message: string): string {
  return authShell(message, '<a href="/auth/login" style="text-decoration:none"><button type="button">Log in</button></a>');
}

// Shared dark shell for the interstitial + error pages — mirrors the namePage look.
function authShell(sub: string, action: string): string {
  return `<!DOCTYPE html><html><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>The Clearing — Chorus</title>
<meta property="og:title" content="The Clearing — Chorus">
<meta property="og:image" content="https://bridge.lightlifeurbangardens.com/bridge-og.jpg">
<style>body{background:#0d1117;color:#e6edf3;font-family:system-ui;display:flex;align-items:center;justify-content:center;height:100vh;margin:0;
background-image:url('/bridge-og.jpg');background-size:cover;background-position:center}
.login{background:rgba(22,27,34,0.92);padding:2rem;border-radius:12px;border:1px solid #30363d;width:300px;backdrop-filter:blur(10px);-webkit-backdrop-filter:blur(10px);text-align:center}
h2{margin:0 0 0.5rem;font-size:1.2rem}
p{color:#8b949e;font-size:0.9rem;margin:0 0 1.2rem;line-height:1.4}
button{width:100%;padding:0.7rem;background:#238636;border:none;color:white;border-radius:6px;font-size:1rem;cursor:pointer}
button:hover{background:#2ea043}</style></head>
<body><div class="login"><h2>The Clearing</h2><p>${sub}</p>${action}</div></body></html>`;
}

function namePage(): string {
  return `<!DOCTYPE html><html><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>The Clearing — Chorus</title>
<meta property="og:title" content="The Clearing — Chorus">
<meta property="og:description" content="Where the team gathers. Three AI roles, one human, real-time.">
<meta property="og:image" content="https://bridge.lightlifeurbangardens.com/bridge-og.jpg">
<meta property="og:type" content="website">
<meta name="twitter:card" content="summary_large_image">
<meta name="twitter:title" content="The Clearing — Chorus">
<meta name="twitter:image" content="https://bridge.lightlifeurbangardens.com/bridge-og.jpg">
<style>body{background:#0d1117;color:#e6edf3;font-family:system-ui;display:flex;align-items:center;justify-content:center;height:100vh;margin:0;
background-image:url('/bridge-og.jpg');background-size:cover;background-position:center}
.login{background:rgba(22,27,34,0.92);padding:2rem;border-radius:12px;border:1px solid #30363d;width:300px;backdrop-filter:blur(10px);-webkit-backdrop-filter:blur(10px)}
h2{margin:0 0 0.5rem;font-size:1.2rem}
p{color:#8b949e;font-size:0.85rem;margin:0 0 1rem}
input{width:100%;padding:0.6rem;background:rgba(13,17,23,0.8);border:1px solid #30363d;color:#e6edf3;border-radius:6px;font-size:1rem;box-sizing:border-box}
button{width:100%;padding:0.6rem;background:#238636;border:none;color:white;border-radius:6px;font-size:1rem;cursor:pointer;margin-top:0.8rem}
button:hover{background:#2ea043}</style></head>
<body><div class="login"><h2>The Clearing</h2><p>Welcome — what's your first name?</p><form method="POST" action="/set-name">
<input name="name" type="text" placeholder="Your name" autofocus autocomplete="given-name">
<button type="submit">Join</button>
</form></div></body></html>`;
}

function loginPage(error?: string): string {
  return `<!DOCTYPE html><html><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>The Clearing — Chorus</title>
<meta property="og:title" content="The Clearing — Chorus">
<meta property="og:description" content="Where the team gathers. Three AI roles, one human, real-time.">
<meta property="og:image" content="https://bridge.lightlifeurbangardens.com/bridge-og.jpg">
<meta property="og:type" content="website">
<meta name="twitter:card" content="summary_large_image">
<meta name="twitter:title" content="The Clearing — Chorus">
<meta name="twitter:image" content="https://bridge.lightlifeurbangardens.com/bridge-og.jpg">
<style>body{background:#0d1117;color:#e6edf3;font-family:system-ui;display:flex;align-items:center;justify-content:center;height:100vh;margin:0}
.login{background:#161b22;padding:2rem;border-radius:12px;border:1px solid #30363d;width:300px}
h2{margin:0 0 1rem;font-size:1.2rem}
input{width:100%;padding:0.6rem;background:#0d1117;border:1px solid #30363d;color:#e6edf3;border-radius:6px;font-size:1rem;box-sizing:border-box}
button{width:100%;padding:0.6rem;background:#238636;border:none;color:white;border-radius:6px;font-size:1rem;cursor:pointer;margin-top:0.8rem}
button:hover{background:#2ea043}
.error{color:#f85149;font-size:0.85rem;margin-top:0.5rem}</style></head>
<body><div class="login"><h2>The Clearing</h2><form method="POST" action="/login">
<input name="token" type="password" placeholder="Access token" autofocus>
<button type="submit">Enter</button>
${error ? `<div class="error">${error}</div>` : ''}
</form></div></body></html>`;
}

// Static files — no cache, plus rewrite index.html to bust browser cache
app.get('/', (req: Request, res) => {
  const fs = require('fs');
  let html = fs.readFileSync(path.join(__dirname, '../public/index.html'), 'utf-8');
  // Inject guest name for remote users (#1719)
  const guestName = req.cookies?.bridge_name || '';
  const isLocalReq = isLocal(req);
  const userName = isLocalReq ? 'jeff' : (guestName || 'guest');
  html = html.replace('</head>', `<script>window.BRIDGE_USER="${userName.replace(/"/g, '')}";</script></head>`);
  res.setHeader('Cache-Control', 'no-store');
  res.setHeader('Content-Type', 'text/html');
  res.send(html);
});
app.use(express.static(path.join(__dirname, '../public'), {
  etag: false,
  maxAge: 0,
  setHeaders: (res) => {
    res.setHeader('Cache-Control', 'no-store, no-cache, must-revalidate');
    res.setHeader('Pragma', 'no-cache');
  }
}));
// Body parsers moved before auth middleware (#1782)

// Guest logout (#1719)
app.get('/logout', (_req, res) => {
  res.clearCookie('bridge_token');
  res.clearCookie('bridge_name');
  res.send(`<!DOCTYPE html><html><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<style>body{background:#0d1117;color:#e6edf3;font-family:system-ui;display:flex;align-items:center;justify-content:center;height:100vh;margin:0}
h2{font-size:1.2rem}</style></head><body><h2>Signed out. Close this tab.</h2></body></html>`);
});

// Guest name registration (#1719)
app.post('/set-name', (req: Request, res) => {
  const name = (req.body?.name || '').trim().substring(0, 30);
  if (!name) return res.redirect('/');
  res.cookie('bridge_name', name, {
    maxAge: 365 * 24 * 60 * 60 * 1000,
    sameSite: 'lax',
  });
  res.redirect('/');
});

// Components
const tilePoller = new TilePoller();
const messageRouter = new MessageRouter();
const tailer = new ChorusLogTailer(messageRouter);

// Persist messages to disk
const MSG_FILE = '/tmp/bridge-messages.json';
const fs_sync = require('fs');
try {
  const saved = JSON.parse(fs_sync.readFileSync(MSG_FILE, 'utf-8'));
  if (Array.isArray(saved)) {
    for (const msg of saved.slice(-100)) {
      messageRouter.ingest(msg);
    }
    console.log(`[clearing] restored ${Math.min(saved.length, 100)} messages from disk`);
  }
} catch { /* ignored */ }

// Save messages every 10 seconds
// #3604 — unref: background save timer must not keep Node's loop alive (jest exit clean).
setInterval(() => {
  try {
    const msgs = messageRouter.getRecent(200, true);
    fs_sync.writeFileSync(MSG_FILE, JSON.stringify(msgs));
  } catch { /* ignored */ }
}, 10000).unref();

// Ensure upload directory survives /tmp cleanup across reboots
import fs_node from 'fs';
import { readSpineLines, type StreamLine } from './spine-tail';
if (!fs_node.existsSync('/tmp/bridge-uploads')) {
  fs_node.mkdirSync('/tmp/bridge-uploads', { recursive: true });
}

// Serve uploaded images
app.use('/uploads', express.static('/tmp/bridge-uploads'));

// Health
app.get('/health', (_req, res) => res.json({ status: 'ok', port: PORT }));

// API: upload image (with HEIC→JPEG conversion)
app.post('/api/upload', (req, res) => {
  const fs = require('fs');
  const { execSync } = require('child_process');
  const chunks: Buffer[] = [];
  req.on('data', (chunk: Buffer) => chunks.push(chunk));
  req.on('end', () => {
    const body = Buffer.concat(chunks);
    const contentType = req.headers['content-type'] || 'image/png';
    const ts = Date.now();

    if (contentType.includes('heic') || contentType.includes('heif')) {
      // Save HEIC, convert to JPEG with sips
      const heicPath = `/tmp/bridge-uploads/${ts}.heic`;
      const jpgPath = `/tmp/bridge-uploads/${ts}.jpg`;
      fs.writeFileSync(heicPath, body);
      try {
        execSync(`sips -s format jpeg "${heicPath}" --out "${jpgPath}"`, { timeout: 10000 });
        fs.unlinkSync(heicPath);
        res.json({ url: `/uploads/${ts}.jpg`, filename: `${ts}.jpg` });
      } catch {
        // Fallback: serve HEIC as-is
        res.json({ url: `/uploads/${ts}.heic`, filename: `${ts}.heic` });
      }
    } else {
      const ext = contentType.includes('png') ? 'png' : contentType.includes('gif') ? 'gif' : 'jpg';
      const filename = `${ts}.${ext}`;
      fs.writeFileSync(`/tmp/bridge-uploads/${filename}`, body);
      res.json({ url: `/uploads/${filename}`, filename });
    }
  });
});

// API: voice capture — receive audio, transcribe with whisper-cli, return transcript (#1782)
app.post('/api/voice', (req, res) => {
  const fs = require('fs');
  const { execSync } = require('child_process');
  const chunks: Buffer[] = [];
  req.on('data', (chunk: Buffer) => chunks.push(chunk));
  req.on('end', () => {
    const body = Buffer.concat(chunks);
    if (body.length === 0) { res.json({ error: 'empty audio' }); return; }

    const ts = Date.now();
    const uploadDir = '/tmp/bridge-audio-uploads';
    fs.mkdirSync(uploadDir, { recursive: true });
    const webmPath = `${uploadDir}/${ts}.webm`;
    const wavPath = `${uploadDir}/${ts}.wav`;
    fs.writeFileSync(webmPath, body);

    try {
      // Convert webm to wav (whisper-cli needs wav)
      execSync(`ffmpeg -i "${webmPath}" -ar 16000 -ac 1 -y "${wavPath}" 2>/dev/null`, { timeout: 15000 });

      // Transcribe with whisper-cli
      const model = '/opt/homebrew/share/whisper/ggml-base.en.bin';
      const output = execSync(
        `whisper-cli -m "${model}" -f "${wavPath}" --no-timestamps -t 4 2>/dev/null`,
        { encoding: 'utf-8', timeout: 30000 }
      ).trim();

      // Clean up wav (keep webm in audio-uploads for playback persistence)
      try { fs.unlinkSync(wavPath); } catch { /* ignored */ }

      const transcript = output.replace(/^\[.*?\]\s*/gm, '').trim();
      console.log(`[voice] transcribed ${body.length} bytes → "${transcript.substring(0, 100)}"`);

      res.json({ transcript, audioFile: `/audio-uploads/${ts}.webm` });
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      console.error('[voice] transcription failed:', msg);
      try { fs.unlinkSync(wavPath); } catch { /* ignored */ }
      res.json({ error: 'Transcription failed: ' + msg });
    }
  });
});

// Serve persisted audio files for playback (#1782)
app.use('/audio-uploads', express.static('/tmp/bridge-audio-uploads'));

// API: get commands-only view from observations JSONL
app.get('/api/commands/:role', (req, res) => {
  const role = req.params.role;
  if (!['wren', 'silas', 'kade'].includes(role)) return res.status(400).json({ error: 'invalid role' });

  const fs = require('fs');
  const obsFile = `${SCAN_DIR}/${role}-observations.jsonl`;
  const lines: string[] = [];

  try {
    const content = fs.readFileSync(obsFile, 'utf-8');
    lines.push(...formatObserverDigest(content));   // #3604 — extracted, unit-tested
  } catch { /* ignored */ }

  res.json({ text: lines.join('\n') || `No commands recorded for ${role}`, lines: lines.length });
});

// #3607 — spine parsing + TAIL-READ moved to src/spine-tail.ts (was a full
// readFileSync of the 117MB log per 3s poll; see that module + its tests).
const OBS_SKIP_TOKENS = ['nudge', 'chorus-log', 'role-state', 'cards', 'smoke-check'];

function parseObservation(line: string, seen: Set<string>): StreamLine | null {
  try {
    const obs = JSON.parse(line);
    const key = `${obs.ts}|${obs.digest}`;
    if (seen.has(key)) return null;
    seen.add(key);
    const digest = obs.digest || '';
    if (OBS_SKIP_TOKENS.some((t) => digest.includes(t))) return null;
    return { ts: obs.ts, role: obs.role, type: 'obs', text: digest, card: obs.card || null };
  } catch {
    return null;
  }
}

function readObservationsForRole(fs: typeof fs_node, role: string, out: StreamLine[]): void {
  const obsFile = `${SCAN_DIR}/${role}-observations.jsonl`;
  try {
    const obsLines = fs.readFileSync(obsFile, 'utf-8').trim().split('\n').filter(Boolean);
    const seen = new Set<string>();
    for (const line of obsLines.slice(-30)) {
      const entry = parseObservation(line, seen);
      if (entry) out.push(entry);
    }
  } catch { /* ignored */ }
}

function readRoleObservations(fs: typeof fs_node): StreamLine[] {
  const out: StreamLine[] = [];
  for (const role of ['wren', 'silas', 'kade']) readObservationsForRole(fs, role, out);
  return out;
}

function dedupeLines(lines: StreamLine[]): StreamLine[] {
  const out: StreamLine[] = [];
  for (const line of lines) {
    const dominated = out.some((prev) => {
      if (prev.role !== line.role) return false;
      if (prev.text === line.text) return true;
      const shorter = prev.text.length < line.text.length ? prev.text : line.text;
      const longer = prev.text.length >= line.text.length ? prev.text : line.text;
      return shorter.length > 10 && longer.includes(shorter);
    });
    if (!dominated) out.push(line);
  }
  return out;
}

app.get('/api/stream', (req, res) => {
  const fs = require('fs');
  const limit = parseInt(req.query.lines as string) || 60;
  const lines = [
    ...readSpineLines(fs, `${CHORUS_ROOT}/platform/logs/chorus.log`, limit),
    ...readRoleObservations(fs),
  ];
  lines.sort((a, b) => (a.ts || '').localeCompare(b.ts || ''));

  const formatted = dedupeLines(lines).slice(-limit).map((l) => {
    const ts = new Date(l.ts).toLocaleTimeString('en-US', {
      hour: 'numeric', minute: '2-digit', timeZone: 'America/New_York',
    });
    const card = l.card ? ` #${l.card}` : '';
    return { ts, role: l.role, text: l.text + card, type: l.type };
  });
  res.json(formatted);
});

const CHORUS_DOMAINS = new Set(['chorus', 'roles', 'borg']);
const FLOW_ENV_OPTS = {
  encoding: 'utf-8' as const, timeout: 15000,
  env: { ...process.env, PATH: '/Users/jeffbridwell/.nvm/versions/node/v20.11.1/bin:/opt/homebrew/bin:/usr/local/bin:/usr/sbin:/usr/bin:/bin:/sbin', HOME: '/Users/jeffbridwell' },
};

function extractSequenceTag(tags: string): string {
  return extractSequenceTags(tags)[0] || '';
}

// #2325: a card can carry multiple sequence: labels (e.g. werk + clearing).
// Return every one so the nav renders the card under each sub-sequence tile.
export function extractSequenceTags(tags: string): string[] {
  const matches = Array.from(tags.matchAll(/sequence:(\w+)/g)).map((m) => m[1]);
  if (matches.length > 0) return Array.from(new Set(matches));
  const parts = tags.split('|').map((s) => s.trim());
  const bareTag = parts.find((p) => p && !/^(Wren|Silas|Kade|Jeff|P[123]$)/.test(p) && !p.includes(':'));
  return bareTag ? [bareTag] : [];
}

interface ParsedCard {
  id: string;
  title: string;
  status: string;
  owner: string;
  domains: string[];
  type: string;
  priority: number;
  sequence: string | null;
  sequences: string[];
}

interface DomainCard extends ParsedCard {
  subDomain?: string;
}

interface DomainCardGroup {
  cards: DomainCard[];
  counts: {
    wip: number;
    next: number;
    blocked: number;
    activeCards: number;
    activeWorkflows: number;
    activeTotal: number;
  };
}

function parseCardRow(line: string, currentStatus: string): ParsedCard | null {
  const match = line.trim().match(/^(\d+)\s+(.+?)\s+\[([^\]]+)\]$/);
  if (!match) return null;
  const tags = match[3];
  const domains = (tags.match(/domain:(\w+)/g) || []).map((d) => d.replace('domain:', ''));
  return {
    id: match[1],
    title: match[2].trim(),
    status: currentStatus,
    owner: tags.match(/^(Wren|Silas|Kade)/i)?.[1].toLowerCase() || '',
    domains: domains.length > 0 ? domains : ['uncategorized'],
    type: tags.match(/type:(\w+)/)?.[1] || '',
    priority: parseInt(tags.match(/P([123])/)?.[1] || '9'),
    sequence: extractSequenceTag(tags),
    sequences: extractSequenceTags(tags),
  };
}

function parseCardList(output: string): ParsedCard[] {
  const cards: ParsedCard[] = [];
  let currentStatus = '';
  for (const line of output.split('\n')) {
    const statusMatch = line.match(/^(WIP|SWAT|Blocked|Harvesting|Next|Later|Done|Won't Do)\s*\(\d+\)/);
    if (statusMatch) { currentStatus = statusMatch[1]; continue; }
    if (currentStatus === 'Done' || currentStatus === "Won't Do") continue;
    const card = parseCardRow(line, currentStatus);
    if (card) cards.push(card);
  }
  return cards;
}

function loadActiveWorkflowCounts(fs: typeof fs_node, wfDir: string): Record<string, number> {
  const counts: Record<string, number> = {};
  try {
    const files = fs.readdirSync(wfDir).filter((f: string) => f.endsWith('.json'));
    for (const f of files) {
      try {
        const wf = JSON.parse(fs.readFileSync(`${wfDir}/${f}`, 'utf-8'));
        if (['completed', 'archived', 'cancelled'].includes(wf.status)) continue;
        if (wf.card) counts[String(wf.card)] = (counts[String(wf.card)] || 0) + 1;
      } catch { /* ignored */ }
    }
  } catch { /* ignored */ }
  return counts;
}

function groupByProduct(cards: ParsedCard[]): Record<string, DomainCardGroup> {
  const byDomain: Partial<Record<string, DomainCardGroup>> = {};
  for (const card of cards) {
    for (const domain of card.domains) {
      const topLevel = CHORUS_DOMAINS.has(domain) ? 'chorus' : 'gathering';
      let group = byDomain[topLevel];
      if (!group) {
        group = { cards: [], counts: { wip: 0, next: 0, blocked: 0, activeCards: 0, activeWorkflows: 0, activeTotal: 0 } };
        byDomain[topLevel] = group;
      }
      group.cards.push({ ...card, subDomain: CHORUS_DOMAINS.has(domain) ? undefined : domain });
    }
  }
  return byDomain as Record<string, DomainCardGroup>;
}

function computeDomainCounts(byDomain: Record<string, DomainCardGroup>, wfByCard: Record<string, number>): void {
  for (const data of Object.values(byDomain)) {
    const c = data.cards;
    data.counts.wip = c.filter((x: DomainCard) => x.status === 'WIP').length;
    data.counts.next = c.filter((x: DomainCard) => x.status === 'Next' || x.status === 'Later').length;
    data.counts.blocked = c.filter((x: DomainCard) => x.status === 'Blocked').length;
    data.counts.activeCards = c.filter((x: DomainCard) => x.status !== "Won't Do").length;
    let wfCount = 0;
    for (const card of c) wfCount += wfByCard[card.id] || 0;
    data.counts.activeWorkflows = wfCount;
    data.counts.activeTotal = data.counts.activeCards + wfCount;
  }
}

function computeFixFeatureRatio(cards: ParsedCard[]): { typeCounts: Record<string, number>; fixFeatureRatio: string } {
  const typeCounts: Record<string, number> = {};
  for (const card of cards) {
    const t = card.type || 'untyped';
    typeCounts[t] = (typeCounts[t] || 0) + 1;
  }
  const fixes = typeCounts['fix'] || 0;
  const features = (typeCounts['new'] || 0) + (typeCounts['enhance'] || 0);
  const ratio = features > 0 ? (fixes / features).toFixed(2) : fixes > 0 ? 'all-fix' : 'n/a';
  return { typeCounts, fixFeatureRatio: ratio };
}

app.get('/api/flow', (_req, res) => {
  const { execSync } = require('child_process');
  const fs = require('fs');
  try {
    const boardTs = `${CHORUS_ROOT}/platform/scripts/cards`;
    const output = execSync(`bash ${boardTs} list 2>/dev/null`, FLOW_ENV_OPTS).trim();
    const cards = parseCardList(output);
    const wfByCard = loadActiveWorkflowCounts(fs, `${CHORUS_ROOT}/platform/workflows/archive`);
    const byDomain = groupByProduct(cards);
    computeDomainCounts(byDomain, wfByCard);
    const { typeCounts, fixFeatureRatio } = computeFixFeatureRatio(cards);
    res.json({ domains: byDomain, totalCards: cards.length, typeCounts, fixFeatureRatio });
  } catch {
    res.json({ domains: {}, totalCards: 0 });
  }
});

// API: domain-detail proxy (#3667) — the browser must never fetch :3340
// directly (DEC-093: :3340 stays LAN-only, never exposed through the tunnel).
// CHORUS_API_URL is read per-request so tests can point it at a stub upstream.
app.get('/api/domain-detail/:name', async (req, res) => {
  const upstream = process.env.CHORUS_API_URL || 'http://localhost:3340';
  try {
    const r = await fetch(`${upstream}/api/chorus/domain/${encodeURIComponent(req.params.name)}`);
    res.status(r.status).json(await r.json());
  } catch {
    res.status(502).json({ error: 'upstream unreachable' });
  }
});

// API: card detail — fetch full card view for inline expansion
app.get('/api/card/:id', (_req, res) => {
  const { execSync } = require('child_process');
  const cardId = _req.params.id.replace(/\D/g, '');
  if (!cardId) { res.status(400).json({ error: 'Invalid card ID' }); return; }
  const envOpts = {
    encoding: 'utf-8' as const, timeout: 10000,
    env: { ...process.env, PATH: '/Users/jeffbridwell/.nvm/versions/node/v20.11.1/bin:/opt/homebrew/bin:/usr/local/bin:/usr/sbin:/usr/bin:/bin:/sbin', HOME: '/Users/jeffbridwell' }
  };
  try {
    const boardTs = `${CHORUS_ROOT}/platform/scripts/cards`;
    const output = execSync(`bash ${boardTs} view ${cardId} 2>/dev/null`, envOpts).trim();
    // Parse the output
    const titleMatch = output.match(/^#\d+\s+(.+)/);
    const statusMatch = output.match(/Status:\s+(\S+)/);
    const ownerMatch = output.match(/Owner:\s+(\S+)/);
    const descMatch = output.match(/Desc:\n([\s\S]*?)(?=\n {2}\w+:|$)/);
    const desc = descMatch ? descMatch[1].replace(/^ {4}/gm, '').trim() : '';
    // Extract AC items
    const acItems = (desc.match(/- \[[ x]\].+/g) || []).map((line: string) => ({
      done: line.includes('[x]'),
      text: line.replace(/^- \[[ x]\]\s*/, ''),
    }));
    // Extract domains/labels
    const domainsMatch = output.match(/Domains:\s+(.+)/);
    const domains = domainsMatch ? domainsMatch[1].trim() : '';
    // Extract comments (blast radius, domain radius, etc.)
    const commentsSection = output.match(/Comments \(\d+\):\n([\s\S]*?)(?=\n\*\*|$)/);
    const comments = commentsSection ? commentsSection[1].replace(/^ {4}/gm, '').trim() : '';
    // Extract blast/domain radius sections
    const blastRadius = output.match(/\*\*Blast Radius\*\*[^\n]*\n([\s\S]*?)(?=\n\*\*|_Generated|$)/);
    // eslint-disable-next-line security/detect-unsafe-regex -- bounded by lookahead (?=\n\*\*|_Generated|$), not backtracking-unbounded.
    const domainRadius = output.match(/\*\*Domain Radius\*\*[^\n]*(?:\n([\s\S]*?))?(?=\n\*\*|_Generated|$)/);
    res.json({
      id: cardId,
      title: titleMatch ? titleMatch[1].trim() : 'Unknown',
      status: statusMatch ? statusMatch[1] : 'Unknown',
      owner: ownerMatch ? ownerMatch[1] : 'Unknown',
      domains,
      description: desc,
      ac: acItems,
      blastRadius: blastRadius ? blastRadius[0].trim() : '',
      domainRadius: domainRadius ? domainRadius[0].trim() : '',
      comments,
    });
  } catch {
    res.status(404).json({ error: 'Card not found' });
  }
});

function getRoleTTY(role: string): string | null {
  const fs = require('fs');
  const pidFile = `${SCAN_DIR}/${role}.pid`;
  try {
    const pid = fs.readFileSync(pidFile, 'utf-8').trim();
    const { execSync } = require('child_process');
    const tty = execSync(`ps -p ${pid} -o tty=`, { encoding: 'utf-8' }).trim();
    if (tty && tty !== '??') return `/dev/${tty}`;
  } catch { /* ignored */ }
  return null;
}

// API: get raw terminal output for a role (for fold panel)
app.get('/api/session/:role', (req, res) => {
  const role = req.params.role;
  if (!['wren', 'silas', 'kade'].includes(role)) return res.status(400).json({ error: 'invalid role' });

  const tty = getRoleTTY(role);
  if (!tty) return res.json({ text: `No active session for ${role}`, lines: 0 });

  try {
    const { execSync } = require('child_process');
    const tailLines = parseInt(req.query.lines as string) || 80;
    const raw = execSync(`osascript -e '
tell application "Terminal"
  repeat with w in windows
    repeat with t in tabs of w
      if tty of t is "${tty}" then
        return history of t
      end if
    end repeat
  end repeat
  return ""
end tell'`, { encoding: 'utf-8', timeout: 5000, env: { ...process.env, PATH: '/usr/bin:/bin:/usr/sbin' } });

    // Return last N lines
    const allLines = raw.split('\n');
    const tail = allLines.slice(-tailLines).join('\n');
    res.json({ text: tail, lines: allLines.length });
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    res.json({ text: `Error reading terminal: ${msg}`, lines: 0 });
  }
});

// API: get current tile state
app.get('/api/tiles', (_req, res) => res.json(tilePoller.getTiles()));

// API: get recent messages (for page load)
app.get('/api/messages', (req, res) => res.json(messageRouter.getRecent(50, !!req.query.includeHidden)));

// #2895 proposal routes RETIRED in #2905 — Jeff direct: bouncer is just
// nudge-based now. Agent cards add composes a structured nudge text and
// refuses to file; agent decides whether to send via chorus_nudge_message;
// Jeff sees the nudge, decides, files the card himself if approved.
// No Bridge-side workflow surface needed.

// API: receive message from role (callback endpoint)
app.post('/api/message', (req, res) => {
  const { from, text } = req.body;
  if (!from || !text) return res.status(400).json({ error: 'from and text required' });
  messageRouter.ingest({ from, text, ts: new Date().toISOString(), type: req.body.type || 'role-response', level: req.body.level || '' });
  res.json({ ok: true });
});

// Socket.IO auth — remote connections need token (#1719).
// #3669 — was address-only, so a tunneled WS (cloudflared → 127.0.0.1) counted
// as local and skipped the token: an unauthenticated jeff-message command path
// over the public tunnel. Now uses the SAME classifier as the HTTP gate, so the
// cf-* tunnel headers on the WS upgrade demote a tunneled handshake to remote.
io.use((socket, next) => {
  void socketAuth(socket).then((ok) => next(ok ? undefined : new Error('Authentication required')))
    .catch(() => next(new Error('Authentication required')));
});

// #3669 — the WS gate must accept the SAME auth the HTTP gate does. Before this it
// only knew the bridge token, so a human logged in via CSS (clearing_session cookie,
// no bridge token) connected the page but the live socket was refused → "connecting…"
// forever + no data. Now: local → allow; else the CSS session cookie (verified WebID
// in the allow-set, fresh) → allow; else the bridge token as migration fallback.
async function socketAuth(socket: { handshake: { address?: string; headers: Record<string, unknown>; auth: { token?: string }; query: { token?: unknown } } }): Promise<boolean> {
  const ip = socket.handshake.address || '';
  if (isLocalConnection(socket.handshake.headers, ip)) return true;

  const cookieHeader = String(socket.handshake.headers.cookie || '');
  const sessMatch = cookieHeader.match(/clearing_session=([^;]+)/);
  if (sessMatch) {
    const sess = verifyCookie<{ webid?: string; iat?: number }>(decodeURIComponent(sessMatch[1]), SESSION_SECRET, 'session');
    const fresh = !!sess?.iat && Date.now() - sess.iat <= SESSION_MAX_AGE_MS;
    if (sess?.webid && fresh && (await isWebIdAllowed(sess.webid, Date.now()))) return true;
  }

  if (!REQUIRE_DPOP) {
    const token = socket.handshake.auth.token || String(socket.handshake.query.token || '');
    const cookieMatch = cookieHeader.match(/bridge_token=([^;]+)/);
    const cookieToken = cookieMatch ? decodeURIComponent(cookieMatch[1]) : '';
    if (token === BRIDGE_TOKEN || cookieToken === BRIDGE_TOKEN) return true;
  }
  return false;
}

// Socket.IO
io.on('connection', (socket) => {
  // Send initial state
  socket.emit('tiles', tilePoller.getTiles());
  socket.emit('messages', messageRouter.getRecent(50));

  // Client heartbeat — respond to ping with pong (#2036)
  socket.on('ping', () => { socket.emit('pong'); });

  // #2266: end chat session when last client disconnects so in-flight
  // Anthropic streams abort and the server stops spinning.
  socket.on('disconnect', () => {
    const remaining = io.sockets.sockets.size;
    if (remaining === 0 && clearingChat.getState().active) {
      clearingChat.endSession('client-disconnected');
    }
  });

  // Message from The Clearing UI — Jeff or guest (#1719; ack contract rewritten #3646).
  // The old inline body awaited every per-target hand-off (5s abort each, SEQUENTIAL)
  // before acking, while the UI timed out at 3s — the works-once bug: boundary-timed
  // sends showed "Failed" + restored the text for messages that had actually landed.
  // Now: ingest → ack ok → parallel hand-offs, each outcome pushed to the client as a
  // 'delivery-status' event the UI renders visibly.
  socket.on('jeff-message', (data: { text: string; from?: string }, ack?: (result: { ok: boolean; error?: string }) => void) => {
    const senderName = resolveJeffMessageSender(socket.handshake.headers.cookie || '', data.from);
    const cleanText = (data.text || '').replace(/@(wren|silas|kade)\s*/gi, '').trim();
    const finalText = cleanText.replace(/\[img:(\/uploads\/[^\]]+)\]/g, `[img:http://localhost:${PORT}$1]`);
    const safeMsg = finalText.replace(/"/g, '\\"');
    void processJeffInput(
      {
        ingest: (m) => messageRouter.ingest(m),
        deliver: (target) => deliverJeffMessageToTarget(target, safeMsg, cleanText),
        targetsOf: pickJeffMessageTargets,
        now: () => new Date().toISOString(),
        onDeliveryStatus: (status) => socket.emit('delivery-status', status),
      },
      { text: data.text || '', from: senderName },
      ack,
    );
  });
});

// Broadcast tile updates every 5 seconds
// #3604 — unref: background broadcast timer must not keep Node's loop alive (jest exit clean).
setInterval(() => {
  tilePoller.poll();
  io.emit('tiles', tilePoller.getTiles());
}, 5000).unref();

// Broadcast new messages as they arrive
messageRouter.on('message', (msg) => {
  io.emit('message', msg);
});

// Start tailing chorus log for spine events (demos, accepts, blocks)
tailer.start();

// Push board events to all connected clients (#1681)
tailer.on('board-event', (event) => {
  io.emit('board-event', event);
  // #2467: clearCard() retired — card is no longer in role-state.
  // Tile renderer reads cards directly from the board (boardCache),
  // which already reflects card.accepted (card moves out of WIP →
  // automatically drops from tile on next poll). No state mutation needed.
  if (event.type === 'role.state.changed' || event.type === 'card.accepted' || event.type === 'card.pulled') {
    tilePoller.poll();
    io.emit('tiles', tilePoller.getTiles());
  }
});

// Start tailing session JSONL files for conversation mirroring (#1665)
const sessionTailer = new SessionTailer(messageRouter);
sessionTailer.start();

// Graceful restart: POST /api/restart shuts down cleanly so new process can bind
app.post('/api/restart', (_req, res) => {
  res.json({ ok: true, message: 'shutting down' });
  setTimeout(() => {
    tailer.stop();
    sessionTailer.stop();
    void io.close();
    // Force exit if graceful close hangs; unref + clear-on-success so the
    // fallback doesn't fire after tests mock-then-restore process.exit.
    const forceExit = setTimeout(() => process.exit(0), 3000);
    forceExit.unref();
    server.close(() => { clearTimeout(forceExit); process.exit(0); });
  }, 500);
});

// Debug: socket connection count
app.get('/api/debug', (_req, res) => {
  const sockets = io.sockets.sockets;
  res.json({
    connectedClients: sockets.size,
    messageCount: messageRouter.getRecent(999, true).length,
    visibleCount: messageRouter.getRecent(999).length,
    sessionCount: sessionTailer.getSessionCount(),
  });
});

// --- Chat mode (absorbed from clearing/src/server.ts) ---
const clearingChat = new ClearingChat(io);

// Chat API endpoints
app.post('/api/chat/start', (req, res) => {
  const context = req.body?.context || '';
  const result = clearingChat.startSession(context);
  res.json(result);
});

app.post('/api/chat/end', (_req, res) => {
  clearingChat.endSession('user-initiated');
  res.json({ ok: true });
});

app.get('/api/chat/state', (_req, res) => {
  res.json(clearingChat.getState());
});

app.get('/api/chat/messages', (req, res) => {
  const since = parseInt(req.query.since as string) || 0;
  res.json({ messages: clearingChat.getMessages(since) });
});

app.post('/api/chat/message', (req, res) => {
  const { text, activeRoles } = req.body;
  if (!text) return res.status(400).json({ error: 'text required' });
  // Fire-and-forget — responses stream via Socket.IO
  void clearingChat.handleMessage(text, activeRoles);
  res.json({ ok: true });
});

// Socket.IO chat events
io.on('connection', (socket) => {
  // Chat mode events
  socket.on('chat:message', async (data: { text: string; activeRoles?: string[] }) => {
    await clearingChat.handleMessage(data.text, data.activeRoles);
  });

  socket.on('chat:start', (data?: { context?: string }) => {
    clearingChat.startSession(data?.context);
    socket.emit('chat:state', clearingChat.getState());
  });

  socket.on('chat:end', () => {
    clearingChat.endSession('user-initiated');
  });
});

// Export for tests (#2167) — tests import `app` and `server` and spin up
// on an ephemeral port themselves, so importing the module doesn't bind :3470.
export { app, server, io, tilePoller, messageRouter, clearingChat, tailer, sessionTailer };

// Only bind when run as the main module. Under jest (require.main !== module)
// tests control the listener lifecycle.
if (require.main === module) {
  // #3390 — :3470 INTENTIONALLY serves the LAN: #3366 made
  // http://jeffs-mac-mini-m1-3.local:3470 the IP-proof phone-over-wifi URL
  // (a direct LAN hit, not via the tunnel). Binding loopback would kill it.
  // The real exposure here is that LAN access is UNAUTHENTICATED (isLocal()
  // treats 192.168.86.x as local, no token) — that's an auth-model question,
  // not a bind question, escalated to Jeff (LAN-exception in ADR-042 §8).
  server.listen(PORT, () => {
    console.log(`The Clearing listening on http://localhost:${PORT} (also LAN :${PORT})`);
  });

  // HTTPS server for LAN mic access — getUserMedia requires secure context (#1782)
  // Shares the same express app — all routes and Socket.IO handlers work on both.
  const HTTPS_PORT = parseInt(process.env.CLEARING_HTTPS_PORT || '3471');
  const certDir = `${require('os').homedir()}/.chorus/certs`;
  try {
    const fs = require('fs');
    const key = fs.readFileSync(`${certDir}/clearing-key.pem`);
    const cert = fs.readFileSync(`${certDir}/clearing-cert.pem`);
    const httpsServer = createHttpsServer({ key, cert }, app);
    // Attach Socket.IO to HTTPS server too — same handlers via shared app
    io.attach(httpsServer);
    httpsServer.listen(HTTPS_PORT, () => {
      const httpsHost = bonjourHost(LOCAL_HOST_NAME) ?? lanAddress() ?? 'localhost';
      console.log(`The Clearing HTTPS listening on https://${httpsHost}:${HTTPS_PORT} (mic-enabled)`);
    });
  } catch (err: unknown) {
    console.log(`[clearing] HTTPS not available: ${err instanceof Error ? err.message : String(err)} — mic requires tunnel URL`);
  }
}

/** Parse @mention to determine target role. Default: wren */
function parseTarget(text: string): string {
  const match = text.match(/^@(wren|silas|kade)\b/i);
  if (match) return match[1].toLowerCase();
  return 'wren'; // No @ = Wren gets it
}

function resolveJeffMessageSender(cookieHeader: string, fromField?: string): string {
  const nameMatch = cookieHeader.match(/bridge_name=([^;]+)/);
  const guestName = nameMatch ? decodeURIComponent(nameMatch[1]) : '';
  return fromField || guestName || 'jeff';
}

function pickJeffMessageTargets(text: string): string[] {
  const mentions = text.match(/@(wren|silas|kade)/gi) || [];
  if (mentions.length === 0) return [parseTarget(text)];
  return [...new Set(mentions.map((m: string) => m.slice(1).toLowerCase()))];
}

// Returns null on success, error string on failure (#2036).
// #3343: Jeff's Clearing input → POST pulse /api/jeff-input. The pulse
// delivery worker owns the keystroke (same machinery as nudges: per-role
// serialization + retry-with-backoff — the worker retries through the
// busy-session moments that made the old one-shot direct inject fail), but
// the content travels RAW: no [nudge from] framing, so Jeff's input keeps
// its authority at the receiving session (approvals, /card).
//
// History: #2575 shelled chorus-inject directly here; #2804 retired direct
// invocation (not-canonical-caller) which broke this path silently —
// jeff.input.failed ×29/7d, every Clearing delivery refused. The pulse POST
// is the supported surface.
//
// The HTTP ack confirms HAND-OFF to the worker (enqueue), not the keystroke
// itself — the worker's jeff.input.{surfaced,surface.failed} spine events
// carry the terminal verdict. Side effect: emit jeff.input.{delivered,failed}
// fire-and-forget for audit continuity with the pre-#3343 event names.
const PULSE_URL = process.env.PULSE_URL || 'http://localhost:3475';

async function deliverJeffMessageToTarget(target: string, safeMsg: string, cleanText: string): Promise<string | null> {
  const { execFile } = require('child_process');
  console.log(`[clearing] delivering to ${target}: ${cleanText.substring(0, 60)}`);
  const ctrl = new AbortController();
  const timeoutId = setTimeout(() => ctrl.abort(), 5000);
  try {
    const resp = await fetch(`${PULSE_URL}/api/jeff-input`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Chorus-Clearing-Caller': '1' },
      body: JSON.stringify({ to: target, content: safeMsg }),
      signal: ctrl.signal,
    });
    if (!resp.ok) {
      const errBody = await resp.text().catch(() => '');
      throw new Error(`pulse ${resp.status}: ${errBody.slice(0, 160)}`);
    }
    console.log(`[clearing] HANDED OFF to pulse worker for ${target}`);
    // Fire-and-forget audit event — don't block on logging.
    execFile(`${CHORUS_ROOT}/platform/scripts/chorus-log`,
      ['jeff.input.delivered', 'bridge', `to=${target}`, `chars=${safeMsg.length}`],
      () => { /* fire-and-forget */ });
    return null;
  } catch (err) {
    const reason = (err instanceof Error ? err.message : String(err)).split('\n')[0].trim();
    console.error(`[clearing] jeff-input hand-off failed for ${target}: ${reason}`);
    execFile(`${CHORUS_ROOT}/platform/scripts/chorus-log`,
      ['jeff.input.failed', 'bridge', `to=${target}`, `chars=${safeMsg.length}`, `reason=${reason}`],
      () => { /* fire-and-forget */ });
    return reason;
  } finally {
    clearTimeout(timeoutId); // both paths — a failed fetch must not leave the abort timer dangling
  }
}
