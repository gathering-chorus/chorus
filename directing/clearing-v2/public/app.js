/** #4571 — the path prefix this page was served under ('' on the LAN, '/clearing-v2' via the door). */
function clearingBase() {
  return (typeof window !== 'undefined' && window.CLEARING_BASE) || '';
}

// @domain: messages

const messagesContainer = typeof document !== 'undefined' ? document.getElementById('messages-stream') : null;
const composerForm = typeof document !== 'undefined' ? document.getElementById('composer-form') : null;
const composerInput = typeof document !== 'undefined' ? document.getElementById('composer-input') : null;
const relayIndicator = typeof document !== 'undefined' ? document.getElementById('relay-indicator') : null;
const tilesContainer = typeof document !== 'undefined' ? document.getElementById('tiles-container') : null;

const STORE = 'chorus.room.key.v1';

let nodeSchnorr = null;
let nodeSha256 = null;
let nodeUtils = null;

if (typeof require !== 'undefined') {
  try {
    nodeSchnorr = require('@noble/curves/secp256k1').schnorr;
    nodeSha256 = require('@noble/hashes/sha256').sha256;
    nodeUtils = require('@noble/hashes/utils');
  } catch {}
}

const hexFromBytes = (b) => nodeUtils ? nodeUtils.bytesToHex(b) : Array.from(b).map((x) => x.toString(16).padStart(2, '0')).join('');
const bytesFromHex = (h) => nodeUtils ? nodeUtils.hexToBytes(h) : new Uint8Array(h.match(/.{2}/g).map((b) => parseInt(b, 16)));
const utf8Encode = (s) => new TextEncoder().encode(s);

function getLocalStorage(custom) {
  if (custom) return custom;
  if (typeof localStorage !== 'undefined') return localStorage;
  return null;
}

// #4592 / #4618 — the room's "team nudges" toggle, off by default, remembered per device.
// The room page's own script reads the same key (views.ts TEAM_NUDGES_KEY).
const TEAM_NUDGES_KEY = 'clearing-v2.teamNudges';
function teamNudgesOn(customStorage) {
  const storage = getLocalStorage(customStorage);
  try { return !!storage && storage.getItem(TEAM_NUDGES_KEY) === 'on'; } catch { return false; }
}
function setTeamNudges(customStorage, on) {
  const storage = getLocalStorage(customStorage);
  try { if (storage) storage.setItem(TEAM_NUDGES_KEY, on ? 'on' : 'off'); } catch {}
}

function loadOrCreateRoomKey(webid, customStorage) {
  if (!webid) {
    throw new Error('room-key: not signed in — no WebID to bind a key to. Sign in first; a key minted now would belong to nobody.');
  }
  const storage = getLocalStorage(customStorage);
  let existing = null;
  if (storage) {
    try {
      const all = JSON.parse(storage.getItem(STORE) || '{}');
      existing = all[webid] || null;
    } catch {}
  }

  if (existing) {
    const skBytes = bytesFromHex(existing);
    let pubkey = '';
    if (nodeSchnorr) {
      pubkey = hexFromBytes(nodeSchnorr.getPublicKey(skBytes));
    } else if (typeof window !== 'undefined' && window.roomKey) {
      pubkey = window.roomKey.pubkeyOf(skBytes);
    }
    return { sk: skBytes, pubkey, fresh: false };
  }

  const sk = (typeof crypto !== 'undefined' && crypto.getRandomValues)
    ? crypto.getRandomValues(new Uint8Array(32))
    : new Uint8Array(32).fill(1);
  const skHex = hexFromBytes(sk);
  if (storage) {
    try {
      const all = JSON.parse(storage.getItem(STORE) || '{}');
      all[webid] = skHex;
      storage.setItem(STORE, JSON.stringify(all));
    } catch {}
  }
  let pubkey = '';
  if (nodeSchnorr) {
    pubkey = hexFromBytes(nodeSchnorr.getPublicKey(sk));
  } else if (typeof window !== 'undefined' && window.roomKey) {
    pubkey = window.roomKey.pubkeyOf(sk);
  }
  return { sk, pubkey, fresh: true };
}

