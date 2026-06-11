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
import { SessionTailer } from './session-tailer';
import { ClearingChat } from './chat';

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
console.log(`[clearing] remote URL: http://192.168.86.36:${PORT}?token=${BRIDGE_TOKEN}`);

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

// Auth middleware — local/LAN requests pass, remote requests need token (#1719)
function isLocal(req: express.Request): boolean {
  // Cloudflare tunnel proxies from localhost — check CF headers to detect tunneled requests
  if (req.headers['cf-connecting-ip'] || req.headers['cf-ray']) return false;
  const ip = req.ip || req.socket.remoteAddress || '';
  // Localhost
  if (ip === '127.0.0.1' || ip === '::1' || ip === '::ffff:127.0.0.1') return true;
  // LAN — 192.168.86.x (Jeff's home network)
  if (ip.startsWith('192.168.86.') || ip.startsWith('::ffff:192.168.86.')) return true;
  return false;
}

// Body parsers BEFORE auth — login form needs req.body parsed (#1782)
app.use(express.json());
app.use(express.urlencoded({ extended: false }));

const LOCAL_ONLY_PATHS = ['/health', '/metrics', '/api/debug'];
const ADMIN_PATH_PREFIXES = ['/api/stream', '/api/session/', '/api/commands/', '/api/flow', '/api/restart'];
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

function handleLocalOnlyGate(req: Request, res: Response): boolean {
  if (LOCAL_ONLY_PATHS.includes(req.path)) {
    if (isLocal(req)) return false;
    res.status(403).json({ error: 'forbidden' });
    return true;
  }
  if (ADMIN_PATH_PREFIXES.some((p) => req.path.startsWith(p))) {
    if (isLocal(req)) return false;
    res.status(403).json({ error: 'forbidden' });
    return true;
  }
  return false;
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

app.use((req, res, next) => {
  if (req.path === '/bridge-og.jpg') return next();
  if (handleLocalOnlyGate(req, res)) return;
  if (isLocal(req)) return next();

  const token = extractToken(req);
  // eslint-disable-next-line security/detect-possible-timing-attacks -- BRIDGE_TOKEN is a long random value; this is a tunnel auth gate, not a high-security comparison.
  if (token === BRIDGE_TOKEN) return handleAuthenticated(req, res, next);

  if (req.path === '/login' && req.method === 'POST') return handleLoginPost(req, res);
  res.status(401).send(loginPage());
});

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
setInterval(() => {
  try {
    const msgs = messageRouter.getRecent(200, true);
    fs_sync.writeFileSync(MSG_FILE, JSON.stringify(msgs));
  } catch { /* ignored */ }
}, 10000);

// Ensure upload directory survives /tmp cleanup across reboots
import fs_node from 'fs';
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
  const obsFile = `/tmp/claude-team-scan/${role}-observations.jsonl`;
  const lines: string[] = [];

  try {
    const content = fs.readFileSync(obsFile, 'utf-8');
    const obsLines = content.trim().split('\n').filter(Boolean);
    const seen = new Set<string>();

    for (const line of obsLines.slice(-60)) {
      try {
        const obs = JSON.parse(line);
        // Deduplicate (observer double-write bug)
        const key = `${obs.ts}|${obs.digest}`;
        if (seen.has(key)) continue;
        seen.add(key);

        const ts = new Date(obs.ts).toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit', timeZone: 'America/New_York' });
        const card = obs.card ? ` #${obs.card}` : '';
        // Shorten digest to just the action
        let short = obs.digest || '';
        // Skip nudge commands from commands view — role-to-role coordination (#1675)
        if (short.startsWith('nudging:')) continue;
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
        lines.push(`${ts}  ${short}${card}`);
      } catch { /* ignored */ }
    }
  } catch { /* ignored */ }

  res.json({ text: lines.join('\n') || `No commands recorded for ${role}`, lines: lines.length });
});

type StreamLine = { ts: string; role: string; type: string; text: string; card?: string | null };

const TURN_SKIP_PREFIXES = ['[nudge from', '[feedback]', '[response]', '[reply]', '[ack]', '[direction]', '[correction]'];
const TURN_SKIP_CONTAINS = ['<command-', 'Base directory for this skill', '[Request interrupted', '[Image:', '/var/folders'];
const OBS_SKIP_TOKENS = ['nudge', 'chorus-log', 'role-state', 'cards', 'smoke-check'];

function formatToolDisplay(summary: string, action: string): string | null {
  if (action === 'Read' || action === 'Glob' || action === 'Grep') return null;
  if (action === 'Bash') return summary.replace(/^Bash: /, '→ ');
  if (action === 'Edit') return summary.replace(/^Edit: /, '✏️ ');
  if (action === 'Write') return summary.replace(/^Write: /, '📝 ');
  return summary;
}

interface LogEntry {
  timestamp?: string;
  role?: string;
  event?: string;
  summary?: string;
  action?: string;
  tool_count?: string | number;
  from?: string;
  target?: string;
}

function parseTurnLine(entry: LogEntry, role: string): StreamLine | null {
  let summary = (entry.summary ?? '').substring(0, 200);
  if (TURN_SKIP_PREFIXES.some((p) => summary.startsWith(p))) return null;
  if (TURN_SKIP_CONTAINS.some((p) => summary.includes(p))) return null;
  summary = summary.replace(/\s*\|\s*tools:\s*[^|]*\|\s*[\d.]+s\s*$/, '').trim();
  if (!summary) return null;
  const toolCount = parseInt(String(entry.tool_count ?? '0'), 10);
  const isJeffInput = toolCount === 0;
  if (isJeffInput && summary.length < 5) return null;
  return {
    ts: entry.timestamp ?? '',
    role: isJeffInput ? 'jeff' : role,
    type: 'turn',
    text: isJeffInput ? `→${role}: ${summary}` : summary,
  };
}

