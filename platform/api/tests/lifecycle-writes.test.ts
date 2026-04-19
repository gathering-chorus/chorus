import { handlePulse, handleRoleState, handleAlert } from '../src/lifecycle-writes';

function fakeSink() {
  const writes: Array<{ path: string; data: string; mode: 'append' | 'write' }> = [];
  return {
    writes,
    appendFileSync: (p: string, d: string) => { writes.push({ path: p, data: d, mode: 'append' }); },
    writeFileSync: (p: string, d: string) => { writes.push({ path: p, data: d, mode: 'write' }); },
  };
}

function fakeRes() {
  const self: any = {};
  self.status_ = 200;
  self.body_ = null;
  self.status = (s: number) => { self.status_ = s; return self; };
  self.json = (b: any) => { self.body_ = b; return self; };
  return self;
}

describe('handlePulse', () => {
  it('400s when event is missing', () => {
    const sink = fakeSink();
    const res = fakeRes();
    handlePulse(
      { body: { role: 'kade' } } as any,
      res,
      { appendFileSync: sink.appendFileSync as any, chorusLogPath: '/log', now: () => 't' },
    );
    expect(res.status_).toBe(400);
    expect(res.body_.error).toMatch(/event and role/);
    expect(sink.writes).toHaveLength(0);
  });

  it('400s when role is missing', () => {
    const sink = fakeSink();
    const res = fakeRes();
    handlePulse(
      { body: { event: 'x' } } as any,
      res,
      { appendFileSync: sink.appendFileSync as any, chorusLogPath: '/log', now: () => 't' },
    );
    expect(res.status_).toBe(400);
  });

  it('writes a spine-line and responds ok on a valid pulse', () => {
    const sink = fakeSink();
    const res = fakeRes();
    handlePulse(
      { body: { event: 'card.pulled', role: 'kade', level: 'info', card: 42 } } as any,
      res,
      { appendFileSync: sink.appendFileSync as any, chorusLogPath: '/path/chorus.log', now: () => '2026-04-19 12:00:00' },
    );
    expect(res.body_.ok).toBe(true);
    expect(sink.writes).toHaveLength(1);
    const entry = JSON.parse(sink.writes[0].data.trim());
    expect(entry.event).toBe('card.pulled');
    expect(entry.role).toBe('kade');
    expect(entry.card).toBe('42');
    expect(entry.level).toBe('info');
  });

  it('defaults invalid level to info', () => {
    const sink = fakeSink();
    const res = fakeRes();
    handlePulse(
      { body: { event: 'x', role: 'r', level: 'nonsense' } } as any,
      res,
      { appendFileSync: sink.appendFileSync as any, chorusLogPath: '/log', now: () => 't' },
    );
    const entry = JSON.parse(sink.writes[0].data.trim());
    expect(entry.level).toBe('info');
    expect(res.body_.level).toBe('info');
  });

  it('coerces number and string extras into the spine entry, drops objects', () => {
    const sink = fakeSink();
    const res = fakeRes();
    handlePulse(
      { body: { event: 'e', role: 'r', card: 7, domain: 'chorus', nested: { x: 1 } } } as any,
      res,
      { appendFileSync: sink.appendFileSync as any, chorusLogPath: '/log', now: () => 't' },
    );
    const entry = JSON.parse(sink.writes[0].data.trim());
    expect(entry.card).toBe('7');
    expect(entry.domain).toBe('chorus');
    expect(entry.nested).toBeUndefined();
  });
});

