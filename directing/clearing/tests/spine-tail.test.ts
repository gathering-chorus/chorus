// @test-type: integration — #3607: builds real tmpdir fixture logs (incl. a 100MB+ one) to prove tail-read latency; not unit.
/**
 * spine-tail.ts — #3607 tail-read tests.
 *
 * AC under test:
 *  - readSpineLines tail-reads chorus.log (last ~256KB only) — never loads the whole file
 *  - /api/stream p95 latency < 50ms with a 100MB+ chorus.log (fixture built here, no live log)
 *  - response shape unchanged (same StreamLine semantics as the pre-#3607 full-read implementation)
 */

import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { tailReadUtf8, readSpineLines, TAIL_BYTES } from '../src/spine-tail';

let dir: string;
beforeAll(() => { dir = fs.mkdtempSync(path.join(os.tmpdir(), 'spine-tail-')); });
afterAll(() => { fs.rmSync(dir, { recursive: true, force: true }); });

function spineLine(i: number, role = 'wren', event = 'session_turn'): string {
  return JSON.stringify({
    timestamp: `2026-07-03T12:${String(i % 60).padStart(2, '0')}:00`,
    role, event, summary: `turn number ${i} with some content`, tool_count: '3',
  });
}

describe('tailReadUtf8', () => {
  test('small file (< TAIL_BYTES): returns full content', () => {
    const f = path.join(dir, 'small.log');
    fs.writeFileSync(f, 'line-a\nline-b\nline-c\n');
    expect(tailReadUtf8(fs, f)).toBe('line-a\nline-b\nline-c\n');
  });

  test('large file: returns at most ~TAIL_BYTES and drops the partial first line', () => {
    const f = path.join(dir, 'large.log');
    const lines = Array.from({ length: 20000 }, (_, i) => spineLine(i));
    fs.writeFileSync(f, lines.join('\n') + '\n');
    const tail = tailReadUtf8(fs, f);
    expect(tail.length).toBeLessThanOrEqual(TAIL_BYTES);
    // no partial JSON at the head: the first line parses clean
    const first = tail.split('\n').filter(Boolean)[0];
    expect(() => JSON.parse(first)).not.toThrow();
    // and the file's true last line is present at the end
    expect(tail.trimEnd().endsWith(lines[lines.length - 1])).toBe(true);
  });

  test('missing file: returns empty string (readSpineLines stays [] like before)', () => {
    expect(tailReadUtf8(fs, path.join(dir, 'nope.log'))).toBe('');
    expect(readSpineLines(fs, path.join(dir, 'nope.log'), 60)).toEqual([]);
  });
});

describe('readSpineLines — response shape regression (pre-#3607 semantics)', () => {
  test('parses turn/tool/gemba entries newest-first, skips noise, caps at limit*2', () => {
    const f = path.join(dir, 'shape.log');
    const rows = [
      JSON.stringify({ timestamp: 't1', role: 'wren', event: 'session_turn', summary: 'real turn content here', tool_count: '2' }),
      JSON.stringify({ timestamp: 't2', role: 'kade', event: 'session_tool', summary: 'Bash: cargo test', action: 'Bash' }),
      JSON.stringify({ timestamp: 't3', role: 'kade', event: 'session_tool', summary: 'Read: file.ts', action: 'Read' }), // skipped (Read)
      JSON.stringify({ timestamp: 't4', role: 'silas', event: 'nudge.emitted', from: 'silas,to=wren,content=[gemba] watching the run' }),
      JSON.stringify({ timestamp: 't5', role: 'silas', event: 'nudge.emitted', from: 'silas,to=wren,content=plain nudge' }), // skipped (not gemba)
      JSON.stringify({ timestamp: 't6', role: 'jeffx', event: 'session_turn', summary: 'wrong role skipped' }), // skipped (role)
      JSON.stringify({ timestamp: 't7', role: 'wren', event: 'session_turn', summary: 'jeff typed this', tool_count: '0' }), // jeff-input mapping
      'not json at all', // skipped silently
    ];
    fs.writeFileSync(f, rows.join('\n') + '\n');
    const out = readSpineLines(fs, f, 60);
    // newest-first iteration order (t7 first), same as the old implementation
    expect(out).toEqual([
      { ts: 't7', role: 'jeff', type: 'turn', text: '→wren: jeff typed this' },
      { ts: 't4', role: 'silas', type: 'gemba', text: '[gemba] watching the run' },
      { ts: 't2', role: 'kade', type: 'tool', text: '→ cargo test' },
      { ts: 't1', role: 'wren', type: 'turn', text: 'real turn content here' },
    ]);
  });

  test('caps parsed entries at limit*2 like the old implementation', () => {
    const f = path.join(dir, 'cap.log');
    const rows = Array.from({ length: 50 }, (_, i) => spineLine(i));
    fs.writeFileSync(f, rows.join('\n') + '\n');
    expect(readSpineLines(fs, f, 10)).toHaveLength(20);
  });
});

describe('readSpineLines — latency on a 100MB+ log', () => {
  test('p95 < 50ms', () => {
    const f = path.join(dir, 'big.log');
    // ~120 bytes/line ⇒ ~900k lines ≈ 105MB, streamed in 1MB chunks
    const fd = fs.openSync(f, 'w');
    const chunkLines: string[] = [];
    for (let i = 0; i < 8192; i++) chunkLines.push(spineLine(i));
    const chunk = chunkLines.join('\n') + '\n';
    let written = 0;
    while (written < 105 * 1024 * 1024) written += fs.writeSync(fd, chunk);
    fs.closeSync(fd);
    expect(fs.statSync(f).size).toBeGreaterThan(100 * 1024 * 1024);

    const samples: number[] = [];
    for (let i = 0; i < 20; i++) {
      const t0 = process.hrtime.bigint();
      const out = readSpineLines(fs, f, 80);
      const ms = Number(process.hrtime.bigint() - t0) / 1e6;
      samples.push(ms);
      expect(out.length).toBeGreaterThan(0);
    }
    samples.sort((a, b) => a - b);
    const p95 = samples[Math.floor(samples.length * 0.95) - 1] ?? samples[samples.length - 1];
    expect(p95).toBeLessThan(50);
  }, 120000);
});