function signEvent(sk, content, options = {}) {
  const room = options.room || 'team';
  const kind = options.kind || 1;
  const tags = options.tags || [['room', room]];
  let pubkey = '';
  if (nodeSchnorr) {
    pubkey = hexFromBytes(nodeSchnorr.getPublicKey(sk));
  } else if (typeof window !== 'undefined' && window.roomKey) {
    pubkey = window.roomKey.pubkeyOf(sk);
  }
  const created_at = Math.floor(Date.now() / 1000);
  const serialized = JSON.stringify([0, pubkey.toLowerCase(), created_at, kind, tags, content]);

  let id = '';
  let sig = '';
  if (nodeSha256 && nodeSchnorr) {
    id = hexFromBytes(nodeSha256(utf8Encode(serialized)));
    sig = hexFromBytes(nodeSchnorr.sign(bytesFromHex(id), sk));
  } else if (typeof window !== 'undefined' && window.roomKey) {
    return window.roomKey.signEvent(sk, kind, tags, content);
  }
  return { id, pubkey: pubkey.toLowerCase(), created_at, kind, tags, content, sig };
}

function formatAuthor(role) {
  if (role === 'abby-normal') return 'Abby Normal';
  if (role === 'borg-queen') return 'Borg Queen';
  return role;
}

function formatMessageTime(ts) {
  try {
    return new Date(ts).toLocaleTimeString('en-US', {
      timeZone: 'America/New_York',
      hour: '2-digit',
      minute: '2-digit',
      hour12: false,
      hourCycle: 'h23',
    });
  } catch {
    return ts;
  }
}

function stripPromptHeader(text) {
  return String(text || '').replace(/^---\s+[^|\n]+\|[^\n]+---\s*\n*/, '');
}

function toMs(v) { const n = Number(v); return Number.isFinite(n) && n > 0 ? n : Date.parse(v || ''); }

/** #4646 — same fingerprint as views.ts bodyHash. */
function bodyHash(text) {
  let h = 5381;
  for (let i = 0; i < text.length; i++) h = ((h << 5) + h + text.charCodeAt(i)) | 0;
  return (h >>> 0).toString(36);
}

/** #4646 — a second copy of one message arrives within seconds of the first. */
const COPY_WINDOW_MS = 20000;

function appendMessage(msg) {
  const container = messagesContainer || (typeof document !== 'undefined' ? document.getElementById('messages-stream') : null);
  if (!msg || !msg.visible || !container) return;

  const existing = document.getElementById(`msg-${msg.id}`) || document.getElementById(msg.id);
  if (existing) return; // avoid duplicates

  const cleanText = stripPromptHeader(msg.text || '');
  const bodyKey = cleanText.trim();
  const children = Array.isArray(container.children) ? container.children : Array.from(container.children || []);
  // #4646 — the room merges a second copy of a message (same author, same words) and
  // still pushes it live; the page already shows it, so it is not shown twice.
  // Only a copy seconds apart is the same message; the same words a minute later are a repeat.
  const at = typeof msg.ts === 'number' ? msg.ts : Date.parse(msg.ts || '');
  if (children.some((c) => c && c.dataset && c.dataset.from === msg.from && (c.dataset.body === bodyKey || c.dataset.body === bodyHash(bodyKey))
    && Math.abs(toMs(c.dataset.ts) - at) < COPY_WINDOW_MS)) return;
  // #4591 — only upgrade a placeholder bridge message if the incoming message has the same body;
  // do not drop distinct messages that happen to share identical text.
  const bridgePlaceholder = (bodyKey && msg.from !== 'bridge')
    ? children.find((c) => c && c.dataset && c.dataset.from === 'bridge' && c.dataset.body === bodyKey)
    : null;
  if (bridgePlaceholder) {
    bridgePlaceholder.id = `msg-${msg.id}`;
    bridgePlaceholder.dataset.from = msg.from;
    const authorEl = bridgePlaceholder.querySelector && bridgePlaceholder.querySelector('.message-author');
    if (authorEl) {
      authorEl.textContent = formatAuthor(msg.from);
      authorEl.className = `message-author ${msg.from}`;
    }
    return;
  }

  const el = document.createElement('div');
  el.className = 'message';
  el.id = `msg-${msg.id}`;
  if (!el.dataset) el.dataset = {};
  el.dataset.from = msg.from;
  el.dataset.body = bodyKey;
  el.dataset.ts = msg.ts || '';
  if (msg.kind) el.dataset.kind = msg.kind; // #4592 — the team nudges toggle hides these

  const header = document.createElement('div');
  header.className = 'message-header';

  const author = document.createElement('span');
  author.className = `message-author ${msg.from}`;
  author.textContent = formatAuthor(msg.from);

  const time = document.createElement('span');
  time.className = 'message-time';
  time.textContent = formatMessageTime(msg.ts);

  header.appendChild(author);
  header.appendChild(time);

  if (msg.tick) {
    const tick = document.createElement('span');
    tick.className = 'tick delivery-tick';
    tick.textContent = '✓ relay';
    header.appendChild(tick);
  }

  const text = document.createElement('div');
  text.className = 'message-text';
  appendLinkedText(text, cleanText);

  el.appendChild(header);
  el.appendChild(text);
  appendPhotos(el, cleanText);

  container.appendChild(el);
  container.scrollTop = container.scrollHeight;
}