describe('handleRoleState', () => {
  it('400s when role or state missing', () => {
    const sink = fakeSink();
    const res = fakeRes();
    handleRoleState(
      { body: { role: 'kade' } } as any,
      res,
      { appendFileSync: sink.appendFileSync as any, writeFileSync: sink.writeFileSync as any, chorusLogPath: '/log' },
    );
    expect(res.status_).toBe(400);
  });

  it('400s on invalid state value', () => {
    const sink = fakeSink();
    const res = fakeRes();
    handleRoleState(
      { body: { role: 'kade', state: 'gossiping' } } as any,
      res,
      { appendFileSync: sink.appendFileSync as any, writeFileSync: sink.writeFileSync as any, chorusLogPath: '/log' },
    );
    expect(res.status_).toBe(400);
    expect(res.body_.error).toMatch(/Invalid state/);
  });

  it('writes role-state file + spine line on valid building state', () => {
    const sink = fakeSink();
    const res = fakeRes();
    handleRoleState(
      { body: { role: 'kade', state: 'building', card: 2205, type: 'enhance' } } as any,
      res,
      { appendFileSync: sink.appendFileSync as any, writeFileSync: sink.writeFileSync as any, chorusLogPath: '/log' },
    );
    const writeCall = sink.writes.find(w => w.mode === 'write');
    expect(writeCall!.path).toBe('/tmp/role-state-kade.json');
    const parsed = JSON.parse(writeCall!.data);
    expect(parsed.role).toBe('kade');
    expect(parsed.state).toBe('building');
    expect(parsed.card).toBe(2205);
    const appendCall = sink.writes.find(w => w.mode === 'append');
    expect(appendCall).toBeDefined();
    expect(res.body_).toEqual({ ok: true, role: 'kade', state: 'building', card: 2205 });
  });

  it('passes null for card when absent', () => {
    const sink = fakeSink();
    const res = fakeRes();
    handleRoleState(
      { body: { role: 'wren', state: 'idle' } } as any,
      res,
      { appendFileSync: sink.appendFileSync as any, writeFileSync: sink.writeFileSync as any, chorusLogPath: '/log' },
    );
    expect(res.body_.card).toBeNull();
    const parsed = JSON.parse(sink.writes.find(w => w.mode === 'write')!.data);
    expect(parsed.card).toBeNull();
  });
});

describe('handleAlert', () => {
  it('responds received=0 on empty alerts', () => {
    const sink = fakeSink();
    const res = fakeRes();
    const notify = jest.fn();
    handleAlert(
      { body: { alerts: [] } } as any,
      res,
      { appendFileSync: sink.appendFileSync as any, notify, chorusLogPath: '/log' },
    );
    expect(res.body_).toEqual({ received: 0 });
  });

  it('writes a spine line per alert', () => {
    const sink = fakeSink();
    const res = fakeRes();
    handleAlert(
      {
        body: {
          alerts: [
            { labels: { alertname: 'a', severity: 'warning' }, annotations: { summary: 's1' }, status: 'firing' },
            { labels: { alertname: 'b', severity: 'critical' }, annotations: { description: 'd2' }, status: 'resolved' },
          ],
        },
      } as any,
      res,
      { appendFileSync: sink.appendFileSync as any, notify: jest.fn(), chorusLogPath: '/log' },
    );
    expect(sink.writes).toHaveLength(2);
    const first = JSON.parse(sink.writes[0].data.trim());
    expect(first.alertname).toBe('a');
    expect(first.level).toBe('warn');
    expect(first.event).toBe('alert_firing');
  });

  it('fires desktop notification only for critical+firing alerts', () => {
    const sink = fakeSink();
    const res = fakeRes();
    const notify = jest.fn();
    handleAlert(
      {
        body: {
          alerts: [
            { labels: { alertname: 'a', severity: 'warning' }, annotations: {}, status: 'firing' },
            { labels: { alertname: 'b', severity: 'critical' }, annotations: { summary: 'crit' }, status: 'resolved' },
            { labels: { alertname: 'c', severity: 'critical' }, annotations: { summary: 'real' }, status: 'firing' },
          ],
        },
      } as any,
      res,
      { appendFileSync: sink.appendFileSync as any, notify, chorusLogPath: '/log' },
    );
    expect(notify).toHaveBeenCalledTimes(1);
    expect(notify.mock.calls[0][0]).toContain('ALERT: c');
    expect(notify.mock.calls[0][1]).toBe('real');
  });

  it('truncates description to 500 chars in spine entries', () => {
    const sink = fakeSink();
    const res = fakeRes();
    const big = 'x'.repeat(800);
    handleAlert(
      { body: { alerts: [{ labels: { severity: 'warning' }, annotations: { description: big }, status: 'firing' }] } } as any,
      res,
      { appendFileSync: sink.appendFileSync as any, notify: jest.fn(), chorusLogPath: '/log' },
    );
    const entry = JSON.parse(sink.writes[0].data.trim());
    expect(entry.description.length).toBe(500);
  });

  it('handles missing alerts array gracefully', () => {
    const sink = fakeSink();
    const res = fakeRes();
    handleAlert(
      { body: {} } as any,
      res,
      { appendFileSync: sink.appendFileSync as any, notify: jest.fn(), chorusLogPath: '/log' },
    );
    expect(res.body_).toEqual({ received: 0 });
  });
});
