/**
 * Participants — unit tests (#2167 phase 1).
 *
 * Target: 80%+ on src/participants.ts. Most of the file is pure role
 * construction + prompt composition. The Anthropic SDK is mocked so we
 * can exercise getResponse paths without a network call.
 */

// Mock the Anthropic SDK so `new Anthropic()` returns a controllable stub.
const mockCreate = jest.fn();
const mockStreamOn = jest.fn();
const mockFinalMessage = jest.fn();
const mockStream = jest.fn(() => {
  const stream: any = { on: mockStreamOn, finalMessage: mockFinalMessage };
  return stream;
});

jest.mock('@anthropic-ai/sdk', () => {
  return jest.fn().mockImplementation(() => ({
    messages: {
      create: mockCreate,
      stream: mockStream,
    },
  }));
});

import { Participants } from '../src/participants';

beforeEach(() => {
  mockCreate.mockReset();
  mockStreamOn.mockReset();
  mockFinalMessage.mockReset();
  mockStream.mockClear();
});

describe('Participants — role construction', () => {
  test('constructor produces three roles: Wren, Silas, Kade', () => {
    const p = new Participants('claude-haiku-4-5-20251001', 1000);
    const names = p.getRoles().map((r) => r.name).sort();
    expect(names).toEqual(['Kade', 'Silas', 'Wren']);
  });

  test('default (non-guest) includes grounding rules text', () => {
    const p = new Participants('claude-haiku-4-5-20251001', 1000);
    for (const role of p.getRoles()) {
      expect(role.systemPrompt).toContain('Grounding Rules (CRITICAL)');
      expect(role.systemPrompt).not.toContain('Guest Session Rules');
    }
  });

  test('guestMode=true swaps grounding rules for guest rules', () => {
    const p = new Participants('claude-haiku-4-5-20251001', 1000, undefined, true);
    for (const role of p.getRoles()) {
      expect(role.systemPrompt).toContain('Guest Session Rules');
      expect(role.systemPrompt).not.toContain('Grounding Rules (CRITICAL)');
    }
  });

  test('session context is appended to each role prompt', () => {
    const ctx = 'We are focused on #2167 coverage work.';
    const p = new Participants('claude-haiku-4-5-20251001', 1000, ctx);
    for (const role of p.getRoles()) {
      expect(role.systemPrompt).toContain('## Session Context');
      expect(role.systemPrompt).toContain(ctx);
    }
  });

  test('whitespace-only session context is treated as empty', () => {
    const p = new Participants('claude-haiku-4-5-20251001', 1000, '   \n ');
    for (const role of p.getRoles()) {
      expect(role.systemPrompt).not.toContain('## Session Context');
    }
  });

  test('session context is trimmed when appended', () => {
    const p = new Participants('claude-haiku-4-5-20251001', 1000, '\n  trimmed  \n');
    expect(p.getRoles()[0].systemPrompt).toContain('trimmed');
    expect(p.getRoles()[0].systemPrompt).not.toMatch(/Context\n\n.*\n {2}trimmed/);
  });
});

describe('Participants — role lookup', () => {
  test('getRoleByName finds roles case-insensitively', () => {
    const p = new Participants('claude-haiku-4-5-20251001', 1000);
    expect(p.getRoleByName('wren')?.title).toBe('Product Manager');
    expect(p.getRoleByName('SILAS')?.title).toBe('Architect');
    expect(p.getRoleByName('Kade')?.title).toBe('Engineer');
  });

  test('getRoleByName returns undefined for unknown role', () => {
    const p = new Participants('claude-haiku-4-5-20251001', 1000);
    expect(p.getRoleByName('jeff')).toBeUndefined();
    expect(p.getRoleByName('')).toBeUndefined();
  });
});

describe('Participants — context + mode updates', () => {
  test('updateContext rewrites all role prompts', () => {
    const p = new Participants('claude-haiku-4-5-20251001', 1000);
    expect(p.getRoles()[0].systemPrompt).not.toContain('fresh context');
    p.updateContext('fresh context block');
    for (const role of p.getRoles()) {
      expect(role.systemPrompt).toContain('## Session Context');
      expect(role.systemPrompt).toContain('fresh context block');
    }
  });

  test('setGuestMode(true) switches to guest rules', () => {
    const p = new Participants('claude-haiku-4-5-20251001', 1000);
    p.setGuestMode(true);
    for (const role of p.getRoles()) {
      expect(role.systemPrompt).toContain('Guest Session Rules');
    }
  });

  test('setGuestMode(false) restores default grounding', () => {
    const p = new Participants('claude-haiku-4-5-20251001', 1000, undefined, true);
    p.setGuestMode(false);
    for (const role of p.getRoles()) {
      expect(role.systemPrompt).toContain('Grounding Rules (CRITICAL)');
      expect(role.systemPrompt).not.toContain('Guest Session Rules');
    }
  });
});

