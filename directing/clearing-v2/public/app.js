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
  return role === 'abby-normal' ? 'Abby Normal' : role;
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

function appendMessage(msg) {
  const container = messagesContainer || (typeof document !== 'undefined' ? document.getElementById('messages-stream') : null);
  if (!msg || !msg.visible || !container) return;

  const existing = document.getElementById(`msg-${msg.id}`) || document.getElementById(msg.id);
  if (existing) return; // avoid duplicates

  const cleanText = stripPromptHeader(msg.text || '');
  const bodyKey = cleanText.trim();
  const children = Array.isArray(container.children) ? container.children : Array.from(container.children || []);
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
if (typeof window !== 'undefined' && window.addEventListener) {
  window.addEventListener('DOMContentLoaded', scrollRoomToNewest);
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
  if (inputEl) inputEl.value = '';

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

// Form send handler
if (composerForm && composerInput) {
  composerForm.addEventListener('submit', async (e) => {
    e.preventDefault();
    await handleSendMessage(composerInput.value);
  });
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
    appendLinkedText,
    updateTiles,
    scrollRoomToNewest,
    initStream,
    loadOrCreateRoomKey,
    signEvent,
    handleSendMessage,
    teamNudgesOn,
    setTeamNudges,
  };
}
