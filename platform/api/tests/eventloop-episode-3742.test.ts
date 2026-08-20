// @test-type: unit — the fs signal is the FileEpisodeGate's own tempdir state; no service, no network, no live surface
// #3742 — one freeze must read as ONE incident.
//
// Two detectors watch the same loop (#3082): an external probe and the
// in-process `blocked` library. A single ~20s freeze produces at least two
// events — the probe times out at its 8000ms ceiling mid-freeze (so it reports
// its CAP, never the real duration) and the in-process detector fires when the
// loop unfreezes with the FULL length. Jeff receives both as separate alarms.
// Measured 2026-08-04: three big freezes, each preceded ~15s by an exactly-8000ms
// probe timeout. One freeze, two clocks, two alarms.
import {
  episodeKey,
  groupIntoEpisodes,
  summarizeEpisode,
  isCappedReading,
  BlockReading,
} from '../src/eventloop-episode';

const r = (
  ts: string,
  duration_ms: number,
  source: 'probe' | 'in-process',
  op = 'unknown',
): BlockReading => ({ ts, duration_ms, source, op });

describe('#3742 episode keying', () => {
  it('the measured pair-pattern collapses to ONE episode', () => {
    // The real 2026-08-04 14:32/14:33 pair: probe caps at 8000, sendFile
    // reports 19559 fifteen seconds later. Same freeze.
    const readings = [
      r('2026-08-04T18:32:50.000Z', 8000, 'probe'),
      r('2026-08-04T18:33:05.000Z', 19559, 'in-process', 'sendFile'),
    ];
    const episodes = groupIntoEpisodes(readings, 30_000);
    expect(episodes).toHaveLength(1);
    expect(episodes[0].readings).toHaveLength(2);
  });

  it('an episode keeps BOTH vantages and reports the uncapped duration', () => {
    const episodes = groupIntoEpisodes(
      [
        r('2026-08-04T18:32:50.000Z', 8000, 'probe'),
        r('2026-08-04T18:33:05.000Z', 19559, 'in-process', 'sendFile'),
      ],
      30_000,
    );
    const s = summarizeEpisode(episodes[0]);
    expect(s.sources.sort()).toEqual(['in-process', 'probe']);
    // A capped reading must never be taken as a duration (#3742 card note:
    // "probe-timeout=8000 should be treated as freeze >=8s, duration unknown").
    expect(s.worst_ms).toBe(19559);
    expect(s.count).toBe(2);
  });

  it('a capped probe reading is labelled as a floor, not a measurement', () => {
    expect(isCappedReading(r('2026-08-04T18:32:50.000Z', 8000, 'probe'))).toBe(true);
    expect(isCappedReading(r('2026-08-04T18:32:50.000Z', 3143, 'probe'))).toBe(false);
    // The in-process detector has no ceiling — 8000 from it is a real duration.
    expect(isCappedReading(r('2026-08-04T18:32:50.000Z', 8000, 'in-process'))).toBe(false);
  });

  it('a storm of stalls becomes one episode per storm, not one per stall', () => {
    // The 08:30-09:02 storm: six freeze-pairs on a ~6-7min cadence.
    const burst: BlockReading[] = [];
    for (let i = 0; i < 20; i++) {
      burst.push(r(new Date(Date.UTC(2026, 7, 5, 12, 30, i * 2)).toISOString(), 3000, 'probe'));
    }
    const episodes = groupIntoEpisodes(burst, 30_000);
    expect(episodes).toHaveLength(1);
    expect(summarizeEpisode(episodes[0]).count).toBe(20);
  });

  // --- NEGATIVE PROOFS (#3734) ---

  it('NEGATIVE: genuinely separate incidents stay separate', () => {
    // If everything collapsed into one episode the grouping would be useless in
    // the other direction — a quiet hour then a new freeze is a NEW incident.
    const episodes = groupIntoEpisodes(
      [
        r('2026-08-04T18:32:50.000Z', 8000, 'probe'),
        r('2026-08-04T23:48:00.000Z', 20486, 'in-process', 'sendFile'),
      ],
      30_000,
    );
    expect(episodes).toHaveLength(2);
  });

  it('NEGATIVE: the window boundary is respected exactly', () => {
    const base = Date.UTC(2026, 7, 4, 18, 0, 0);
    const within = groupIntoEpisodes(
      [
        r(new Date(base).toISOString(), 1000, 'probe'),
        r(new Date(base + 29_999).toISOString(), 1000, 'probe'),
      ],
      30_000,
    );
    expect(within).toHaveLength(1);
    const beyond = groupIntoEpisodes(
      [
        r(new Date(base).toISOString(), 1000, 'probe'),
        r(new Date(base + 30_001).toISOString(), 1000, 'probe'),
      ],
      30_000,
    );
    expect(beyond).toHaveLength(2);
  });

  it('NEGATIVE: an empty reading set produces no episodes, not one empty one', () => {
    expect(groupIntoEpisodes([], 30_000)).toEqual([]);
  });

  it('the episode key is stable and derived from the episode start', () => {
    const k1 = episodeKey('2026-08-04T18:32:50.000Z');
    const k2 = episodeKey('2026-08-04T18:32:50.000Z');
    expect(k1).toBe(k2);
    expect(k1).not.toBe(episodeKey('2026-08-04T23:48:00.000Z'));
  });

  it('out-of-order readings still group correctly', () => {
    const episodes = groupIntoEpisodes(
      [
        r('2026-08-04T18:33:05.000Z', 19559, 'in-process', 'sendFile'),
        r('2026-08-04T18:32:50.000Z', 8000, 'probe'),
      ],
      30_000,
    );
    expect(episodes).toHaveLength(1);
    expect(summarizeEpisode(episodes[0]).worst_ms).toBe(19559);
  });
});

