/**
 * ClearingChat — unit tests (#2167 phase 2).
 *
 * Target: 80%+ on src/chat.ts. Socket.IO server, Participants (Anthropic),
 * and execSync (nudge binary) are all mocked at module boundaries.
 */

const mockExecSync = jest.fn();
jest.mock('child_process', () => ({ execSync: mockExecSync }));

// Mock Participants so we don't need the Anthropic SDK.
class MockParticipants {
  roles = [
    { name: 'Wren', title: 'PM', color: '#4ade80', systemPrompt: 'wren prompt' },
    { name: 'Silas', title: 'Arch', color: '#60a5fa', systemPrompt: 'silas prompt' },
    { name: 'Kade', title: 'Eng', color: '#fb923c', systemPrompt: 'kade prompt' },
  ];
  constructor(_model: string, _max: number, _ctx?: string, _guest?: boolean) {}
  getRoles() { return this.roles; }
  updateContext() {}
  setGuestMode() {}
  getRoleByName(n: string) { return this.roles.find((r) => r.name.toLowerCase() === n.toLowerCase()); }
  getResponse = jest.fn();
}

jest.mock('../src/participants', () => ({
  Participants: MockParticipants,
}));

import { ClearingChat } from '../src/chat';

// Minimal Socket.IO server stub.
function makeIo() {
  const events: Array<{ channel: string; payload: any }> = [];
  return {
    emit: jest.fn((channel: string, payload: any) => { events.push({ channel, payload }); }),
    _events: events,
  };
}

beforeEach(() => {
  mockExecSync.mockReset();
});

describe('ClearingChat — sessions', () => {
  test('startSession returns a sessionId and flips active', () => {
    const io = makeIo();
    const chat = new ClearingChat(io as any);
    const { sessionId } = chat.startSession();
    expect(sessionId).toMatch(/^clearing-\d+$/);
    expect(chat.getState().active).toBe(true);
    expect(chat.getState().sessionId).toBe(sessionId);
  });

  test('startSession with context updates participants', () => {
    const io = makeIo();
    const chat = new ClearingChat(io as any);
    const spy = jest.spyOn((chat as any).participants, 'updateContext');
    chat.startSession('new ctx');
    expect(spy).toHaveBeenCalledWith('new ctx');
  });

  test('starting a new session with existing messages ends the prior one', () => {
    const io = makeIo();
    const chat = new ClearingChat(io as any);
    chat.startSession();
    (chat as any).transcript.add('Jeff', 'hi');  // pretend there's a message
    const endSpy = jest.spyOn(chat, 'endSession');
    chat.startSession();
    expect(endSpy).toHaveBeenCalledWith('new-session');
  });

  test('endSession on an inactive session is a no-op', () => {
    const io = makeIo();
    const chat = new ClearingChat(io as any);
    expect(() => chat.endSession('test')).not.toThrow();
    expect(chat.getState().active).toBe(false);
  });

  test('endSession resets transcript for next session', () => {
    const io = makeIo();
    const chat = new ClearingChat(io as any);
    chat.startSession();
    (chat as any).transcript.add('Jeff', 'hi');
    chat.endSession('test');
    expect(chat.getState().messageCount).toBe(0);
  });
});

