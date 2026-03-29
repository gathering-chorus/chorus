import express from 'express';
import { createServer } from 'http';
import { Server } from 'socket.io';
import path from 'path';
import fs from 'fs';
import crypto from 'crypto';
import { execSync, exec } from 'child_process';
import { Participants } from './participants';
import { Transcript } from './transcript';

// --- Nudge bridge: send messages to role terminal sessions ---
const NUDGE_SCRIPT = path.resolve(__dirname, '../../scripts/nudge.sh');

function executeNudge(from: string, target: string, message: string): { success: boolean; detail: string } {
  try {
    const safeMsg = message.replace(/"/g, '\\"');
    const safeFrom = from.replace(/"/g, '\\"');
    const replyTo = serverPort ? `http://localhost:${serverPort}/api/message` : '';
    const replyFlag = replyTo ? ` --reply-to "${replyTo}"` : '';
    const result = execSync(
      `bash "${NUDGE_SCRIPT}" "${target}" "${safeMsg}" --from "${safeFrom}" --force${replyFlag}`,
      { timeout: 10_000, encoding: 'utf-8' }
    ).trim();
    return { success: true, detail: result };
  } catch (err: unknown) {
    const errMsg = err instanceof Error ? err.message : String(err);
    return { success: false, detail: errMsg };
  }
}

function extractNudges(content: string): { cleaned: string; nudges: Array<{ target: string; message: string }> } {
  const nudges: Array<{ target: string; message: string }> = [];
  const cleaned = content.replace(/^\/nudge\s+(wren|silas|kade)\s+(.+)$/gim, (_match, target, msg) => {
    nudges.push({ target: target.toLowerCase(), message: msg.replace(/^["']|["']$/g, '') });
    return '';
  });
  return { cleaned: cleaned.trim(), nudges };
}

const MODEL = process.env.CLEARING_MODEL || 'claude-haiku-4-5-20251001';
const MAX_TOKENS = parseInt(process.env.CLEARING_MAX_TOKENS || '300');
const PORT = parseInt(process.env.CLEARING_PORT || '3460'); // fixed port for always-on service
const CHORUS_INDEX = process.env.CHORUS_INDEX !== 'false'; // index to /chorus by default
const SESSION_CONTEXT = process.env.CLEARING_CONTEXT || ''; // injected context for focused sessions
// Guest state — can be activated at startup (env) or mid-session (API)
let guestModeActive = process.env.CLEARING_GUEST === 'true';
let sessionToken = process.env.CLEARING_SESSION_TOKEN || '';

if (!process.env.ANTHROPIC_API_KEY) {
  console.error('ANTHROPIC_API_KEY not set');
  process.exit(1);
}

// Read Werk version from manifest
let werkVersion = '';
try {
  const manifestPath = path.resolve(__dirname, '../../../messages/claudemd/manifest.json');
  const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf-8'));
  werkVersion = manifest.version || '';
} catch { /* no manifest — version stays blank */ }

const app = express();
const server = createServer(app);
const io = new Server(server);
const participants = new Participants(MODEL, MAX_TOKENS, SESSION_CONTEXT, guestModeActive);
const transcript = new Transcript(MODEL);

let sessionActive = false;
let autoSaveInterval: NodeJS.Timeout | null = null;
let serverPort = 0; // stored on listen for tab close
let sessionId = ''; // current session UUID
let guestName = ''; // guest's display name (set on join)
let guestPresent = false; // true when a guest socket is connected

// Save transcript on process signals — save session but keep running unless SIGTERM
function handleShutdown(signal: string) {
  console.log(`\nReceived ${signal}`);
  if (sessionActive && transcript.getMessages().length > 0) {
    endSession(signal.toLowerCase());
  }
  if (signal === 'SIGTERM') {
    process.exit(0); // launchd stop = clean exit
  }
}
process.on('SIGTERM', () => handleShutdown('SIGTERM'));
process.on('SIGINT', () => handleShutdown('SIGINT'));
process.on('SIGHUP', () => handleShutdown('SIGHUP'));

// Auto-save transcript every 30 seconds while active
function startAutoSave() {
  if (autoSaveInterval) return;
  autoSaveInterval = setInterval(() => {
    if (transcript.getMessages().length > 0) {
      transcript.save();
    }
  }, 30_000);
}

function stopAutoSave() {
  if (autoSaveInterval) {
    clearInterval(autoSaveInterval);
    autoSaveInterval = null;
  }
}

// Parse JSON bodies for the /api/message endpoint
app.use(express.json());

// Serve the chat UI
app.use(express.static(path.join(__dirname, '..', 'public')));

// Proxy local files for rich media (images, etc.)
// SECURITY: disabled when guest is present (DEC-272)
app.get('/file/*', (req, res) => {
  if (guestPresent) {
    return res.status(403).send('File proxy disabled — guest session active');
  }
  const filePath = '/' + (req.params as Record<string, string>)[0];
  res.sendFile(filePath, (err) => {
    if (err) res.status(404).send('File not found');
  });
});

// --- Guest invite API ---
// Generate a guest invite token mid-session
app.post('/api/guest-invite', (_req, res) => {
  if (guestPresent) {
    return res.status(409).json({ error: 'Guest already connected' });
  }
  // Generate 128-bit token
  sessionToken = crypto.randomBytes(16).toString('hex');
  guestModeActive = true;
  // Switch participants to guest-safe prompts
  participants.setGuestMode(true);
  // Build the guest join URL
  const tunnelUrl = 'https://clearing.lightlifeurbangardens.com';
  const localUrl = `http://localhost:${serverPort}`;
  const guestUrl = `${tunnelUrl}?token=${sessionToken}`;
  const localGuestUrl = `${localUrl}?token=${sessionToken}`;
  console.log(`Guest invite created: ${guestUrl}`);
  // Notify all connected clients about guest mode activation
  io.emit('guest-mode', { active: true, guestName: null });
  res.json({ ok: true, token: sessionToken, guestUrl, localGuestUrl });
});

// Revoke guest access
app.delete('/api/guest-invite', (_req, res) => {
  sessionToken = '';
  guestModeActive = false;
  guestPresent = false;
  guestName = '';
  // Switch participants back to normal prompts
  participants.setGuestMode(false);
  // Disconnect any guest sockets
  for (const [, s] of io.sockets.sockets) {
    if ((s as any).isGuest) {
      s.disconnect(true);
    }
  }
  io.emit('guest-mode', { active: false, guestName: null });
  console.log('Guest invite revoked');
  res.json({ ok: true });
});

// --- Session management ---
app.post('/api/sessions', (req, res) => {
  // End current session if active
  if (sessionActive && transcript.getMessages().length > 0) {
    endSession('new-session');
  }
  // Create new session
  sessionId = `clearing-${Date.now()}`;
  sessionActive = true;
  const context = req.body?.context || SESSION_CONTEXT || '';
  if (context) {
    participants.updateContext(context);
  }
  startAutoSave();
  console.log(`New session: ${sessionId}`);
  res.json({ ok: true, sessionId, port: serverPort });
});

app.get('/api/sessions', (_req, res) => {
  res.json({
    active: sessionActive,
    sessionId,
    messageCount: transcript.getMessages().length,
    port: serverPort,
  });
});

// --- Round-trip: poll for new messages ---
app.get('/api/messages', (req, res) => {
  const since = parseInt(req.query.since as string) || 0;
  const msgs = transcript.getMessages().filter(m => parseInt(m.id) > since);
  res.json({ messages: msgs, port: serverPort });
});

// --- Round-trip: receive responses from terminal roles ---
app.post('/api/message', (req, res) => {
  const { from, content } = req.body;
  if (!from || !content) {
    return res.status(400).json({ error: 'from and content required' });
  }
  const msg = transcript.add(from, content);
  io.emit('message', { ...msg, viaTerminal: true });
  console.log(`Terminal response from ${from}: ${content.slice(0, 80)}...`);
  res.json({ ok: true, id: msg.id });
});

function parseAddressed(content: string): string[] {
  const mentions = content.match(/@(\w+)/g);
  if (!mentions) return [];
  return mentions.map((m) => m.slice(1).toLowerCase());
}

// --- Guest auth middleware ---
// Always active — checks runtime token state on every handshake.
// Local connections (no token) are always Jeff.
io.use((socket, next) => {
  const token = socket.handshake.auth?.token as string;
  const name = socket.handshake.auth?.name as string;
  // Local connections without token are Jeff (host)
  if (!token) {
    (socket as any).isGuest = false;
    (socket as any).displayName = 'Jeff';
    return next();
  }
  // Guest mode must be active and token must match
  if (!guestModeActive || !sessionToken || token !== sessionToken) {
    return next(new Error('Invalid session token'));
  }
  if (!name || name.trim().length === 0) {
    return next(new Error('Guest name required'));
  }
  (socket as any).isGuest = true;
  (socket as any).displayName = name.trim();
  return next();
});

io.on('connection', (socket) => {
  const isGuest = (socket as any).isGuest === true;
  const displayName = (socket as any).displayName || 'Jeff';

  sessionActive = true;
  startAutoSave();

  if (isGuest) {
    guestName = displayName;
    guestPresent = true;
    console.log(`Guest connected: ${guestName}`);
    // Announce guest arrival
    const sysMsg = transcript.add('System', `${guestName} has joined the session.`);
    io.emit('message', sysMsg);
    io.emit('guest-mode', { active: true, guestName });
  } else {
    console.log('Jeff connected');
  }

  // Send participant info and session config
  socket.emit('init', {
    participants: participants.getRoles().map((r) => ({
      name: r.name,
      title: r.title,
      color: r.color,
    })),
    model: MODEL,
    maxTokens: MAX_TOKENS,
    werkVersion: werkVersion ? `Werk v${werkVersion}` : '',
    context: SESSION_CONTEXT || null,
    guestMode: guestModeActive,
    guestName: isGuest ? displayName : null,
  });

  // Replay any existing messages (reconnection support)
  for (const msg of transcript.getMessages()) {
    socket.emit('message', msg);
  }

  socket.on('message', async (data: string | { text: string; activeRoles?: string[] }) => {
    const content = typeof data === 'string' ? data : data.text;
    const activeRoles = typeof data === 'object' ? data.activeRoles : undefined;
    const senderName = isGuest ? guestName : 'Jeff';

    // Guests cannot use nudge commands
    if (!isGuest) {
      // Check for nudge command from Jeff: /nudge <role>, or shorthand nw/ns/nk
      const shorthandMap: Record<string, string> = { nw: 'wren', ns: 'silas', nk: 'kade' };
      const nudgeMatch = content.match(/^(?:\/nudge\s+(wren|silas|kade)|(nw|ns|nk))\s+(.+)$/i);
      if (nudgeMatch) {
        const target = (nudgeMatch[1] || shorthandMap[nudgeMatch[2]?.toLowerCase()]).toLowerCase();
        const msg = nudgeMatch[3].replace(/^["']|["']$/g, '');
        console.log(`Nudge detected: target=${target} msg=${msg}`);
        // Record Jeff's message in the conversation so everyone sees it
        const jeffMsg = transcript.add('Jeff', content);
        io.emit('message', jeffMsg);

        const result = executeNudge('jeff', target, msg);
        const sysMsg = transcript.add('System',
          result.success ? `Nudge sent to ${target}.` : `Nudge to ${target} failed: ${result.detail}`);
        io.emit('message', sysMsg);
        return;
      }
    }

    // Record message with sender attribution
    const userMsg = transcript.add(senderName, content);
    io.emit('message', userMsg);

    // Notify client if this is a DECISION marker
    if (/^DECISION[\s:–—-]/i.test(content)) {
      io.emit('decision', {
        messageId: userMsg.id,
        text: content,
        speaker: senderName,
      });
    }

    // Determine which roles respond
    // Priority: @mentions > activeRoles from UI badges > all roles
    const addressed = parseAddressed(content);
    const respondingRoles =
      addressed.length > 0
        ? participants.getRoles().filter((r) => addressed.includes(r.name.toLowerCase()))
        : activeRoles && activeRoles.length > 0
          ? participants.getRoles().filter((r) => activeRoles.includes(r.name.toLowerCase()))
          : participants.getRoles();

    // Each role responds sequentially with streaming
    for (const role of respondingRoles) {
      try {
        // Signal typing
        io.emit('stream:start', { sender: role.name });

        const response = await participants.getResponse(
          role,
          transcript.getMessages(),
          (token: string) => {
            io.emit('stream:token', { sender: role.name, token });
          }
        );

        // Detect [pass] — role chose not to respond
        const trimmed = response.content.trim();
        if (trimmed === '[pass]' || trimmed.toLowerCase().startsWith('[pass]')) {
          io.emit('stream:end', { sender: role.name, passed: true });
          console.log(`${role.name} passed`);
          continue;
        }

        // Extract and execute nudges from role response
        const { cleaned, nudges } = extractNudges(response.content);
        const displayContent = nudges.length > 0 ? cleaned : response.content;

        const roleMsg = transcript.add(role.name, displayContent, {
          input: response.inputTokens,
          output: response.outputTokens,
        });

        io.emit('stream:end', {
          sender: role.name,
          message: roleMsg,
          cleanedContent: nudges.length > 0 ? displayContent : undefined,
        });

        // Execute nudges after streaming ends
        for (const nudge of nudges) {
          const result = executeNudge(role.name.toLowerCase(), nudge.target, nudge.message);
          const nudgeNotice = transcript.add('System',
            result.success
              ? `${role.name} nudged ${nudge.target}'s terminal: "${nudge.message}"`
              : `${role.name}'s nudge to ${nudge.target} failed: ${result.detail}`
          );
          io.emit('message', nudgeNotice);
        }

        // Send updated cost + decision info
        const decisions = transcript.extractDecisions();
        io.emit('cost', {
          totalTokens: transcript.getTotalTokens(),
          estimatedCost: transcript.getEstimatedCost(),
          messageCount: transcript.getMessages().length,
          decisionCount: decisions.length,
        });
      } catch (err: unknown) {
        const errMsg = err instanceof Error ? err.message : String(err);
        const sysMsg = transcript.add('System', `${role.name} failed to respond: ${errMsg}`);
        io.emit('stream:end', { sender: role.name, message: sysMsg });
      }
    }
  });

  socket.on('end-session', () => {
    endSession('user-initiated');
  });

  socket.on('disconnect', () => {
    if (isGuest) {
      console.log(`Guest disconnected: ${guestName}`);
      guestPresent = false;
      const sysMsg = transcript.add('System', `${guestName} has left the session.`);
      io.emit('message', sysMsg);
      io.emit('guest-mode', { active: guestModeActive, guestName: null });
    } else {
      console.log('Jeff disconnected');
      if (sessionActive) {
        endSession('disconnect');
      }
    }
  });
});

function endSession(reason: string) {
  sessionActive = false;
  stopAutoSave();

  if (transcript.getMessages().length > 0) {
    const filePath = transcript.save();
    const returnObj = transcript.buildReturnObject(filePath);
    const summary = returnObj.session;
    const decisions = returnObj.decisions;

    console.log(`\n${'='.repeat(60)}`);
    console.log(`Session ended (${reason})`);
    console.log(`Transcript: ${filePath}`);
    console.log(
      `${summary.messageCount} messages, ${summary.totalTokens.input + summary.totalTokens.output} tokens, ~$${summary.estimatedCost.toFixed(3)}`
    );

    if (decisions.length > 0) {
      console.log(`\nDecisions captured (${decisions.length}):`);
      for (const d of decisions) {
        console.log(`  [${d.speaker}] ${d.marker}`);
      }
    } else {
      console.log('\nNo DECISION markers found in transcript.');
    }
    console.log(`${'='.repeat(60)}`);

    // Write return object to known location for invoking process
    fs.writeFileSync('/tmp/clearing-last-transcript.txt', filePath);
    fs.writeFileSync('/tmp/clearing-last-return.json', JSON.stringify(returnObj, null, 2));

    // Notify client before closing
    io.emit('session-ended', { path: filePath });

    // Index transcript to /chorus shared memory
    if (CHORUS_INDEX) {
      indexToChorus(filePath);
    }

    // Capture intake items and auto-route to roles
    captureAndRoute('/tmp/clearing-last-return.json');
  } else {
    // No messages — still notify client
    io.emit('session-ended', { path: '' });
  }

  // Reset transcript for next session (always-on mode)
  transcript.reset();
  sessionId = '';
  console.log('Session ended — waiting for next session');
}

/**
 * Capture intake items from the Clearing session and auto-route to roles.
 * Runs chorus-capture.sh, then routes items with clear role assignments:
 * - Decisions → workflow + handoff brief
 * - Commitments/actions → handoff brief only
 * Ambiguous items stay pending for Wren to triage.
 */
function captureAndRoute(returnJsonPath: string) {
  const repoRoot = path.resolve(__dirname, '../../..');
  const captureScript = path.join(repoRoot, 'chorus/scripts/chorus-capture.sh');
  const workflowScript = path.join(repoRoot, 'messages/scripts/workflow.sh');
  const intakeDir = path.join(process.env.HOME || '', '.chorus', 'intake');

  // 1. Run chorus-capture.sh to create intake items
  let captureOutput: string;
  try {
    captureOutput = execSync(`bash "${captureScript}" "${returnJsonPath}"`, {
      timeout: 10_000,
      encoding: 'utf-8',
    }).trim();
    const firstLine = captureOutput.split('\n')[0] || '';
    console.log(`Capture: ${firstLine}`);
  } catch (err: unknown) {
    const errMsg = err instanceof Error ? err.message : String(err);
    console.log(`Capture failed (non-fatal): ${errMsg}`);
    return;
  }

  // 2. Find the intake file — derive session_id from return object
  let intake: { session_id: string; items: Array<Record<string, unknown>> };
  try {
    const returnObj = JSON.parse(fs.readFileSync(returnJsonPath, 'utf-8'));
    const started = (returnObj.session?.started || '') as string;
    const sessionId = started.replace(/[:.]/g, '-').slice(0, 19);
    const intakeFile = path.join(intakeDir, `${sessionId}.json`);

    if (!fs.existsSync(intakeFile)) {
      console.log('No intake file created — nothing to route');
      return;
    }
    intake = JSON.parse(fs.readFileSync(intakeFile, 'utf-8'));
  } catch (err: unknown) {
    const errMsg = err instanceof Error ? err.message : String(err);
    console.log(`Intake read failed (non-fatal): ${errMsg}`);
    return;
  }

  // 3. Filter for auto-routable items
  const routable = intake.items.filter((item: Record<string, unknown>) => {
    const routing = item.routing as Record<string, unknown> | undefined;
    return (
      item.status === 'pending' &&
      routing?.action_required &&
      routing?.suggested_owner &&
      routing?.suggested_owner !== 'jeff' // Jeff's items stay for Wren
    );
  });

  if (routable.length === 0) {
    console.log('No auto-routable items');
    return;
  }

  // 4. Route each item
  const briefDirs: Record<string, string> = {
    silas: path.join(repoRoot, 'architect/briefs'),
    kade: path.join(repoRoot, 'engineer/briefs'),
    wren: path.join(repoRoot, 'product-manager/briefs'),
  };

  const date = new Date().toISOString().slice(0, 10);
  const routed: string[] = [];

  for (const item of routable) {
    const routing = item.routing as Record<string, unknown>;
    const owner = routing.suggested_owner as string;
    const briefDir = briefDirs[owner];
    if (!briefDir) continue;

    const itemType = item.type as string;
    const itemText = item.text as string;
    const itemRole = item.role as string;
    const cardTitle = (routing.suggested_card_title as string) || itemText.slice(0, 60);

    try {
      // Decisions with clear owner → create workflow
      if (itemType === 'decision') {
        const safeText = itemText.replace(/"/g, '\\"').replace(/`/g, '\\`');
        const safeStep = `${owner}:${cardTitle.replace(/"/g, '\\"').replace(/`/g, '\\`')}`;
        try {
          execSync(
            `bash "${workflowScript}" create "${safeText}" --source "clearing:${intake.session_id}" --steps "${safeStep}"`,
            { timeout: 5_000, encoding: 'utf-8' }
          );
        } catch {
          // Workflow creation is best-effort — brief still gets written
        }
      }

      // Write handoff brief
      const briefName = `${date}-clearing-${itemType}-${routed.length + 1}.md`;
      const briefPath = path.join(briefDir, briefName);
      const lines = [
        `# Clearing ${itemType}: ${cardTitle}`,
        '',
        `**From:** Clearing session \`${intake.session_id}\``,
        `**Type:** ${itemType} | **Speaker:** ${itemRole}`,
        `**Auto-routed to:** ${owner}`,
        '',
        '## Content',
        '',
        itemText,
        '',
      ];
      if (routing.target_entity) {
        lines.push(`**Related:** ${routing.target_entity}`, '');
      }
      fs.writeFileSync(briefPath, lines.join('\n'));

      // Mark as routed
      item.status = 'routed';
      (item as Record<string, unknown>).routed_at = new Date().toISOString();
      routed.push(`[${itemType[0].toUpperCase()}] → ${owner}`);
    } catch (err: unknown) {
      const errMsg = err instanceof Error ? err.message : String(err);
      console.log(`Route failed for ${itemType} (non-fatal): ${errMsg}`);
    }
  }

  // 5. Save updated intake with routed statuses
  try {
    const started = (JSON.parse(fs.readFileSync(returnJsonPath, 'utf-8')).session?.started || '') as string;
    const sessionId = started.replace(/[:.]/g, '-').slice(0, 19);
    const intakeFile = path.join(intakeDir, `${sessionId}.json`);
    fs.writeFileSync(intakeFile, JSON.stringify(intake, null, 2));
  } catch { /* best-effort — intake file update is non-critical */ }

  if (routed.length > 0) {
    console.log(`Auto-routed ${routed.length}: ${routed.join(', ')}`);
  }
}

/**
 * Index the clearing transcript into the chorus shared memory index.
 * Uses the same SQLite FTS5 database as Slack and Claude session indexing.
 */
function indexToChorus(transcriptPath: string) {
  const dbPath = process.env.CHORUS_DB || path.join(process.env.HOME || '', '.chorus', 'index.db');

  if (!fs.existsSync(dbPath)) {
    console.log('Chorus index not found — skipping indexing');
    return;
  }

  try {
    const sessionId = path.basename(transcriptPath, '.json');

    // Write Python indexer to a temp file to avoid shell escaping issues
    const tmpScript = path.join(require('os').tmpdir(), `clearing-index-${sessionId}.py`);
    const pyCode = `
import json, sqlite3, os, sys
from datetime import datetime, timezone

db_path = sys.argv[1]
transcript_path = sys.argv[2]
session_id = sys.argv[3]

conn = sqlite3.connect(db_path)
cur = conn.cursor()
count = 0

with open(transcript_path) as f:
    data = json.load(f)

for msg in data.get('messages', []):
    sender = msg.get('sender', 'unknown')
    content = msg.get('content', '')
    ts = msg.get('timestamp', 0)
    msg_id = msg.get('id', '')

    if not content or len(content) < 5:
        continue

    role_map = {'Jeff': 'jeff', 'Wren': 'wren', 'Silas': 'silas', 'Kade': 'kade', 'System': 'system'}
    role = role_map.get(sender, 'unknown')
    author = sender.lower()

    iso_ts = datetime.fromtimestamp(ts / 1000, tz=timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ') if ts > 0 else datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')
    source_id = f'clearing:{session_id}:{msg_id}'

    is_decision = content.upper().startswith('DECISION')
    metadata = json.dumps({'session_id': session_id, 'is_decision': is_decision, 'path': transcript_path})

    try:
        cur.execute('''INSERT INTO messages
            (source, source_id, channel, role, author, content, timestamp, session_id, metadata)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)''',
            ('clearing', source_id, 'clearing:session', role, author, content, iso_ts, session_id, metadata))
        if cur.rowcount > 0:
            count += 1
    except sqlite3.IntegrityError:
        pass

now = datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')
cur.execute("INSERT OR REPLACE INTO watermarks (source, last_seen, last_indexed) VALUES (?, ?, ?)",
            (f'clearing:{session_id}', str(os.path.getsize(transcript_path)), now))

conn.commit()
conn.close()
print(count)
`.trimStart();

    fs.writeFileSync(tmpScript, pyCode);

    const result = execSync(`python3 "${tmpScript}" "${dbPath}" "${transcriptPath}" "${sessionId}"`, {
      timeout: 10_000,
      encoding: 'utf-8',
    }).trim();

    // Clean up temp file
    try { fs.unlinkSync(tmpScript); } catch {}

    if (result && result !== '0') {
      console.log(`Indexed ${result} messages to /chorus`);
    }
  } catch (err: unknown) {
    const errMsg = err instanceof Error ? err.message : String(err);
    console.log(`Chorus indexing failed: ${errMsg}`);
  }
}

server.listen(PORT, () => {
  const addr = server.address();
  const port = typeof addr === 'object' && addr ? addr.port : PORT;
  serverPort = port;
  const url = `http://localhost:${port}`;

  // Write port for terminal roles to discover
  fs.writeFileSync('/tmp/clearing-port', String(port));

  console.log(`The Clearing is open at ${url}`);
  console.log(`Model: ${MODEL} | Max tokens: ${MAX_TOKENS}`);
  console.log(`Participants: Jeff, ${participants.getRoles().map((r) => r.name).join(', ')}`);
  if (CHORUS_INDEX) console.log('Chorus indexing: enabled');
  if (guestModeActive) {
    console.log(`Guest mode: enabled (token required for external access)`);
    console.log(`Guest join URL: ${url}?token=${sessionToken}`);
  }
  console.log('');

  // Open browser (macOS)
  try {
    execSync(`open "${url}"`);
  } catch {
    console.log(`Open ${url} in your browser`);
  }
});
