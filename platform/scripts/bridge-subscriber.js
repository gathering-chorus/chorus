#!/usr/bin/env node
/**
 * Bridge Event Bus Subscriber
 * Connects to Bridge (localhost:3470) via Socket.IO, listens for board and role events,
 * writes relevant events to the role's nudge inbox for prompt-cycle drain.
 *
 * Usage: node bridge-subscriber.js <role>
 * Card: #1694
 */

// Resolve socket.io-client from Bridge's node_modules (CHORUS_ROOT, not hardcoded — #1964)
const CHORUS_ROOT = process.env.CHORUS_ROOT || '/Users/jeffbridwell/CascadeProjects/chorus';
const BRIDGE_NODE_MODULES = `${CHORUS_ROOT}/directing/clearing/node_modules`;
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
  // Bridge events are informational — visible on Bridge UI already.
  // Don't inject into terminal sessions where they become noise for Jeff (#2298).
  // Only queue events that require role action (blocked, rejected).
  if (text.startsWith('[bridge]') && !text.includes('BLOCKED') && !text.includes('rejected')) {
    return;
  }
  fs.appendFileSync(INBOX_FILE, text + '\n');
}

function formatBoardEvent(event) {
  const { type, card, role: eventRole, detail, cardOwner } = event;

  // AC 2: Skip events from our own role — we already know
  if (eventRole === role) return null;

  // AC 1: Skip Jeff's own actions — don't echo back what he just did
  if (eventRole === 'jeff') return null;

  switch (type) {
    case 'card.pulled':
      // AC 4: Only notify observers of the pulling role, not everyone
      return `[bridge] ${eventRole} pulled #${card} to WIP`;
    case 'card.accepted':
      // AC 3: Acceptance events only go to card owner and building role
      if (cardOwner && cardOwner !== role && eventRole !== role) return null;
      return `[bridge] #${card} accepted by ${eventRole}`;
    case 'card.rejected':
      return `[bridge] #${card} rejected by ${eventRole}${detail ? ': ' + detail : ''}`;
    case 'card.demo.started':
      return `[bridge] ${eventRole} demoing #${card} — /gemba ${eventRole}`;
    case 'role.state.changed':
      // AC 5: Only surface blocked state, not routine transitions
      if (detail !== 'blocked') return null;
      return `[bridge] ${eventRole} BLOCKED${detail ? ': ' + detail : ''}`;
    default:
      return null;
  }
}

// #4130 — every line carries its own time.
//
// bridge-subscriber-health.test.sh asked "any ping timeouts in the last 20
// LINES", because the lines had no timestamps and a line count was the only
// window available. A count-window cannot age out: a fix stays red until enough
// new lines push the old ones past 20, and a subscriber that is healthy and
// therefore quiet never emits those lines at all — so the healthier it gets,
// the longer it stays red. That is what kept this suite at 3 fails after the
// launchd fix took it from 6.
//
// With a timestamp per line the test can ask the question it always meant:
// "has this subscriber timed out RECENTLY", which a quiet log answers with no.
const stamp = () => new Date().toISOString();
const log = (msg) => console.error(`${stamp()} ${msg}`);

log(`[bridge-subscriber] Connecting to ${BRIDGE_URL} for role ${role}...`);

const socket = io(BRIDGE_URL, {
  reconnection: true,
  reconnectionDelay: 5000,
  reconnectionAttempts: Infinity,
  timeout: 60000,
  pingTimeout: 120000,  // Server pingInterval=60s — give 120s tolerance (#1964)
});

socket.on('connect', () => {
  log(`[bridge-subscriber] Connected to Bridge event bus`);
});

socket.on('board-event', (event) => {
  const formatted = formatBoardEvent(event);
  if (formatted) {
    queueEvent(formatted);
  }
});

socket.on('disconnect', (reason) => {
  log(`[bridge-subscriber] Disconnected: ${reason}. Reconnecting...`);
});

socket.on('connect_error', (err) => {
  log(`[bridge-subscriber] Connection error: ${err.message}`);
});

// Graceful shutdown
process.on('SIGINT', () => {
  log("[bridge-subscriber] Shutting down");
  socket.close();
  process.exit(0);
});

process.on('SIGTERM', () => {
  log("[bridge-subscriber] Shutting down");
  socket.close();
  process.exit(0);
});