describe('ClearingChat — handleMessage: nudge shortcut commands', () => {
  test('/nudge wren ... fires executeNudge and records system message', async () => {
    mockExecSync.mockReturnValue('DELIVERED to wren at 2026-04-17 16:00');
    const io = makeIo();
    const chat = new ClearingChat(io as any);
    chat.startSession();
    await chat.handleMessage('/nudge wren Check #2167 please');
    expect(mockExecSync).toHaveBeenCalled();
    const cmd = (mockExecSync.mock.calls[0][0] as string);
    expect(cmd).toContain('nudge wren');
    expect(cmd).toContain('--from jeff');
    // One jeff message + one system-success message
    const channels = io._events.map((e) => e.channel);
    expect(channels.filter((c) => c === 'chat:message')).toHaveLength(2);
  });

  test('shorthand "nw msg" routes to wren', async () => {
    mockExecSync.mockReturnValue('ok');
    const io = makeIo();
    const chat = new ClearingChat(io as any);
    chat.startSession();
    await chat.handleMessage('nw quick ping');
    expect(mockExecSync).toHaveBeenCalled();
    expect((mockExecSync.mock.calls[0][0] as string)).toContain('nudge wren');
  });

  test('shorthand "ns" routes to silas, "nk" routes to kade', async () => {
    mockExecSync.mockReturnValue('ok');
    const io = makeIo();
    const chat = new ClearingChat(io as any);
    chat.startSession();
    await chat.handleMessage('ns arch question');
    await chat.handleMessage('nk build status?');
    const cmds = mockExecSync.mock.calls.map((c) => c[0] as string);
    expect(cmds.some((c) => c.includes('nudge silas'))).toBe(true);
    expect(cmds.some((c) => c.includes('nudge kade'))).toBe(true);
  });

  test('nudge failure records failure-detail system message', async () => {
    mockExecSync.mockImplementation(() => { throw new Error('spawn failed'); });
    const io = makeIo();
    const chat = new ClearingChat(io as any);
    chat.startSession();
    await chat.handleMessage('/nudge kade failing test');
    // Find the second chat:message (the system one)
    const msgs = io._events.filter((e) => e.channel === 'chat:message').map((e) => e.payload);
    expect(msgs.length).toBeGreaterThanOrEqual(2);
    const sys = msgs[msgs.length - 1];
    expect(sys.sender).toBe('System');
    expect(sys.content).toMatch(/failed/i);
  });
});

describe('ClearingChat — handleMessage: role responses', () => {
  function makeMockResponse(content: string) {
    return { content, inputTokens: 5, outputTokens: 3 };
  }

  test('unaddressed message triggers all three roles', async () => {
    const io = makeIo();
    const chat = new ClearingChat(io as any);
    chat.startSession();
    (chat as any).participants.getResponse.mockResolvedValue(makeMockResponse('ok'));
    await chat.handleMessage('hello team');
    expect((chat as any).participants.getResponse).toHaveBeenCalledTimes(3);
  });

  test('@mention routes only to mentioned role', async () => {
    const io = makeIo();
    const chat = new ClearingChat(io as any);
    chat.startSession();
    (chat as any).participants.getResponse.mockResolvedValue(makeMockResponse('noted'));
    await chat.handleMessage('@silas architecture question');
    expect((chat as any).participants.getResponse).toHaveBeenCalledTimes(1);
    const role = (chat as any).participants.getResponse.mock.calls[0][0];
    expect(role.name).toBe('Silas');
  });

  test('activeRoles filter applied when no @mention', async () => {
    const io = makeIo();
    const chat = new ClearingChat(io as any);
    chat.startSession();
    (chat as any).participants.getResponse.mockResolvedValue(makeMockResponse('ok'));
    await chat.handleMessage('any thoughts', ['kade']);
    expect((chat as any).participants.getResponse).toHaveBeenCalledTimes(1);
    const role = (chat as any).participants.getResponse.mock.calls[0][0];
    expect(role.name).toBe('Kade');
  });

  test('[pass] response does not record a transcript message', async () => {
    const io = makeIo();
    const chat = new ClearingChat(io as any);
    chat.startSession();
    (chat as any).participants.getResponse.mockResolvedValue(makeMockResponse('[pass]'));
    await chat.handleMessage('@kade thoughts?');
    expect(io.emit).toHaveBeenCalledWith('chat:typed', { sender: 'Kade', passed: true });
    // Only jeff's message should be in transcript
    const msgs = chat.getMessages();
    expect(msgs).toHaveLength(1);
    expect(msgs[0].sender).toBe('Jeff');
  });

  test('case-insensitive [PASS] prefix also passes', async () => {
    const io = makeIo();
    const chat = new ClearingChat(io as any);
    chat.startSession();
    (chat as any).participants.getResponse.mockResolvedValue(makeMockResponse('[PASS] nothing to add'));
    await chat.handleMessage('@wren thoughts?');
    expect(io.emit).toHaveBeenCalledWith('chat:typed', { sender: 'Wren', passed: true });
  });

  test('extractNudges from response content strips nudge lines and fires nudges', async () => {
    const io = makeIo();
    const chat = new ClearingChat(io as any);
    chat.startSession();
    mockExecSync.mockReturnValue('ok');
    (chat as any).participants.getResponse.mockResolvedValue(makeMockResponse(
      'I think we should deploy.\n/nudge silas review ops gate'
    ));
    await chat.handleMessage('@wren what next?');
    // Nudge fired
    const nudgeCall = mockExecSync.mock.calls.find((c) => (c[0] as string).includes('nudge silas'));
    expect(nudgeCall).toBeDefined();
    // Transcript has Wren's cleaned content (no /nudge line)
    const wrenMsg = chat.getMessages().find((m) => m.sender === 'Wren');
    expect(wrenMsg?.content).toBe('I think we should deploy.');
  });

  test('role response error emits chat:typed with System sender', async () => {
    const io = makeIo();
    const chat = new ClearingChat(io as any);
    chat.startSession();
    (chat as any).participants.getResponse.mockRejectedValue(new Error('API down'));
    await chat.handleMessage('@silas thoughts?');
    const errEvents = io._events.filter(
      (e) => e.channel === 'chat:typed' && e.payload?.message?.content?.includes('failed')
    );
    expect(errEvents).toHaveLength(1);
    expect(errEvents[0].payload.message.content).toContain('Silas failed');
    expect(errEvents[0].payload.message.content).toContain('API down');
  });

  test('cost event emitted after each role response', async () => {
    const io = makeIo();
    const chat = new ClearingChat(io as any);
    chat.startSession();
    (chat as any).participants.getResponse.mockResolvedValue(makeMockResponse('ok'));
    await chat.handleMessage('@wren hi');
    const costEvents = io._events.filter((e) => e.channel === 'chat:cost');
    expect(costEvents).toHaveLength(1);
    expect(costEvents[0].payload).toHaveProperty('totalTokens');
    expect(costEvents[0].payload).toHaveProperty('estimatedCost');
  });
});

