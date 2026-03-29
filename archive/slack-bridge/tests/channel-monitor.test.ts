import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { ChannelMonitor } from '../src/channel-monitor';

// Mock Slack WebClient
const mockConversationsList = jest.fn();
const mockConversationsHistory = jest.fn();
const mockAuthTest = jest.fn();

const mockSlack = {
  auth: { test: mockAuthTest },
  conversations: {
    list: mockConversationsList,
    history: mockConversationsHistory,
  },
} as any;

describe('ChannelMonitor', () => {
  let tmpDir: string;
  let monitor: ChannelMonitor;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'bridge-monitor-'));
    const statePath = path.join(tmpDir, 'last-seen.json');

    mockAuthTest.mockResolvedValue({ user_id: 'UBOT123' });
    mockConversationsList.mockResolvedValue({
      channels: [
        { name: 'silas', id: 'C_SILAS' },
        { name: 'wren', id: 'C_WREN' },
        { name: 'kade', id: 'C_KADE' },
        { name: 'all-gathering', id: 'C_ALL' },
      ],
    });

    monitor = new ChannelMonitor(mockSlack, statePath);
    jest.clearAllMocks();

    // Re-setup mocks after clearAllMocks
    mockAuthTest.mockResolvedValue({ user_id: 'UBOT123' });
    mockConversationsList.mockResolvedValue({
      channels: [
        { name: 'silas', id: 'C_SILAS' },
        { name: 'wren', id: 'C_WREN' },
        { name: 'kade', id: 'C_KADE' },
        { name: 'all-gathering', id: 'C_ALL' },
      ],
    });
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('initializes with bot user ID and channel map', async () => {
    await monitor.initialize();
    expect(mockAuthTest).toHaveBeenCalled();
    expect(mockConversationsList).toHaveBeenCalled();
    expect(monitor.getChannelId('silas')).toBe('C_SILAS');
    expect(monitor.getChannelId('all-gathering')).toBe('C_ALL');
  });

  it('returns new messages from poll', async () => {
    await monitor.initialize();

    mockConversationsHistory.mockResolvedValue({
      messages: [
        { text: 'hello', user: 'U_JEFF', ts: '1000.001' },
      ],
    });

    const messages = await monitor.poll(['silas']);
    expect(messages).toHaveLength(1);
    expect(messages[0].text).toBe('hello');
    expect(messages[0].channelName).toBe('silas');
  });

  it('filters out bridge-originated messages by marker', async () => {
    await monitor.initialize();

    mockConversationsHistory.mockResolvedValue({
      messages: [
        { text: '**Wren**: Here is my response\n··bridge', user: 'UBOT123', ts: '1000.002' },
        { text: 'human message', user: 'U_JEFF', ts: '1000.001' },
      ],
    });

    const messages = await monitor.poll(['silas']);
    expect(messages).toHaveLength(1);
    expect(messages[0].text).toBe('human message');
  });

  it('passes through role messages without bridge marker', async () => {
    await monitor.initialize();

    mockConversationsHistory.mockResolvedValue({
      messages: [
        { text: '**Wren**: Check the brief I sent', user: 'UBOT123', ts: '1000.002' },
        { text: 'hey silas', user: 'U_JEFF', ts: '1000.001' },
      ],
    });

    const messages = await monitor.poll(['silas']);
    expect(messages).toHaveLength(2);
  });

  it('persists last-seen state', async () => {
    await monitor.initialize();

    mockConversationsHistory.mockResolvedValue({
      messages: [{ text: 'msg', user: 'U_JEFF', ts: '2000.001' }],
    });

    await monitor.poll(['silas']);

    // Verify state file exists
    const statePath = path.join(tmpDir, 'last-seen.json');
    expect(fs.existsSync(statePath)).toBe(true);
    const state = JSON.parse(fs.readFileSync(statePath, 'utf-8'));
    expect(state['C_SILAS']).toBe('2000.001');
  });

  it('skips unknown channels', async () => {
    await monitor.initialize();
    const messages = await monitor.poll(['nonexistent']);
    expect(messages).toHaveLength(0);
  });
});
