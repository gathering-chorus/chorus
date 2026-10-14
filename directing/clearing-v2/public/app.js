// @domain: messages

const messagesContainer = typeof document !== 'undefined' ? document.getElementById('messages-stream') : null;
const composerForm = typeof document !== 'undefined' ? document.getElementById('composer-form') : null;
const composerInput = typeof document !== 'undefined' ? document.getElementById('composer-input') : null;
const relayIndicator = typeof document !== 'undefined' ? document.getElementById('relay-indicator') : null;
const tilesContainer = typeof document !== 'undefined' ? document.getElementById('tiles-container') : null;

function formatAuthor(role) {
  return role === 'abby-normal' ? 'Abby Normal' : role;
}

function formatMessageTime(ts) {
  try {
    return new Date(ts).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  } catch {
    return ts;
  }
}

function appendMessage(msg) {
  if (!msg || !msg.visible || !messagesContainer) return;

  const existing = document.getElementById(`msg-${msg.id}`);
  if (existing) return; // avoid duplicates

  const el = document.createElement('div');
  el.className = 'message';
  el.id = `msg-${msg.id}`;

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

  const text = document.createElement('div');
  text.className = 'message-text';
  text.textContent = msg.text;

  el.appendChild(header);
  el.appendChild(text);

  messagesContainer.appendChild(el);
  messagesContainer.scrollTop = messagesContainer.scrollHeight;
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
  const evtSource = new EventSource('/api/events');

  evtSource.addEventListener('init', (e) => {
    try {
      const data = JSON.parse(e.data);
      if (data.messages) {
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

// Form send handler
if (composerForm && composerInput) {
  composerForm.addEventListener('submit', async (e) => {
    e.preventDefault();
    const text = composerInput.value.trim();
    if (!text) return;

    composerInput.value = '';

    try {
      await fetch('/api/send', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ from: 'jeff', text }),
      });
    } catch (err) {
      console.error('Send failed', err);
    }
  });
}

if (typeof window !== 'undefined') {
  // Periodic tile poll as fallback
  setInterval(async () => {
    try {
      const res = await fetch('/api/tiles');
      if (res.ok) {
        const tiles = await res.json();
        updateTiles(tiles);
      }
    } catch {}
  }, 5000);

  initStream();
}

if (typeof module !== 'undefined' && module.exports) {
  module.exports = { formatAuthor, formatMessageTime, appendMessage, updateTiles, initStream };
}
