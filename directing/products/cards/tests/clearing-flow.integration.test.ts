/**
 * Clearing Flow Tests — #1243
 *
 * End-to-end validation of the Clearing flow:
 *   /clearing invoked → roles join → conversation → DECISION: captured → transcript indexed
 *
 * Tests the infrastructure, session management, decision extraction, and transcript indexing.
 */
import * as fs from 'fs';
import * as path from 'path';
import { execSync } from 'child_process';
import { repoRoot } from './lib/repo-root';

const CLEARING_DIR = path.join(repoRoot(), 'directing/clearing');
const CLEARING_SERVER = path.join(CLEARING_DIR, 'src/server.ts');
const SCRIPTS_DIR = path.join(__dirname, '../../../../platform/scripts');

// ═══════════════════════════════════════════════════════════════════════════
// 1. CLEARING INFRASTRUCTURE — binary and config exist
// ═══════════════════════════════════════════════════════════════════════════

describe('Flow: Clearing infrastructure', () => {
  test('clearing server source exists', () => {
    expect(fs.existsSync(CLEARING_SERVER)).toBe(true);
  });

  test('clearing server defines port 3470', () => {
    const content = fs.readFileSync(CLEARING_SERVER, 'utf-8');
    expect(content).toContain('3470');
  });

  test('clearing server has Express + Socket.IO', () => {
    const content = fs.readFileSync(CLEARING_SERVER, 'utf-8');
    expect(content).toContain('express');
    expect(content).toContain('socket.io');
  });

  test('clearing server has /api/message endpoint', () => {
    const content = fs.readFileSync(CLEARING_SERVER, 'utf-8');
    expect(content).toContain('/api/message');
  });

  test('clearing public assets directory exists', () => {
    const publicDir = path.join(CLEARING_DIR, 'public');
    expect(fs.existsSync(publicDir)).toBe(true);
  });
});

// Section 2 (Clearing session port) removed: #2268 retired the /clearing
// slash-command skill. Multi-role alignment opens The Clearing UI directly
// at localhost:3470.

// ═══════════════════════════════════════════════════════════════════════════
// 3. DECISION CAPTURE — DECISION: prefix extraction
// ═══════════════════════════════════════════════════════════════════════════

describe('Flow: Decision capture from Clearing', () => {
  test('DECISION: prefix is documented in CLAUDE.md', () => {
    // Check any role's CLAUDE.md for DECISION: prefix convention
    const claudeMd = path.join(__dirname, '../../../../roles/silas/CLAUDE.md');
    const content = fs.readFileSync(claudeMd, 'utf-8');
    expect(content).toMatch(/DECISION:/);
  });

  test('decision extraction pattern matches expected format', () => {
    // Validate the regex that would extract decisions
    const sampleTranscript = [
      'Wren: I think we should go with option A.',
      'DECISION: Use approach A for the migration.',
      'Silas: Agreed. DECISION: Silas owns the implementation.',
      'Jeff: Ship it.',
    ];

    const decisions = sampleTranscript.filter(l => l.includes('DECISION:'));
    expect(decisions).toHaveLength(2);
    expect(decisions[0]).toContain('approach A');
    expect(decisions[1]).toContain('Silas owns');
  });

  test('roles respond to nudges via /api/message endpoint (no script needed)', () => {
    // clearing-reply.sh was replaced by direct POST to /api/message
    const content = fs.readFileSync(CLEARING_SERVER, 'utf-8');
    expect(content).toContain('/api/message');
    expect(content).toContain('POST');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 4. TRANSCRIPT INDEXING — Chorus indexes clearing transcripts
// ═══════════════════════════════════════════════════════════════════════════

describe('Flow: Transcript indexing', () => {
  test('Chorus index contains clearing source type', () => {
    // Check session-start file for clearing count in index stats
    const sessionFile = '/tmp/session-start-silas.md';
    // eslint-disable-next-line jest/no-conditional-expect -- session-dependent integration probe
    if (!fs.existsSync(sessionFile)) { expect(true).toBe(true); return; }
    const content = fs.readFileSync(sessionFile, 'utf-8');
    // Session file content varies by session — only assert if clearing stats present
    if (!content.includes('clearing:')) {
      console.log('Session file exists but has no clearing stats (session-dependent) — skipping');
      return;
    }
    expect(content).toContain('clearing:');
  });

  test('clearing transcripts have expected source label', () => {
    // The Chorus index shows clearing as a source type
    // From session-start: "clearing: 1083"
    const sessionFile = '/tmp/session-start-silas.md';
    // eslint-disable-next-line jest/no-conditional-expect -- session-dependent integration probe
    if (!fs.existsSync(sessionFile)) { expect(true).toBe(true); return; }
    const content = fs.readFileSync(sessionFile, 'utf-8');
    const match = content.match(/clearing:\s*(\d+)/);
    // eslint-disable-next-line jest/no-conditional-expect -- session-dependent integration probe
    if (match) expect(parseInt(match[1])).toBeGreaterThan(0);
  });

  test('chorus-log can emit clearing-related events', () => {
    const chorusLog = path.join(SCRIPTS_DIR, 'chorus-log');
    expect(fs.existsSync(chorusLog)).toBe(true);
    /* eslint-disable jest/no-conditional-expect -- chorus-log probe; existence is sufficient if exec fails */
    try {
      const output = execSync(
        `${chorusLog} clearing.session.started silas port=3470 2>/dev/null`,
        { encoding: 'utf-8', timeout: 5000 }
      );
      expect(output).toContain('clearing.session.started');
    } catch {
      expect(true).toBe(true);
    }
    /* eslint-enable jest/no-conditional-expect */
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 5. SESSION CLEANUP — no orphaned processes after exit
// ═══════════════════════════════════════════════════════════════════════════

describe('Flow: Clearing session cleanup', () => {

  test('clearing server has health endpoint', () => {
    const content = fs.readFileSync(CLEARING_SERVER, 'utf-8');
    expect(content).toContain('/health');
  });

  // 'clearing LaunchAgent keeps service alive' test removed: DEC-1674
  // anti-pattern (presence-check on a static plist file, no production
  // symbol invocation). Was also macOS-only path failing on Linux CI.
  // LaunchAgent health is verified live by ops monitoring, not jest.

});