// --- The shared gate: what actually reaches Jeff ---
//
// Both detectors already throttle their nudges, but INDEPENDENTLY (5m each).
// So a single freeze still delivers two alarms — one per vantage — and neither
// throttle can see the other. The gate below is shared state: an episode that
// has already been announced does not get announced again, whoever saw it.
import { EpisodeGate } from '../src/eventloop-episode';

describe('#3742 shared episode gate', () => {
  it('the measured pair delivers ONE alarm, not one per detector', () => {
    const gate = new EpisodeGate(30_000);
    expect(gate.shouldAnnounce(r('2026-08-04T18:32:50.000Z', 8000, 'probe'))).toBe(true);
    // Same freeze, other vantage, 15s later — already announced.
    expect(gate.shouldAnnounce(r('2026-08-04T18:33:05.000Z', 19559, 'in-process', 'sendFile')))
      .toBe(false);
  });

  it('a 20-reading storm delivers ONE alarm', () => {
    const gate = new EpisodeGate(30_000);
    let announced = 0;
    for (let i = 0; i < 20; i++) {
      const reading = r(new Date(Date.UTC(2026, 7, 5, 12, 30, i * 2)).toISOString(), 3000, 'probe');
      if (gate.shouldAnnounce(reading)) announced++;
    }
    expect(announced).toBe(1);
  });

  it('NEGATIVE: a genuinely new incident DOES announce', () => {
    // The gate must not be a mute button. If this ever returns false, one early
    // freeze would silence every later one and the alarm would be worthless.
    const gate = new EpisodeGate(30_000);
    expect(gate.shouldAnnounce(r('2026-08-04T18:32:50.000Z', 8000, 'probe'))).toBe(true);
    expect(gate.shouldAnnounce(r('2026-08-04T23:48:00.000Z', 20486, 'in-process', 'sendFile')))
      .toBe(true);
  });

  it('NEGATIVE: the episode summary still counts everything it suppressed', () => {
    // Collapsing must not lose data — a storm has to READ like a storm.
    const gate = new EpisodeGate(30_000);
    gate.shouldAnnounce(r('2026-08-04T18:32:50.000Z', 8000, 'probe'));
    gate.shouldAnnounce(r('2026-08-04T18:33:05.000Z', 19559, 'in-process', 'sendFile'));
    const s = gate.currentSummary();
    expect(s?.count).toBe(2);
    expect(s?.worst_ms).toBe(19559);
    expect(s?.sources.sort()).toEqual(['in-process', 'probe']);
  });
});

// --- Cross-process gate ---
//
// The two detectors are separate PROCESSES: the in-process one lives inside
// chorus-api, the probe runs as its own LaunchAgent worker. An in-memory gate
// cannot span them, so the shared state has to live on disk. Without this the
// pair-collapse above would be true in tests and false in production — the
// exact "landed but not running" shape.
import { FileEpisodeGate } from '../src/eventloop-episode';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

describe('#3742 cross-process gate', () => {
  let statePath: string;
  beforeEach(() => {
    statePath = path.join(
      fs.mkdtempSync(path.join(os.tmpdir(), 'ep3742-')),
      'episode.json',
    );
  });

  it('two SEPARATE gate instances share one episode — one alarm', () => {
    // Simulates the real topology: chorus-api's detector and the probe worker.
    const inProcess = new FileEpisodeGate(statePath, 30_000);
    const probeWorker = new FileEpisodeGate(statePath, 30_000);
    expect(probeWorker.shouldAnnounce(r('2026-08-04T18:32:50.000Z', 8000, 'probe'))).toBe(true);
    expect(
      inProcess.shouldAnnounce(r('2026-08-04T18:33:05.000Z', 19559, 'in-process', 'sendFile')),
    ).toBe(false);
  });

  it('NEGATIVE: a new incident announces across processes too', () => {
    const a = new FileEpisodeGate(statePath, 30_000);
    const b = new FileEpisodeGate(statePath, 30_000);
    expect(a.shouldAnnounce(r('2026-08-04T18:32:50.000Z', 8000, 'probe'))).toBe(true);
    expect(b.shouldAnnounce(r('2026-08-04T23:48:00.000Z', 20486, 'in-process'))).toBe(true);
  });

  it('NEGATIVE: an unreadable state file ANNOUNCES rather than swallowing', () => {
    // Fail loud, not silent. If the shared state is broken the alarm must still
    // reach Jeff — a monitor that goes quiet on its own error is the worst case.
    const gate = new FileEpisodeGate('/nonexistent/dir/episode.json', 30_000);
    expect(gate.shouldAnnounce(r('2026-08-04T18:32:50.000Z', 8000, 'probe'))).toBe(true);
    expect(gate.shouldAnnounce(r('2026-08-04T18:32:52.000Z', 8000, 'probe'))).toBe(true);
  });

  it('corrupt state is treated as no state, not as a reason to go quiet', () => {
    fs.writeFileSync(statePath, '{not json');
    const gate = new FileEpisodeGate(statePath, 30_000);
    expect(gate.shouldAnnounce(r('2026-08-04T18:32:50.000Z', 8000, 'probe'))).toBe(true);
  });
});
