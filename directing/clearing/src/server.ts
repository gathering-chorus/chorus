import express from 'express';
import { createServer } from 'http';
import { Server } from 'socket.io';
import path from 'path';
import { TilePoller } from './tiles';
import { MessageRouter } from './router';
import { ChorusLogTailer } from './tailer';
import { SessionTailer } from './session-tailer';
import { ClearingChat } from './chat';

const PORT = parseInt(process.env.COMMAND_CHANNEL_PORT || '3470');
const NUDGE_SCRIPT = '/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/nudge';

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
const io = new Server(server, { cors: { origin: '*' } });

// Cookie parser (minimal — just need bridge_token) — must be before auth
app.use((req: any, _res, next) => {
  if (!req.cookies) {
    req.cookies = {};
    const cookieHeader = req.headers.cookie || '';
    for (const pair of cookieHeader.split(';')) {
      const [key, val] = pair.trim().split('=');
      if (key && val) req.cookies[key] = decodeURIComponent(val);
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

app.use((req, res, next) => {
  // OG image always open (link previews need it without auth)
  if (req.path === '/bridge-og.jpg') return next();

  // Health/metrics: local only, blocked from tunnel (#1756)
  if (req.path === '/health' || req.path === '/metrics' || req.path === '/api/debug') {
    if (isLocal(req)) return next();
    return res.status(403).json({ error: 'forbidden' });
  }

  // Admin APIs: local only (#1756)
  const adminPaths = ['/api/stream', '/api/session/', '/api/commands/', '/api/flow', '/api/restart'];
  if (adminPaths.some(p => req.path.startsWith(p))) {
    if (isLocal(req)) return next();
    return res.status(403).json({ error: 'forbidden' });
  }

  // Local requests skip auth
  if (isLocal(req)) return next();

  // Check token: query param, cookie, or Authorization header
  const token = (req.query.token as string)
    || req.cookies?.bridge_token
    || req.headers.authorization?.replace('Bearer ', '');

  if (token === BRIDGE_TOKEN) {
    // Set cookie so Jeff doesn't need the token in every URL
    if (req.query.token && !req.cookies?.bridge_token) {
      res.cookie('bridge_token', BRIDGE_TOKEN, {
        maxAge: 365 * 24 * 60 * 60 * 1000, // 1 year
        httpOnly: true,
        sameSite: 'lax',
      });
    }

    // Guest name gate — remote users must identify themselves (#1719)
    const hasName = req.cookies?.bridge_name;
    if (!hasName && req.path !== '/set-name' && req.path !== '/bridge-og.jpg') {
      // Jeff on LAN doesn't hit this (isLocal returned above)
      // Show name prompt
      if (req.path === '/' || req.path === '/index.html') {
        return res.send(namePage());
      }
    }

    return next();
  }

  // No valid token — show login page
  if (req.path === '/login' && req.method === 'POST') {
    const { token: submittedToken } = req.body || {};
    if (submittedToken === BRIDGE_TOKEN) {
      res.cookie('bridge_token', BRIDGE_TOKEN, {
        maxAge: 365 * 24 * 60 * 60 * 1000,
        httpOnly: true,
        sameSite: 'lax',
      });
      return res.redirect('/');
    }
    return res.status(401).send(loginPage('Wrong token'));
  }

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
app.get('/', (req: any, res) => {
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
app.use(express.json());
app.use(express.urlencoded({ extended: false }));

// Guest logout (#1719)
app.get('/logout', (_req, res) => {
  res.clearCookie('bridge_token');
  res.clearCookie('bridge_name');
  res.send(`<!DOCTYPE html><html><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<style>body{background:#0d1117;color:#e6edf3;font-family:system-ui;display:flex;align-items:center;justify-content:center;height:100vh;margin:0}
h2{font-size:1.2rem}</style></head><body><h2>Signed out. Close this tab.</h2></body></html>`);
});

// Guest name registration (#1719)
app.post('/set-name', (req: any, res) => {
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
} catch {}

// Save messages every 10 seconds
setInterval(() => {
  try {
    const msgs = messageRouter.getRecent(200, true);
    fs_sync.writeFileSync(MSG_FILE, JSON.stringify(msgs));
  } catch {}
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
      } catch (err) {
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
      } catch {}
    }
  } catch {}

  res.json({ text: lines.join('\n') || `No commands recorded for ${role}`, lines: lines.length });
});

// API: unified activity stream — all roles interleaved by time
app.get('/api/stream', (req, res) => {
  const fs = require('fs');
  const logFile = '/Users/jeffbridwell/CascadeProjects/chorus/platform/logs/chorus.log';
  const limit = parseInt(req.query.lines as string) || 60;

  const lines: any[] = [];
  try {
    const content = fs.readFileSync(logFile, 'utf-8');
    const logLines = content.trim().split('\n').filter(Boolean);

    // Scan from end
    let count = 0;
    for (let i = logLines.length - 1; i >= 0 && count < limit * 2; i--) {
      try {
        const entry = JSON.parse(logLines[i]);
        const event = entry.event || '';
        const role = entry.role || '';
        if (!role || !['wren', 'silas', 'kade'].includes(role)) continue;

        if (event === 'session_tool') {
          const summary = (entry.summary || '').substring(0, 120);
          const action = entry.action || '';
          // Compact: just the tool + short description
          let display = summary;
          if (action === 'Bash') display = summary.replace(/^Bash: /, '→ ');
          else if (action === 'Edit') display = summary.replace(/^Edit: /, '✏️ ');
          else if (action === 'Write') display = summary.replace(/^Write: /, '📝 ');
          else if (action === 'Read') continue; // skip reads
          else if (action === 'Glob' || action === 'Grep') continue; // skip searches

          lines.push({
            ts: entry.timestamp || '',
            role,
            type: 'tool',
            text: display,
          });
          count++;
        } else if (event === 'session_turn') {
          let summary = (entry.summary || '').substring(0, 200);
          // Skip nudge relays and system noise
          if (summary.startsWith('[nudge from') || summary.startsWith('[feedback]') || summary.startsWith('[response]')) continue;
          if (summary.startsWith('[reply]') || summary.startsWith('[ack]') || summary.startsWith('[direction]') || summary.startsWith('[correction]')) continue;
          if (summary.includes('<command-') || summary.includes('Base directory for this skill')) continue;
          if (summary.includes('[Request interrupted')) continue;
          if (summary.includes('[Image:')) continue;
          if (summary.includes('/var/folders')) continue;
          // Strip metadata suffix
          summary = summary.replace(/\s*\|\s*tools:\s*[^|]*\|\s*[\d.]+s\s*$/, '').trim();
          if (!summary) continue;
          // Detect Jeff's input vs role output (#1706)
          // tool_count=0 means Jeff typed it; roles always use tools when responding
          const toolCount = parseInt(entry.tool_count || '0', 10);
          const isJeffInput = toolCount === 0;
          if (isJeffInput && summary.length < 5) continue;
          lines.push({
            ts: entry.timestamp || '',
            role: isJeffInput ? 'jeff' : role,
            type: 'turn',
            text: isJeffInput ? `→${role}: ${summary}` : summary,
          });
          count++;
        } else if (event === 'role.nudge.sent') {
          // Capture gemba observations sent to jeff
          const target = (entry.target || '').split(',')[0] || '';
          const content = (entry.target || '').match(/content=(.+)/)?.[1] || '';
          if (content.includes('[gemba]')) {
            lines.push({
              ts: entry.timestamp || '',
              role,
              type: 'gemba',
              text: content.substring(0, 200),
            });
            count++;
          }
        }
      } catch {}
    }
  } catch {}

  // Also include observations for richer tool data
  for (const role of ['wren', 'silas', 'kade']) {
    const obsFile = `/tmp/claude-team-scan/${role}-observations.jsonl`;
    try {
      const content = fs.readFileSync(obsFile, 'utf-8');
      const obsLines = content.trim().split('\n').filter(Boolean);
      const seen = new Set<string>();
      for (const line of obsLines.slice(-30)) {
        try {
          const obs = JSON.parse(line);
          const key = `${obs.ts}|${obs.digest}`;
          if (seen.has(key)) continue;
          seen.add(key);
          const digest = obs.digest || '';
          // Filter nudge traffic and system plumbing from stream
          if (digest.includes('nudge') || digest.includes('nudge ') || digest.includes('chorus-log') ||
              digest.includes('role-state') || digest.includes('cards') || digest.includes('smoke-check')) continue;
          lines.push({
            ts: obs.ts,
            role: obs.role,
            type: 'obs',
            text: digest,
            card: obs.card || null,
          });
        } catch {}
      }
    } catch {}
  }

  // Sort by timestamp, newest last
  lines.sort((a: any, b: any) => (a.ts || '').localeCompare(b.ts || ''));

  // Deduplicate: same role + similar text within 2 seconds (#1706)
  const deduped: typeof lines = [];
  for (const line of lines) {
    const dominated = deduped.some(prev => {
      if (prev.role !== line.role) return false;
      // Same text = exact dup
      if (prev.text === line.text) return true;
      // One contains the other (observation vs turn)
      const shorter = prev.text.length < line.text.length ? prev.text : line.text;
      const longer = prev.text.length >= line.text.length ? prev.text : line.text;
      if (shorter.length > 10 && longer.includes(shorter)) return true;
      return false;
    });
    if (!dominated) deduped.push(line);
  }

  // Format for display
  const formatted = deduped.slice(-limit).map((l: any) => {
    const ts = new Date(l.ts).toLocaleTimeString('en-US', {
      hour: 'numeric', minute: '2-digit', timeZone: 'America/New_York'
    });
    const card = l.card ? ` #${l.card}` : '';
    return { ts, role: l.role, text: l.text + card, type: l.type };
  });

  res.json(formatted);
});

// API: board flow state — grouped by domain, matching /flow page sort
app.get('/api/flow', (_req, res) => {
  const { execSync } = require('child_process');
  const fs = require('fs');
  const glob = require('path');
  const envOpts = {
    encoding: 'utf-8' as const, timeout: 15000,
    env: { ...process.env, PATH: '/Users/jeffbridwell/.nvm/versions/node/v20.11.1/bin:/opt/homebrew/bin:/usr/local/bin:/usr/sbin:/usr/bin:/bin:/sbin', HOME: '/Users/jeffbridwell' }
  };
  try {
    const boardTs = '/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards';
    const output = execSync(`bash ${boardTs} list 2>/dev/null`, envOpts).trim();

    // Parse all active cards
    const cards: any[] = [];
    let currentStatus = '';
    for (const line of output.split('\n')) {
      const statusMatch = line.match(/^(WIP|Blocked|Harvesting|Next|Later|Done|Won't Do)\s*\(\d+\)/);
      if (statusMatch) { currentStatus = statusMatch[1]; continue; }
      if (currentStatus === 'Done' || currentStatus === "Won't Do") continue;
      const cardMatch = line.trim().match(/^(\d+)\s+(.+?)\s+\[([^\]]+)\]$/);
      if (cardMatch) {
        const tags = cardMatch[3];
        const ownerMatch = tags.match(/^(Wren|Silas|Kade)/i);
        const domains = (tags.match(/domain:(\w+)/g) || []).map((d: string) => d.replace('domain:', ''));
        const typeMatch = tags.match(/type:(\w+)/);
        const priorityMatch = tags.match(/P([123])/);
        const sequenceMatch = tags.match(/sequence:(\w+)/);
        // Bare tags (not prefixed with chunk:/domain:/type:/sequence:/P[123]/role) are sequence labels
        let sequence = sequenceMatch ? sequenceMatch[1] : '';
        if (!sequence) {
          const parts = tags.split('|').map((s: string) => s.trim());
          const bareTag = parts.find((p: string) => p && !/^(Wren|Silas|Kade|Jeff|P[123]$)/.test(p) && !p.includes(':'));
          if (bareTag) sequence = bareTag;
        }
        cards.push({
          id: cardMatch[1],
          title: cardMatch[2].trim(),
          status: currentStatus,
          owner: ownerMatch ? ownerMatch[1].toLowerCase() : '',
          domains: domains.length > 0 ? domains : ['uncategorized'],
          type: typeMatch ? typeMatch[1] : '',
          priority: priorityMatch ? parseInt(priorityMatch[1]) : 9,
          sequence,
        });
      }
    }

    // Count active workflows per card
    const wfDir = '/Users/jeffbridwell/CascadeProjects/chorus/platform/workflows/archive';
    const wfByCard: Record<string, number> = {};
    try {
      const files = fs.readdirSync(wfDir).filter((f: string) => f.endsWith('.json'));
      for (const f of files) {
        try {
          const wf = JSON.parse(fs.readFileSync(`${wfDir}/${f}`, 'utf-8'));
          if (wf.status === 'completed' || wf.status === 'archived' || wf.status === 'cancelled') continue;
          if (wf.card) wfByCard[String(wf.card)] = (wfByCard[String(wf.card)] || 0) + 1;
        } catch {}
      }
    } catch {}

    // Group by domain with counts
    const byDomain: Record<string, any> = {};
    for (const card of cards) {
      for (const domain of card.domains) {
        if (!byDomain[domain]) byDomain[domain] = { cards: [], counts: { wip: 0, next: 0, blocked: 0, activeCards: 0, activeWorkflows: 0, activeTotal: 0 } };
        byDomain[domain].cards.push(card);
      }
    }

    // Compute counts
    for (const [domain, data] of Object.entries(byDomain) as any[]) {
      const c = data.cards;
      data.counts.wip = c.filter((x: any) => x.status === 'WIP').length;
      data.counts.next = c.filter((x: any) => x.status === 'Next' || x.status === 'Later').length;
      data.counts.blocked = c.filter((x: any) => x.status === 'Blocked').length;
      data.counts.activeCards = c.filter((x: any) => x.status !== "Won't Do").length;
      // Sum workflows for cards in this domain
      let wfCount = 0;
      for (const card of c) { wfCount += wfByCard[card.id] || 0; }
      data.counts.activeWorkflows = wfCount;
      data.counts.activeTotal = data.counts.activeCards + wfCount;
    }

    // Fix:feature ratio (#1909 AC6)
    const typeCounts: Record<string, number> = {};
    for (const card of cards) {
      const t = (card as any).type || 'untyped';
      typeCounts[t] = (typeCounts[t] || 0) + 1;
    }
    const fixes = typeCounts['fix'] || 0;
    const features = (typeCounts['new'] || 0) + (typeCounts['enhance'] || 0);
    const fixFeatureRatio = features > 0 ? (fixes / features).toFixed(2) : fixes > 0 ? 'all-fix' : 'n/a';

    res.json({ domains: byDomain, totalCards: cards.length, typeCounts, fixFeatureRatio });
  } catch (err) {
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
    const boardTs = '/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards';
    const output = execSync(`bash ${boardTs} view ${cardId} 2>/dev/null`, envOpts).trim();
    // Parse the output
    const titleMatch = output.match(/^#\d+\s+(.+)/);
    const statusMatch = output.match(/Status:\s+(\S+)/);
    const ownerMatch = output.match(/Owner:\s+(\S+)/);
    const descMatch = output.match(/Desc:\n([\s\S]*?)(?=\n  \w+:|$)/);
    const desc = descMatch ? descMatch[1].replace(/^    /gm, '').trim() : '';
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
    const comments = commentsSection ? commentsSection[1].replace(/^    /gm, '').trim() : '';
    // Extract blast/domain radius sections
    const blastRadius = output.match(/\*\*Blast Radius\*\*[^\n]*\n([\s\S]*?)(?=\n\*\*|_Generated|$)/);
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

// Role TTY map — populated on first request
const roleTTYs: Record<string, string> = {};

function getRoleTTY(role: string): string | null {
  const fs = require('fs');
  const pidFile = `/tmp/claude-team-scan/${role}.pid`;
  try {
    const pid = fs.readFileSync(pidFile, 'utf-8').trim();
    const { execSync } = require('child_process');
    const tty = execSync(`ps -p ${pid} -o tty=`, { encoding: 'utf-8' }).trim();
    if (tty && tty !== '??') return `/dev/${tty}`;
  } catch {}
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

  const token = socket.handshake.auth?.token || socket.handshake.query?.token;
  // Also check cookies from handshake headers
  const cookieHeader = socket.handshake.headers?.cookie || '';
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

  // Message from The Clearing UI — Jeff or guest (#1719, #1802 reverted to working state)
  socket.on('jeff-message', (data: { text: string; from?: string }, ack?: (result: { ok: boolean; error?: string }) => void) => {
    const { text } = data;
    if (!text?.trim()) { ack?.({ ok: false, error: 'empty' }); return; }

    // Determine sender — cookie or explicit from field
    const cookieHeader = socket.handshake.headers?.cookie || '';
    const nameMatch = cookieHeader.match(/bridge_name=([^;]+)/);
    const guestName = nameMatch ? decodeURIComponent(nameMatch[1]) : '';
    const senderName = data.from || guestName || 'jeff';

    // Record message
    messageRouter.ingest({
      from: senderName,
      text: text.trim(),
      ts: new Date().toISOString(),
      type: 'jeff-input',
    });

    // Route to mentioned roles — @mentions override tile lock
    const mentions = text.match(/@(wren|silas|kade)/gi) || [];
    const targets = mentions.length > 0
      ? [...new Set(mentions.map((m: string) => m.slice(1).toLowerCase()))]
      : [parseTarget(text)];
    const cleanText = text.replace(/@(wren|silas|kade)\s*/gi, '').trim();

    const { execSync } = require('child_process');
    let finalText = cleanText.replace(/\[img:(\/uploads\/[^\]]+)\]/g, `[img:http://localhost:${PORT}$1]`);
    const safeMsg = finalText.replace(/"/g, '\\"');

    for (const target of targets) {
      try {
        const cmd = `bash "${NUDGE_SCRIPT}" "${target}" "${safeMsg}" --from jeff --force`;
        console.log(`[clearing] delivering to ${target}: ${cleanText.substring(0, 60)}`);
        const result = execSync(cmd, {
          timeout: 10_000,
          encoding: 'utf-8',
          env: { ...process.env, PATH: '/Users/jeffbridwell/.nvm/versions/node/v20.11.1/bin:/opt/homebrew/bin:/usr/local/bin:/usr/sbin:/usr/bin:/bin:/sbin', HOME: '/Users/jeffbridwell' },
        });
        console.log(`[clearing] result: ${result.trim()}`);
      } catch (err) {
        const errMsg = err instanceof Error ? (err as any).stderr || err.message : String(err);
        console.error(`[clearing] delivery to ${target} failed: ${errMsg}`);
        ack?.({ ok: false, error: errMsg });
        return;
      }
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
  // Also refresh tiles immediately on state changes
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
    io.close();
    server.close(() => process.exit(0));
    // Force exit after 3s if graceful close hangs
    setTimeout(() => process.exit(0), 3000);
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

app.post('/api/chat/message', async (req, res) => {
  const { text, activeRoles } = req.body;
  if (!text) return res.status(400).json({ error: 'text required' });
  // Fire-and-forget — responses stream via Socket.IO
  clearingChat.handleMessage(text, activeRoles);
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

server.listen(PORT, () => {
  console.log(`The Clearing listening on http://localhost:${PORT}`);
});

/** Parse @mention to determine target role. Default: wren */
function parseTarget(text: string): string {
  const match = text.match(/^@(wren|silas|kade)\b/i);
  if (match) return match[1].toLowerCase();

  // Context-based routing: if text mentions a role's domain keywords
  const lower = text.toLowerCase();
  if (lower.includes('deploy') || lower.includes('infra') || lower.includes('hook') || lower.includes('launchagent')) return 'silas';
  if (lower.includes('build') || lower.includes('test') || lower.includes('handler') || lower.includes('page')) return 'kade';
  return 'wren'; // PM is default
}