function appendInlineText(parent, text) {
  const tokens = String(text).split(/(`[^`\n]+`|\*\*[^*\n]+\*\*|https?:\/\/[^\s<>"']+|#\d+)/g);
  for (const tok of tokens) {
    if (!tok) continue;
    if (tok.startsWith('`') && tok.endsWith('`') && tok.length > 2) {
      const code = document.createElement('code');
      code.textContent = tok.slice(1, -1);
      parent.appendChild(code);
    } else if (tok.startsWith('**') && tok.endsWith('**') && tok.length > 4) {
      const strong = document.createElement('strong');
      strong.textContent = tok.slice(2, -2);
      parent.appendChild(strong);
    } else if (/^https?:\/\//.test(tok)) {
      const m = tok.match(/^(https?:\/\/[^\s<>"']+?)([.,;!?)]*)$/);
      const url = m ? m[1] : tok;
      const punct = m ? m[2] : '';
      const a = document.createElement('a');
      a.href = url;
      a.target = '_blank';
      a.rel = 'noopener noreferrer';
      a.textContent = url;
      parent.appendChild(a);
      if (punct) {
        parent.appendChild(document.createTextNode(punct));
      }
    } else if (/^#\d+$/.test(tok)) {
      const id = tok.slice(1);
      const a = document.createElement('a');
      a.className = 'cid card-ref';
      a.href = '#';
      a.textContent = tok;
      a.onclick = () => {
        if (typeof openCardDrawer === 'function') openCardDrawer(id);
        return false;
      };
      parent.appendChild(a);
    } else {
      parent.appendChild(document.createTextNode(tok));
    }
  }
}

function appendBlocks(parent, text) {
  const lines = text.split('\n');
  let currentUl = null;
  let currentOl = null;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const bulletMatch = line.match(/^[-*]\s+(.*)$/);
    const numberedMatch = line.match(/^(\d+)\.\s+(.*)$/);

    if (bulletMatch) {
      currentOl = null;
      if (!currentUl) {
        currentUl = document.createElement('ul');
        parent.appendChild(currentUl);
      }
      const li = document.createElement('li');
      appendInlineText(li, bulletMatch[1]);
      currentUl.appendChild(li);
    } else if (numberedMatch) {
      currentUl = null;
      if (!currentOl) {
        currentOl = document.createElement('ol');
        parent.appendChild(currentOl);
      }
      const li = document.createElement('li');
      appendInlineText(li, numberedMatch[2]);
      currentOl.appendChild(li);
    } else {
      currentUl = null;
      currentOl = null;
      if (i > 0 && parent.children && parent.children.length > 0) {
        const lastChild = parent.children[parent.children.length - 1];
        const lastTag = lastChild && (lastChild.tag || lastChild.tagName);
        if (lastTag !== 'UL' && lastTag !== 'OL' && lastTag !== 'ul' && lastTag !== 'ol') {
          parent.appendChild(document.createElement('br'));
        }
      }
      if (line.length > 0) {
        appendInlineText(parent, line);
      }
    }
  }
}

/** #4574/#4575/#4597 — #N card links, http(s) links, **bold**, lists, code fences and line breaks
 *  built from DOM nodes: the message text is never parsed as HTML, so a message cannot inject markup. */
/** #4635 — names of uploaded photos a message mentions (same rule as the server's uploadNames). */
function photoNames(text) {
  return Array.from(String(text || '').matchAll(/\/clearing-v2-uploads\/(\d+\.(?:jpg|png|gif|heic))/g), (m) => m[1]);
}

function appendPhotos(el, text) {
  const names = photoNames(text);
  if (names.length === 0 || typeof document === 'undefined') return;
  const box = document.createElement('div');
  box.className = 'msg-photos';
  for (const n of names) {
    const a = document.createElement('a');
    a.href = clearingBase() + '/uploads/' + n;
    a.target = '_blank';
    const img = document.createElement('img');
    img.src = clearingBase() + '/uploads/' + n;
    img.alt = 'photo ' + n;
    a.appendChild(img);
    box.appendChild(a);
  }
  el.appendChild(box);
}

/** #4635 — the composer's camera button: upload the photo, then put its path in the
 *  message box so Jeff adds words and sends. Failure is named in the composer. */
async function uploadPhoto(file, fetchImpl) {
  if (!file) return { ok: false, reason: 'no photo chosen' };
  const type = /\.hei[cf]$/i.test(file.name || '') ? 'image/heic' : (file.type || 'image/jpeg');
  try {
    const res = await (fetchImpl || fetch)(clearingBase() + '/api/upload', { method: 'POST', headers: { 'Content-Type': type }, body: file });
    const body = await res.json().catch(() => ({}));
    if (!res.ok || !body.ok) return { ok: false, reason: body.reason || body.detail || ('upload failed (' + res.status + ')') };
    return { ok: true, path: body.path };
  } catch (e) {
    return { ok: false, reason: 'upload failed: ' + (e && e.message ? e.message : String(e)) };
  }
}

function watchPhotoButton() {
  if (typeof document === 'undefined') return;
  const picker = document.getElementById('composer-photo');
  if (!picker || !picker.addEventListener) return;
  const label = document.querySelector ? document.querySelector('.composer-photo') : null;
  if (label && label.addEventListener) {
    label.addEventListener('click', (e) => {
      if (e.target !== picker) {
        picker.click();
      }
    });
  }
  picker.addEventListener('change', async () => {
    const input = document.getElementById('composer-input');
    const status = document.getElementById('send-error');
    const files = Array.from(picker.files || []);
    const failed = [];
    for (const [i, file] of files.entries()) {
      if (status) status.textContent = 'Uploading photo ' + (i + 1) + ' of ' + files.length + '…';
      const out = await uploadPhoto(file);
      if (out.ok && input) input.value = (input.value ? input.value.trimEnd() + ' ' : '') + '📷 ' + out.path + ' ';
      if (!out.ok) failed.push(out.reason);
    }
    // #4635 — Jeff 12:22: "much easier if i can pick several at once"
    if (status) status.textContent = failed.length ? failed.length + ' of ' + files.length + ' photos not attached: ' + failed[0] : '';
    picker.value = '';
    if (input && input.focus) input.focus();
  });
}

/** #4635 — send a recording to Library for words. Failure is named, never silent. */
async function transcribeVoice(blob, fetchImpl) {
  if (!blob || !blob.size) return { ok: false, reason: 'the recording was empty' };
  try {
    const res = await (fetchImpl || fetch)(clearingBase() + '/api/voice', { method: 'POST', headers: { 'Content-Type': blob.type || 'audio/webm' }, body: blob });
    const body = await res.json().catch(() => ({}));
    if (!res.ok || !body.ok) return { ok: false, reason: body.reason || body.detail || ('voice failed (' + res.status + ')') };
    return { ok: true, transcript: body.transcript };
  } catch (e) {
    return { ok: false, reason: 'voice failed: ' + (e && e.message ? e.message : String(e)) };
  }
}

/** #4635 — tap the mic to record, tap again to stop; the words land in the message box. */
function watchMicButton() {
  if (typeof document === 'undefined' || typeof navigator === 'undefined') return;
  const mic = document.getElementById('composer-mic');
  if (!mic || !mic.addEventListener) return;
  let recorder = null;
  let chunks = [];
  const say = (t) => { const s = document.getElementById('send-error'); if (s) s.textContent = t; };
  mic.addEventListener('click', async () => {
    if (recorder && recorder.state === 'recording') { recorder.stop(); return; }
    if (!navigator.mediaDevices || !navigator.mediaDevices.getUserMedia || typeof MediaRecorder === 'undefined') {
      say('Voice not available here: this browser cannot record (it needs https)');
      return;
    }
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      chunks = [];
      recorder = new MediaRecorder(stream);
      recorder.ondataavailable = (e) => { if (e.data && e.data.size) chunks.push(e.data); };
      recorder.onstop = async () => {
        stream.getTracks().forEach((t) => t.stop());
        mic.setAttribute('aria-pressed', 'false');
        say('Turning your voice into words…');
        const out = await transcribeVoice(new Blob(chunks, { type: recorder.mimeType || 'audio/webm' }));
        say(out.ok ? '' : 'Voice not added: ' + out.reason);
        const input = document.getElementById('composer-input');
        if (out.ok && input) { input.value = (input.value ? input.value + ' ' : '') + out.transcript; input.focus(); }
      };
      recorder.start();
      mic.setAttribute('aria-pressed', 'true');
      say('Recording… tap the mic again to stop');
    } catch (e) {
      say('Voice not available: ' + (e && e.message ? e.message : String(e)));
    }
  });
}

function appendLinkedText(parent, text) {
  const str = String(text);
  const parts = str.split(/(```[a-zA-Z0-9_-]*\n?[\s\S]*?```)/g);
  for (const part of parts) {
    if (!part) continue;
    if (part.startsWith('```') && part.endsWith('```')) {
      const fenceMatch = part.match(/^```([a-zA-Z0-9_-]*)\n?([\s\S]*?)```$/);
      const lang = fenceMatch ? fenceMatch[1] : '';
      const codeContent = fenceMatch ? fenceMatch[2].replace(/\n$/, '') : part.slice(3, -3);
      const pre = document.createElement('pre');
      const code = document.createElement('code');
      if (lang) code.className = `language-${lang}`;
      code.textContent = codeContent;
      pre.appendChild(code);
      parent.appendChild(pre);
    } else {
      appendBlocks(parent, part);
    }
  }
}

/** #4575 — on a phone the room fills exactly what is visible, so Send sits above Safari's
 *  bar. window.innerHeight is the visible height; the header above the room varies (the
 *  trust strip wraps), so a fixed calc() left Send under the bar on Jeff's iPhone. */
function fitChatToViewport() {
  if (typeof document === 'undefined' || typeof window === 'undefined') return;
  const chat = document.querySelector('.chat');
  if (!chat || !chat.getBoundingClientRect) return;
  if (window.innerWidth > 768) { chat.style.height = ''; return; }
  const top = chat.getBoundingClientRect().top + window.scrollY;
  chat.style.height = Math.max(240, window.innerHeight - top) + 'px';
}
/** #4597 — the room's history is rendered by the server and nothing scrolled it, so the
 *  room opened at the oldest message. Open it at the newest. */
function scrollRoomToNewest() {
  if (typeof document === 'undefined') return;
  const stream = document.getElementById('messages-stream');
  if (stream) stream.scrollTop = stream.scrollHeight;
}
/** #4597 — Jeff 19:11: "i cant scroll up in message history". Near the top of the room,
 *  fetch the page of messages just older than the oldest shown and put it above, keeping
 *  the reader's place. Resolves to how many were added (0 = no older messages). */
let loadingOlder = false;
async function loadOlderMessages(fetchImpl) {
  if (typeof document === 'undefined' || loadingOlder) return 0;
  const stream = document.getElementById('messages-stream');
  const first = stream && stream.querySelector('[id^="msg-"]');
  const picker = document.getElementById('room-picker');
  if (!stream || !first) return 0;
  const room = (picker && picker.value) || 'team';
  loadingOlder = true;
  try {
    const url = clearingBase() + '/api/rooms/' + encodeURIComponent(room) + '/history?before=' + encodeURIComponent(first.id.slice(4));
    const res = await (fetchImpl || fetch)(url);
    if (!res.ok) return 0;
    const body = await res.json();
    if (!body || !body.count) return 0;
    const fromBottom = stream.scrollHeight - stream.scrollTop;
    first.insertAdjacentHTML('beforebegin', body.html);
    stream.scrollTop = stream.scrollHeight - fromBottom;
    return body.count;
  } finally {
    loadingOlder = false;
  }
}
/** #4646 — re-fetch the role tiles so a teammate's cards stay true while the page is open. */
async function refreshPeopleRow(fetchImpl) {
  if (typeof document === 'undefined') return false;
  const row = document.getElementById('rooms-people-row');
  const picker = document.getElementById('room-picker');
  if (!row) return false;
  const room = (picker && picker.value) || 'team';
  try {
    const res = await (fetchImpl || fetch)(clearingBase() + '/api/rooms/' + encodeURIComponent(room) + '/people');
    if (!res.ok) return false;
    const body = await res.json();
    if (!body || !body.html) return false;
    row.outerHTML = body.html;
    return true;
  } catch {
    return false;
  }
}

function watchRoomScroll() {
  if (typeof document === 'undefined') return;
  const stream = document.getElementById('messages-stream');
  if (stream && stream.addEventListener) {
    stream.addEventListener('scroll', () => { if (stream.scrollTop < 80) loadOlderMessages(); });
  }
}
if (typeof window !== 'undefined' && window.addEventListener) {
  window.addEventListener('DOMContentLoaded', scrollRoomToNewest);
  window.addEventListener('DOMContentLoaded', watchRoomScroll);
  window.addEventListener('DOMContentLoaded', () => { if (document.getElementById('rooms-people-row')) setInterval(refreshPeopleRow, 10000); });
  window.addEventListener('DOMContentLoaded', watchPhotoButton);
  window.addEventListener('DOMContentLoaded', watchMicButton);
  window.addEventListener('DOMContentLoaded', initComposerInput);
  window.addEventListener('load', scrollRoomToNewest);
  window.addEventListener('load', fitChatToViewport);
  window.addEventListener('resize', fitChatToViewport);
}

function updateTiles(tiles) {
  if (!Array.isArray(tiles)) return;
  for (const t of tiles) {
    const tileEl = document.getElementById(`tile-${t.role}`);
    if (tileEl) {
      const stateEl = tileEl.querySelector('.tile-state');
      if (stateEl) {
        stateEl.className = `tile-state ${t.state}`;
        stateEl.textContent = t.card ? `${t.state} #${t.card}` : t.state;
      }
    }
  }
}