describe('Participants — getResponse (non-streaming)', () => {
  test('calls Anthropic create with role prompt and transcript', async () => {
    mockCreate.mockResolvedValue({
      content: [{ type: 'text', text: 'hello from wren' }],
      usage: { input_tokens: 42, output_tokens: 7 },
    });
    const p = new Participants('claude-haiku-4-5-20251001', 500);
    const wren = p.getRoleByName('wren')!;
    const result = await p.getResponse(wren, [
      { id: '1', sender: 'jeff', content: 'hi team', timestamp: 0 },
    ]);
    expect(mockCreate).toHaveBeenCalledWith(expect.objectContaining({
      model: 'claude-haiku-4-5-20251001',
      max_tokens: 500,
      system: wren.systemPrompt,
    }));
    expect(result.content).toBe('hello from wren');
    expect(result.inputTokens).toBe(42);
    expect(result.outputTokens).toBe(7);
  });

  test('filters non-text blocks from the response', async () => {
    mockCreate.mockResolvedValue({
      content: [
        { type: 'tool_use', name: 'x' },
        { type: 'text', text: 'first ' },
        { type: 'text', text: 'second' },
      ],
      usage: { input_tokens: 1, output_tokens: 2 },
    });
    const p = new Participants('claude-haiku-4-5-20251001', 500);
    const result = await p.getResponse(p.getRoles()[0], []);
    expect(result.content).toBe('first second');
  });

  test('empty message history renders as "empty chat" preamble', async () => {
    mockCreate.mockResolvedValue({
      content: [{ type: 'text', text: 'ack' }],
      usage: { input_tokens: 1, output_tokens: 1 },
    });
    const p = new Participants('claude-haiku-4-5-20251001', 500);
    await p.getResponse(p.getRoles()[0], []);
    const userContent = mockCreate.mock.calls[0][0].messages[0].content;
    expect(userContent).toContain('(empty chat — you are the first to speak)');
  });

  test('prefixes the current role with "(you)" in the transcript', async () => {
    mockCreate.mockResolvedValue({
      content: [{ type: 'text', text: 'ok' }],
      usage: { input_tokens: 1, output_tokens: 1 },
    });
    const p = new Participants('claude-haiku-4-5-20251001', 500);
    const wren = p.getRoleByName('wren')!;
    await p.getResponse(wren, [
      { id: '1', sender: 'Wren', content: 'I said this', timestamp: 0 },
      { id: '2', sender: 'Kade', content: 'ack', timestamp: 1 },
    ]);
    const userContent = mockCreate.mock.calls[0][0].messages[0].content;
    expect(userContent).toContain('[Wren (you)]: I said this');
    expect(userContent).toContain('[Kade]: ack');
  });
});

describe('Participants — getResponse (streaming)', () => {
  test('streaming path invokes onToken per text chunk and returns final usage', async () => {
    const tokens: string[] = [];
    let textHandler: ((t: string) => void) | undefined;
    mockStreamOn.mockImplementation((event: string, cb: any) => {
      if (event === 'text') textHandler = cb;
    });
    mockFinalMessage.mockImplementation(() => {
      // Simulate text streaming before resolving finalMessage.
      textHandler?.('hel');
      textHandler?.('lo');
      return Promise.resolve({
        content: [{ type: 'text', text: 'hello' }],
        usage: { input_tokens: 3, output_tokens: 2 },
      });
    });

    const p = new Participants('claude-haiku-4-5-20251001', 500);
    const result = await p.getResponse(p.getRoles()[0], [], (t) => tokens.push(t));

    expect(mockStream).toHaveBeenCalled();
    expect(tokens).toEqual(['hel', 'lo']);
    expect(result.content).toBe('hello');
    expect(result.inputTokens).toBe(3);
    expect(result.outputTokens).toBe(2);
  });
});