function parseToolEntry(entry: LogEntry, role: string): StreamLine | null {
  const display = formatToolDisplay((entry.summary ?? '').substring(0, 120), entry.action ?? '');
  if (display === null) return null;
  return { ts: entry.timestamp ?? '', role, type: 'tool', text: display };
}

// #2435 — canonical event is nudge.emitted. chorus-log packs the first kv as
// the JSON field; for nudge.emitted that's "from":"<sender>,to=...,content=<preview>".
function parseNudgeEntry(entry: LogEntry, role: string): StreamLine | null {
  const packed = entry.from ?? entry.target ?? '';
  const content = packed.match(/content=(.+)/)?.[1] || '';
  if (!content.includes('[gemba]')) return null;
  return { ts: entry.timestamp ?? '', role, type: 'gemba', text: content.substring(0, 200) };
}

function parseLogEntry(entry: LogEntry): StreamLine | null {
  const role = entry.role ?? '';
  if (!role || !['wren', 'silas', 'kade'].includes(role)) return null;
  const event = entry.event ?? '';
  if (event === 'session_tool') return parseToolEntry(entry, role);
  if (event === 'session_turn') return parseTurnLine(entry, role);
  if (event === 'nudge.emitted') return parseNudgeEntry(entry, role);
  return null;
}

function readSpineLines(fs: typeof fs_node, logFile: string, limit: number): StreamLine[] {
  const out: StreamLine[] = [];
  try {
    const logLines = fs.readFileSync(logFile, 'utf-8').trim().split('\n').filter(Boolean);
    let count = 0;
    for (let i = logLines.length - 1; i >= 0 && count < limit * 2; i--) {
      try {
        const line = parseLogEntry(JSON.parse(logLines[i]));
        if (line) { out.push(line); count++; }
      } catch { /* ignored */ }
    }
  } catch { /* ignored */ }
  return out;
}

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
  const obsFile = `/tmp/claude-team-scan/${role}-observations.jsonl`;
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
  const pidFile = `/tmp/claude-team-scan/${role}.pid`;
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

// Socket.IO auth — remote connections need token (#1719)
io.use((socket, next) => {
  const ip = socket.handshake.address || '';
  const isLocalSocket = ip === '127.0.0.1' || ip === '::1' || ip === '::ffff:127.0.0.1'
    || ip.startsWith('192.168.86.') || ip.startsWith('::ffff:192.168.86.');
  if (isLocalSocket) return next();

  const token = socket.handshake.auth.token || socket.handshake.query.token;
  // Also check cookies from handshake headers
  const cookieHeader = socket.handshake.headers.cookie || '';
  const cookieMatch = cookieHeader.match(/bridge_token=([^;]+)/);
  const cookieToken = cookieMatch ? decodeURIComponent(cookieMatch[1]) : '';

  if (token === BRIDGE_TOKEN || cookieToken === BRIDGE_TOKEN) return next();
  next(new Error('Authentication required'));
});

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

  // Message from The Clearing UI — Jeff or guest (#1719, #1802 reverted to working state)
  socket.on('jeff-message', async (data: { text: string; from?: string }, ack?: (result: { ok: boolean; error?: string }) => void) => {
    const { text } = data;
    if (!text.trim()) { ack?.({ ok: false, error: 'empty' }); return; }

    const senderName = resolveJeffMessageSender(socket.handshake.headers.cookie || '', data.from);
    messageRouter.ingest({ from: senderName, text: text.trim(), ts: new Date().toISOString(), type: 'jeff-input' });

    const targets = pickJeffMessageTargets(text);
    const cleanText = text.replace(/@(wren|silas|kade)\s*/gi, '').trim();
    const finalText = cleanText.replace(/\[img:(\/uploads\/[^\]]+)\]/g, `[img:http://localhost:${PORT}$1]`);
    const safeMsg = finalText.replace(/"/g, '\\"');

    for (const target of targets) {
      const err = await deliverJeffMessageToTarget(target, safeMsg, cleanText);
      if (err) { ack?.({ ok: false, error: err }); return; }
    }
    ack?.({ ok: true });
  });
});

// Broadcast tile updates every 5 seconds
setInterval(() => {
  tilePoller.poll();
  io.emit('tiles', tilePoller.getTiles());
}, 5000);

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
export { app, server, io, tilePoller, messageRouter, clearingChat };

// Only bind when run as the main module. Under jest (require.main !== module)
// tests control the listener lifecycle.
if (require.main === module) {
  server.listen(PORT, () => {
    console.log(`The Clearing listening on http://localhost:${PORT}`);
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
      console.log(`The Clearing HTTPS listening on https://192.168.86.36:${HTTPS_PORT} (mic-enabled)`);
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
  try {
    const ctrl = new AbortController();
    const timeoutId = setTimeout(() => ctrl.abort(), 5000);
    const resp = await fetch(`${PULSE_URL}/api/jeff-input`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Chorus-Clearing-Caller': '1' },
      body: JSON.stringify({ to: target, content: safeMsg }),
      signal: ctrl.signal,
    });
    clearTimeout(timeoutId);
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
  }
}