// #4649 — a message of Jeff's that did not reach its role says so, instead of
// looking sent. The server sends the list on connect and each new failure live.
function markUndelivered(id, roles) {
  if (typeof document === 'undefined' || !id || !Array.isArray(roles) || roles.length === 0) return;
  const msg = document.getElementById(`msg-${id}`) || document.getElementById(id);
  if (!msg) return;
  if (msg.classList) msg.classList.add('undelivered');
  let mark = msg.querySelector ? msg.querySelector('.msg-undelivered') : null;
  if (!mark) {
    mark = document.createElement('div');
    mark.className = 'msg-undelivered';
    // under the words, not beside them: .msg lays header and text out in a row
    const text = msg.querySelector ? msg.querySelector('.message-text') : null;
    (text || msg).appendChild(mark);
  }
  mark.textContent = `not delivered to ${roles.join(', ')}`;
}

// Setup EventSource for SSE live stream
function initStream() {
  const evtSource = new EventSource(clearingBase() + '/api/events');

  evtSource.addEventListener('init', (e) => {
    try {
      const data = JSON.parse(e.data);
      // #4574 — pages without a message list (Board, Roles) ignore the room's history
      if (data.messages && messagesContainer) {
        messagesContainer.innerHTML = '';
        data.messages.forEach(appendMessage);
      }
      if (data.undelivered) Object.entries(data.undelivered).forEach(([id, roles]) => markUndelivered(id, roles));
      if (data.tiles) {
        updateTiles(data.tiles);
      }
      relayIndicator.textContent = 'Connected (SSE)';
      relayIndicator.className = 'relay-status connected';
    } catch (err) {
      console.error('Error parsing init event', err);
    }
  });

  evtSource.addEventListener('message', (e) => {
    try {
      const msg = JSON.parse(e.data);
      appendMessage(msg);
    } catch (err) {
      console.error('Error parsing message event', err);
    }
  });

  evtSource.addEventListener('delivery', (e) => {
    try {
      const d = JSON.parse(e.data);
      if (d && d.ok === false) markUndelivered(d.id, d.roles || [d.to]);
    } catch (err) {
      console.error('Error parsing delivery event', err);
    }
  });

  evtSource.addEventListener('tiles', (e) => {
    try {
      const tiles = JSON.parse(e.data);
      updateTiles(tiles);
    } catch (err) {
      console.error('Error parsing tiles event', err);
    }
  });

  evtSource.onerror = () => {
    relayIndicator.textContent = 'Disconnected (Reconnecting...)';
    relayIndicator.className = 'relay-status';
  };
}

