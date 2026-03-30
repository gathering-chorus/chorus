#!/usr/bin/env node
/**
 * Bridge Event Bus Subscriber
 * Connects to Bridge (localhost:3470) via Socket.IO, listens for board and role events,
 * writes relevant events to the role's nudge inbox for prompt-cycle drain.
 *
 * Usage: node bridge-subscriber.js <role>
 * Card: #1694
 */

// Resolve socket.io-client from Bridge's node_modules
const BRIDGE_NODE_MODULES = '/Users/jeffbridwell/CascadeProjects/chorus/directing/clearing/node_modules';
const io = require(`${BRIDGE_NODE_MODULES}/socket.io-client`);
const fs = require('fs');
const path = require('path');

const role = process.argv[2];
if (!role || !['wren', 'silas', 'kade'].includes(role)) {
  console.error('Usage: bridge-subscriber.js <role>');
  process.exit(1);
}

const BRIDGE_URL = 'http://localhost:3470';
const INBOX_DIR = `/tmp/voice-inbox/${role}`;
const INBOX_FILE = path.join(INBOX_DIR, 'pending-inject.txt');

// Ensure inbox dir exists
fs.mkdirSync(INBOX_DIR, { recursive: true });

function queueEvent(text) {
  fs.appendFileSync(INBOX_FILE, text + '\n');
}

function formatBoardEvent(event) {
  const { type, card, role: eventRole, detail } = event;

  // Skip events from our own role — we already know
  if (eventRole === role) return null;

  switch (type) {
    case 'card.pulled':
      return `[bridge] ${eventRole} pulled #${card} to WIP`;
    case 'card.accepted':
      return `[bridge] #${card} accepted by ${eventRole}`;
    case 'card.rejected':
      return `[bridge] #${card} rejected by ${eventRole}${detail ? ': ' + detail : ''}`;
    case 'card.demo.started':
      return `[bridge] ${eventRole} demoing #${card} — /gemba ${eventRole}`;
    case 'role.state.changed':
      return `[bridge] ${eventRole} → ${detail || 'unknown state'}`;
    default:
      return null; // Don't surface unknown events
  }
}

console.error(`[bridge-subscriber] Connecting to ${BRIDGE_URL} for role ${role}...`);

const socket = io(BRIDGE_URL, {
  reconnection: true,
  reconnectionDelay: 5000,
  reconnectionAttempts: Infinity,
  timeout: 30000,   // Must be > server pingInterval (25s)
  pingTimeout: 30000,
});

socket.on('connect', () => {
  console.error(`[bridge-subscriber] Connected to Bridge event bus`);
});

socket.on('board-event', (event) => {
  const formatted = formatBoardEvent(event);
  if (formatted) {
    queueEvent(formatted);
  }
});

socket.on('disconnect', (reason) => {
  console.error(`[bridge-subscriber] Disconnected: ${reason}. Reconnecting...`);
});

socket.on('connect_error', (err) => {
  console.error(`[bridge-subscriber] Connection error: ${err.message}`);
});

// Graceful shutdown
process.on('SIGINT', () => {
  console.error('[bridge-subscriber] Shutting down');
  socket.close();
  process.exit(0);
});

process.on('SIGTERM', () => {
  console.error('[bridge-subscriber] Shutting down');
  socket.close();
  process.exit(0);
});