describe('ClearingChat — decision capture', () => {
  test('DECISION: prefix emits chat:decision with messageId and text', async () => {
    const io = makeIo();
    const chat = new ClearingChat(io as any);
    chat.startSession();
    (chat as any).participants.getResponse.mockResolvedValue({ content: 'ok', inputTokens: 1, outputTokens: 1 });
    await chat.handleMessage('DECISION: ship #2167 tonight');
    const dec = io._events.find((e) => e.channel === 'chat:decision');
    expect(dec).toBeDefined();
    expect(dec!.payload.text).toContain('DECISION: ship #2167');
    expect(dec!.payload.speaker).toBe('Jeff');
  });

  test('non-decision does not emit chat:decision', async () => {
    const io = makeIo();
    const chat = new ClearingChat(io as any);
    chat.startSession();
    (chat as any).participants.getResponse.mockResolvedValue({ content: 'ok', inputTokens: 1, outputTokens: 1 });
    await chat.handleMessage('just a thought');
    expect(io._events.find((e) => e.channel === 'chat:decision')).toBeUndefined();
  });
});

describe('ClearingChat — getState and getMessages', () => {
  test('getState includes messageCount and role metadata', () => {
    const io = makeIo();
    const chat = new ClearingChat(io as any);
    const s = chat.getState();
    expect(s.messageCount).toBe(0);
    expect(s.roles.map((r: any) => r.name)).toEqual(['Wren', 'Silas', 'Kade']);
    expect(s.roles[0]).toHaveProperty('color');
  });

  test('getMessages sinceId filters by integer id', () => {
    const io = makeIo();
    const chat = new ClearingChat(io as any);
    chat.startSession();
    (chat as any).transcript.add('Jeff', 'a');
    (chat as any).transcript.add('Jeff', 'b');
    (chat as any).transcript.add('Jeff', 'c');
    expect(chat.getMessages(1)).toHaveLength(2);
    expect(chat.getMessages(0)).toHaveLength(3);
    expect(chat.getMessages()).toHaveLength(3);
  });
});

describe('ClearingChat — auto-save lifecycle', () => {
  beforeEach(() => jest.useFakeTimers());
  afterEach(() => jest.useRealTimers());

  test('startSession enables auto-save timer', () => {
    const io = makeIo();
    const chat = new ClearingChat(io as any);
    chat.startSession();
    expect((chat as any).autoSaveInterval).not.toBeNull();
  });

  test('endSession clears the auto-save timer', () => {
    const io = makeIo();
    const chat = new ClearingChat(io as any);
    chat.startSession();
    chat.endSession('test');
    expect((chat as any).autoSaveInterval).toBeNull();
  });

  test('startAutoSave is idempotent (no duplicate timers)', () => {
    const io = makeIo();
    const chat = new ClearingChat(io as any);
    chat.startSession();
    const first = (chat as any).autoSaveInterval;
    (chat as any).startAutoSave();
    expect((chat as any).autoSaveInterval).toBe(first);
  });
});