async function handleSendMessage(text, options = {}) {
  const inputEl = options.inputEl || composerInput;
  const statusEl = options.statusEl || (typeof document !== 'undefined' ? document.getElementById('send-error') : null);
  const btnEl = options.btnEl || (typeof document !== 'undefined' && composerForm ? composerForm.querySelector('button[type="submit"]') : null);
  const fetchFn = options.fetchFn || (typeof fetch !== 'undefined' ? fetch : null);

  const cleanText = (text || '').trim();
  if (!cleanText) return;

  const savedText = cleanText;
  if (inputEl) {
    inputEl.value = '';
    if (inputEl.style) inputEl.style.height = 'auto';
  }

  if (btnEl) {
    btnEl.textContent = 'Sending...';
    if (btnEl.classList) {
      btnEl.classList.remove('delivery-failed', 'delivered');
      btnEl.classList.add('sending');
    }
  }

  try {
    let signedEvent = null;
    const webid = (typeof window !== 'undefined' && window.CHORUS_WEBID) ? window.CHORUS_WEBID : 'https://id.lightlifeurbangardens.com/jeff/profile/card#me';

    if (typeof window !== 'undefined' && window.roomKey && typeof window.roomKey.signEvent === 'function') {
      const { sk } = window.roomKey.loadOrCreate(webid);
      signedEvent = window.roomKey.signEvent(sk, 1, [['room', 'team']], cleanText);
    } else {
      const { sk } = loadOrCreateRoomKey(webid);
      signedEvent = signEvent(sk, cleanText, { room: 'team' });
    }

    if (!fetchFn) {
      throw new Error('fetch is not defined');
    }

    const post = () => fetchFn(clearingBase() + '/api/send', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ event: signedEvent, from: 'jeff', text: cleanText }),
    });
    let res = await post();
    // #4580 reopen — the server only accepts keys a signed-in session bound, and it holds
    // those in memory, so a server restart forgets this browser's key. On "not signed in",
    // bind the key with the session cookie (POST /api/room/bind) and retry the send once.
    if (!res.ok && res.status === 401 && signedEvent && signedEvent.pubkey) {
      const bind = await fetchFn(clearingBase() + '/api/room/bind', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ pubkey: signedEvent.pubkey, webid }),
      });
      if (bind && bind.ok) res = await post();
    }

    if (!res.ok) {
      let errName = `HTTP ${res.status}`;
      try {
        const json = await res.json();
        // #4571 — no session here: say so and link the door, keep the typed text
        if (json && json.error === 'not-signed-in') {
          if (inputEl) inputEl.value = savedText;
          showSignInNeeded(statusEl, json);
          if (btnEl) { if (btnEl.classList) btnEl.classList.remove('sending'); btnEl.textContent = 'Send'; }
          return;
        }
        if (json && json.error) errName = json.error;
      } catch {
        const txt = await res.text().catch(() => '');
        if (txt) errName = txt;
      }
      throw new Error(errName);
    }

    // Success. #4608 — the roles got it; say so plainly if Buzz did not store it.
    let sent = null;
    try { sent = await res.json(); } catch {}
    if (statusEl) {
      const notStored = sent && sent.stored === false;
      statusEl.textContent = notStored ? `Sent to the team, but not stored in Buzz: ${sent.reason || 'no reason given'}` : '';
      statusEl.style.display = notStored ? 'block' : 'none';
    }
    if (btnEl) {
      if (btnEl.classList) {
        btnEl.classList.remove('sending');
        btnEl.classList.add('delivered');
      }
      btnEl.textContent = 'Sent';
      setTimeout(() => {
        if (btnEl.classList) btnEl.classList.remove('delivered');
        btnEl.textContent = 'Send';
      }, 1500);
    }
  } catch (err) {
    const errName = (err && err.message) ? err.message : String(err);
    console.error('Send failed:', errName);

    // Restore input so typed text is not lost
    if (inputEl) inputEl.value = savedText;

    // Show failed send by name
    if (statusEl) {
      statusEl.textContent = `Failed: ${errName}`;
      statusEl.style.display = 'block';
    }
    if (btnEl) {
      if (btnEl.classList) {
        btnEl.classList.remove('sending');
        btnEl.classList.add('delivery-failed');
      }
      btnEl.textContent = `Failed: ${errName.slice(0, 24)}`;
      setTimeout(() => {
        if (btnEl.classList) btnEl.classList.remove('delivery-failed');
        btnEl.textContent = 'Send';
      }, 4000);
    }
  }
}

