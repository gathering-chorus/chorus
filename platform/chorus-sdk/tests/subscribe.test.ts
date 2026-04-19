/**
 * subscribe.test.ts — spine-event subscription contract (#2239)
 *
 * Covers the SDK's `subscribe` function, which polls a log file for newly
 * appended JSONL events and fires a callback for each match. Tests use a
 * temp log file and a short poll interval so they run hermetically.
 */

import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { subscribe } from '../src/subscribe';

function mkTmpLog(): string {
  return path.join(os.tmpdir(), `chorus-sdk-subscribe-${process.pid}-${Math.random()}.log`);
}

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

function append(file: string, obj: Record<string, unknown>): void {
  fs.appendFileSync(file, JSON.stringify(obj) + '\n');
}

describe('subscribe', () => {
  it('fires callback for events matching a string filter', async () => {
    const logFile = mkTmpLog();
    fs.writeFileSync(logFile, '');
    const seen: Record<string, string>[] = [];
    const unsub = subscribe('card.pulled', (ev) => seen.push(ev), { logFile, pollInterval: 30 });
    await sleep(50);
    append(logFile, { event: 'card.pulled', role: 'kade', card: '2239' });
    append(logFile, { event: 'card.accepted', role: 'kade', card: '2239' });
    await sleep(100);
    unsub();
    expect(seen).toHaveLength(1);
    expect(seen[0].event).toBe('card.pulled');
    fs.unlinkSync(logFile);
  });

  it('string filter matches by substring (includes)', async () => {
    const logFile = mkTmpLog();
    fs.writeFileSync(logFile, '');
    const seen: Record<string, string>[] = [];
    const unsub = subscribe('pulled', (ev) => seen.push(ev), { logFile, pollInterval: 30 });
    await sleep(50);
    append(logFile, { event: 'card.pulled', role: 'kade' });
    append(logFile, { event: 'nudge.pulled.late', role: 'wren' });
    await sleep(100);
    unsub();
    expect(seen).toHaveLength(2);
    fs.unlinkSync(logFile);
  });

  it('regex filter matches by pattern', async () => {
    const logFile = mkTmpLog();
    fs.writeFileSync(logFile, '');
    const seen: Record<string, string>[] = [];
    const unsub = subscribe(/^gate\./, (ev) => seen.push(ev), { logFile, pollInterval: 30 });
    await sleep(50);
    append(logFile, { event: 'gate.code.passed' });
    append(logFile, { event: 'card.pulled' });
    append(logFile, { event: 'gate.ops.failed' });
    await sleep(100);
    unsub();
    expect(seen).toHaveLength(2);
    expect(seen.map((e) => e.event)).toEqual(['gate.code.passed', 'gate.ops.failed']);
    fs.unlinkSync(logFile);
  });

  it('function filter allows custom predicate', async () => {
    const logFile = mkTmpLog();
    fs.writeFileSync(logFile, '');
    const seen: Record<string, string>[] = [];
    const unsub = subscribe(
      (e: string) => e.endsWith('.failed'),
      (ev) => seen.push(ev),
      { logFile, pollInterval: 30 },
    );
    await sleep(50);
    append(logFile, { event: 'gate.code.passed' });
    append(logFile, { event: 'gate.ops.failed' });
    await sleep(100);
    unsub();
    expect(seen).toHaveLength(1);
    expect(seen[0].event).toBe('gate.ops.failed');
    fs.unlinkSync(logFile);
  });

  it('unsubscribe stops the poller (no callback after unsub)', async () => {
    const logFile = mkTmpLog();
    fs.writeFileSync(logFile, '');
    const seen: Record<string, string>[] = [];
    const unsub = subscribe('x', (ev) => seen.push(ev), { logFile, pollInterval: 30 });
    await sleep(50);
    unsub();
    append(logFile, { event: 'x' });
    await sleep(80);
    expect(seen).toHaveLength(0);
    fs.unlinkSync(logFile);
  });

  it('skips malformed JSON lines without crashing', async () => {
    const logFile = mkTmpLog();
    fs.writeFileSync(logFile, '');
    const seen: Record<string, string>[] = [];
    const unsub = subscribe('x', (ev) => seen.push(ev), { logFile, pollInterval: 30 });
    await sleep(50);
    fs.appendFileSync(logFile, 'not-json\n');
    append(logFile, { event: 'x', role: 'kade' });
    fs.appendFileSync(logFile, '{ broken\n');
    await sleep(100);
    unsub();
    expect(seen).toHaveLength(1);
    expect(seen[0].role).toBe('kade');
    fs.unlinkSync(logFile);
  });

  it('tolerates log file that does not exist at subscribe time', async () => {
    const logFile = mkTmpLog();
    // Do NOT create the file
    const seen: Record<string, string>[] = [];
    const unsub = subscribe('x', (ev) => seen.push(ev), { logFile, pollInterval: 30 });
    await sleep(50);
    fs.writeFileSync(logFile, '');
    append(logFile, { event: 'x' });
    await sleep(80);
    unsub();
    expect(seen).toHaveLength(1);
    fs.unlinkSync(logFile);
  });

  it('uses default LOG_FILE path when options.logFile not given', async () => {
    // We don't actually poll the real logs/chorus.log in tests; just verify
    // the function returns an unsubscribe without throwing when options omit
    // logFile. The timer is immediately cleared.
    const unsub = subscribe('x', () => {}, { pollInterval: 30 });
    expect(typeof unsub).toBe('function');
    unsub();
  });
});