/** #4571 — the 401 names why and where to sign in; render it as text plus a link. */
function showSignInNeeded(statusEl, json) {
  if (!statusEl) return;
  statusEl.textContent = json.detail || 'Not signed in.';
  if (json.signInUrl && typeof document !== 'undefined' && document.createElement) {
    statusEl.textContent = 'Not signed in here. ';
    const a = document.createElement('a');
    a.href = json.signInUrl;
    a.textContent = 'Sign in to send';
    statusEl.appendChild(a);
  }
  statusEl.style.display = 'block';
}

// Form send handler & Enter for newline (CR) (#4639)
function initComposerInput() {
  if (typeof document === 'undefined') return;
  const form = document.getElementById('composer-form');
  const input = document.getElementById('composer-input');
  if (!form || !input) return;

  // Auto-resize textarea
  input.addEventListener('input', () => {
    input.style.height = 'auto';
    input.style.height = Math.min(input.scrollHeight, 120) + 'px';
  });

  // Enter = newline (default textarea behavior). Cmd+Enter or Ctrl+Enter = send!
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      void handleSendMessage(input.value);
    }
  });

  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    await handleSendMessage(input.value);
  });
}

if (typeof document !== 'undefined') {
  initComposerInput();
}

function initTrustStripTooltips() {
  if (typeof document === 'undefined') return;
  const chips = document.querySelectorAll('.app-header .trust-strip .chip');
  chips.forEach((chip) => {
    if (!chip.title) {
      chip.title = chip.textContent.trim().replace(/\s+/g, ' ');
    }
  });
}

if (typeof window !== 'undefined') {
  // Periodic tile poll as fallback
  setInterval(async () => {
    try {
      const res = await fetch(clearingBase() + '/api/tiles');
      if (res.ok) {
        const tiles = await res.json();
        updateTiles(tiles);
      }
    } catch {}
  }, 5000);

  initStream();
  initTrustStripTooltips();
}

if (typeof module !== 'undefined' && module.exports) {
  module.exports = {
    formatAuthor,
    formatMessageTime,
    appendMessage,
    markUndelivered,
    appendLinkedText,
    updateTiles,
    scrollRoomToNewest,
    loadOlderMessages,
    refreshPeopleRow,
    uploadPhoto,
    watchPhotoButton,
    transcribeVoice,
    photoNames,
    initStream,
    loadOrCreateRoomKey,
    signEvent,
    handleSendMessage,
    teamNudgesOn,
    setTeamNudges,
  };
}
