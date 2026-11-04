// @test-type: unit — Clearing v2 composer targeting and validation (#4877 AC1, AC2, AC5, AC6)
// @card: #4877
// @owner: abby-normal
// @domain: messages
/**
 * Tests for composer targeting, bare prefill refusal, and mic error surfacing (#4877 AC1, AC2, AC5, AC6).
 */
import type { ClearingMessage } from '../src/types';
const app = require('../public/app.js');

describe('composer targeting and validation (#4877 AC1, AC2, AC5, AC6)', () => {
  beforeEach(() => {
    app.setRoleFilter(null);
  });

  describe('AC6: bare role mentions / empty input refusal', () => {
    it('AC6: refuses to send when input is empty or only whitespace, showing error in statusEl', async () => {
      const statusEl = { textContent: '', style: { display: 'none' } };
      const inputEl = { value: '   ' };
      const fetchFn = jest.fn();

      await app.handleSendMessage('', { inputEl, statusEl, fetchFn });
      expect(fetchFn).not.toHaveBeenCalled();
      expect(statusEl.textContent).toMatch(/Add a message before sending/i);
      expect(statusEl.style.display).toBe('block');
    });

    it('AC6: refuses to send when input contains only bare role mention (e.g. "@wren")', async () => {
      const statusEl = { textContent: '', style: { display: 'none' } };
      const inputEl = { value: '@wren' };
      const fetchFn = jest.fn();

      await app.handleSendMessage('@wren', { inputEl, statusEl, fetchFn });
      expect(fetchFn).not.toHaveBeenCalled();
      expect(statusEl.textContent).toMatch(/Add a message before sending/i);
      expect(statusEl.style.display).toBe('block');
    });

    it('AC6: refuses to send when input contains multiple bare mentions (e.g. "@wren @silas")', async () => {
      const statusEl = { textContent: '', style: { display: 'none' } };
      const inputEl = { value: '@wren @silas ' };
      const fetchFn = jest.fn();

      await app.handleSendMessage('@wren @silas ', { inputEl, statusEl, fetchFn });
      expect(fetchFn).not.toHaveBeenCalled();
      expect(statusEl.textContent).toMatch(/Add a message before sending/i);
    });

    it('AC6: allows send when message contains actual text after role mention', async () => {
      const statusEl = { textContent: '', style: { display: 'none' } };
      const inputEl = { value: '@wren please check the logs' };
      const fetchFn = jest.fn().mockResolvedValue({
        ok: true,
        json: async () => ({ ok: true, stored: true }),
      });

      await app.handleSendMessage('@wren please check the logs', { inputEl, statusEl, fetchFn });
      expect(fetchFn).toHaveBeenCalled();
      expect(statusEl.textContent).toBe('');
    });
  });

  describe('AC2: handleSendMessage preserves actor filter targeting', () => {
    it('AC2: prepends active role filter if message text does not start with @', async () => {
      app.setRoleFilter('silas');
      const inputEl = { value: 'check the memory pressure' };
      const statusEl = { textContent: '', style: { display: 'none' } };
      let sentBody: any = null;
      const fetchFn = jest.fn().mockImplementation(async (url, init) => {
        sentBody = JSON.parse(init.body);
        return { ok: true, json: async () => ({ ok: true, stored: true }) };
      });

      await app.handleSendMessage('check the memory pressure', { inputEl, statusEl, fetchFn });
      expect(fetchFn).toHaveBeenCalled();
      expect(sentBody).not.toBeNull();
      expect(sentBody.text).toBe('@silas check the memory pressure');
    });

    it('AC2: does not double-prepend if message already starts with @silas', async () => {
      app.setRoleFilter('silas');
      const inputEl = { value: '@silas check the memory pressure' };
      const statusEl = { textContent: '', style: { display: 'none' } };
      let sentBody: any = null;
      const fetchFn = jest.fn().mockImplementation(async (url, init) => {
        sentBody = JSON.parse(init.body);
        return { ok: true, json: async () => ({ ok: true, stored: true }) };
      });

      await app.handleSendMessage('@silas check the memory pressure', { inputEl, statusEl, fetchFn });
      expect(fetchFn).toHaveBeenCalled();
      expect(sentBody).not.toBeNull();
      expect(sentBody.text).toBe('@silas check the memory pressure');
    });
  });

  describe('AC5: getUserMedia / MediaRecorder failure surfaces visibly', () => {
    it('AC5: displays visible error when getUserMedia rejects (e.g. Permission Denied)', async () => {
      const errorEl = { textContent: '', innerHTML: '' };
      const micEl: any = {
        addEventListener: jest.fn(),
        dataset: {},
        setAttribute: jest.fn(),
      };

      (global as any).document = {
        getElementById: (id: string) => {
          if (id === 'composer-mic') return micEl;
          if (id === 'send-error') return errorEl;
          return null;
        },
      };

      (global as any).navigator = {
        mediaDevices: {
          getUserMedia: jest.fn().mockRejectedValue(new Error('Permission denied')),
        },
      };
      (global as any).MediaRecorder = function() {};

      app.watchMicButton();
      expect(micEl.addEventListener).toHaveBeenCalledWith('click', expect.any(Function));
      const clickHandler = micEl.addEventListener.mock.calls[0][1];

      await clickHandler();
      expect(errorEl.textContent).toMatch(/Voice not available: Permission denied/i);
    });
  });

  describe('AC1: watchMicButton onstop targeting under filter or direct room', () => {
    it('AC1: prepends @<role> when actor filter is active upon voice transcription stop', async () => {
      app.setRoleFilter('kade');
      const inputEl = { value: '', focus: jest.fn() };
      let clickHandler: Function | null = null;
      const micEl: any = {
        dataset: {},
        setAttribute: jest.fn(),
        addEventListener: jest.fn((event, fn) => {
          if (event === 'click') clickHandler = fn;
        }),
      };
      const errorEl = { textContent: '', innerHTML: '' };

      (global as any).document = {
        getElementById: (id: string) => {
          if (id === 'composer-mic') return micEl;
          if (id === 'composer-input') return inputEl;
          if (id === 'send-error') return errorEl;
          return null;
        },
        querySelectorAll: (sel: string) => sel.includes('actor-tile') ? [{ dataset: { role: 'kade' } }] : [],
      };

      (global as any).Blob = class {
        size = 100;
        parts: any[];
        constructor(parts: any[]) { this.parts = parts || []; }
      };

      let recorderInstance: any = null;
      (global as any).navigator = {
        mediaDevices: {
          getUserMedia: jest.fn().mockResolvedValue({
            getTracks: () => [{ stop: jest.fn() }],
          }),
        },
      };
      (global as any).MediaRecorder = class {
        state = 'inactive';
        mimeType = 'audio/webm';
        ondataavailable: any = null;
        onstop: any = null;
        constructor() {
          recorderInstance = this;
        }
        start() { this.state = 'recording'; }
        stop() { this.state = 'inactive'; }
      };

      (global as any).fetch = jest.fn().mockResolvedValue({
        ok: true,
        json: async () => ({ ok: true, transcript: 'how is the build?' }),
      });

      app.watchMicButton();
      expect(clickHandler).not.toBeNull();
      await clickHandler!(); // Start recording
      expect(recorderInstance).not.toBeNull();
      expect(recorderInstance.state).toBe('recording');

      // Call onstop to simulate recorder completion
      if (recorderInstance.ondataavailable) {
        recorderInstance.ondataavailable({ data: { size: 100 } });
      }
      await recorderInstance.onstop();

      expect(inputEl.value).toBe('@kade how is the build?');
    });

    it('negative proof: does not prepend role if no actor tiles are loaded on the page even if filter is active', async () => {
      app.setRoleFilter('kade');
      const inputEl = { value: '', focus: jest.fn() };
      let clickHandler: Function | null = null;
      const micEl: any = {
        dataset: {},
        setAttribute: jest.fn(),
        addEventListener: jest.fn((event, fn) => {
          if (event === 'click') clickHandler = fn;
        }),
      };
      const errorEl = { textContent: '', innerHTML: '' };

      (global as any).document = {
        getElementById: (id: string) => {
          if (id === 'composer-mic') return micEl;
          if (id === 'composer-input') return inputEl;
          if (id === 'send-error') return errorEl;
          return null;
        },
        querySelectorAll: () => [], // No tiles loaded
      };

      (global as any).Blob = class {
        size = 100;
        parts: any[];
        constructor(parts: any[]) { this.parts = parts || []; }
      };

      let recorderInstance: any = null;
      (global as any).navigator = {
        mediaDevices: {
          getUserMedia: jest.fn().mockResolvedValue({
            getTracks: () => [{ stop: jest.fn() }],
          }),
        },
      };
      (global as any).MediaRecorder = class {
        state = 'inactive';
        mimeType = 'audio/webm';
        ondataavailable: any = null;
        onstop: any = null;
        constructor() {
          recorderInstance = this;
        }
        start() { this.state = 'recording'; }
        stop() { this.state = 'inactive'; }
      };

      (global as any).fetch = jest.fn().mockResolvedValue({
        ok: true,
        json: async () => ({ ok: true, transcript: 'how is the build?' }),
      });

      app.watchMicButton();
      await clickHandler!();
      if (recorderInstance.ondataavailable) {
        recorderInstance.ondataavailable({ data: { size: 100 } });
      }
      await recorderInstance.onstop();

      // Because NO tiles were loaded on the page, no target role is resolved
      expect(inputEl.value).toBe('how is the build?');
      expect(inputEl.value).not.toMatch(/@kade/);
    });

    it('AC1: prepends @<role> when viewing a direct room upon voice transcription stop', async () => {
      app.setRoleFilter(null);
      const inputEl = { value: '', focus: jest.fn() };
      const pickerEl = { value: 'silas' };
      let clickHandler: Function | null = null;
      const micEl: any = {
        dataset: {},
        setAttribute: jest.fn(),
        addEventListener: jest.fn((event, fn) => {
          if (event === 'click') clickHandler = fn;
        }),
      };
      const errorEl = { textContent: '', innerHTML: '' };

      (global as any).document = {
        getElementById: (id: string) => {
          if (id === 'composer-mic') return micEl;
          if (id === 'composer-input') return inputEl;
          if (id === 'room-picker') return pickerEl;
          if (id === 'send-error') return errorEl;
          return null;
        },
        querySelectorAll: (sel: string) => sel.includes('actor-tile') ? [{ dataset: { role: 'silas' } }] : [],
      };

      (global as any).Blob = class {
        size = 100;
        parts: any[];
        constructor(parts: any[]) { this.parts = parts || []; }
      };

      let recorderInstance: any = null;
      (global as any).navigator = {
        mediaDevices: {
          getUserMedia: jest.fn().mockResolvedValue({
            getTracks: () => [{ stop: jest.fn() }],
          }),
        },
      };
      (global as any).MediaRecorder = class {
        state = 'inactive';
        mimeType = 'audio/webm';
        ondataavailable: any = null;
        onstop: any = null;
        constructor() {
          recorderInstance = this;
        }
        start() { this.state = 'recording'; }
        stop() { this.state = 'inactive'; }
      };

      (global as any).fetch = jest.fn().mockResolvedValue({
        ok: true,
        json: async () => ({ ok: true, transcript: 'check the memory pressure' }),
      });

      app.watchMicButton();
      expect(clickHandler).not.toBeNull();
      await clickHandler!(); // Start recording
      expect(recorderInstance).not.toBeNull();

      if (recorderInstance.ondataavailable) {
        recorderInstance.ondataavailable({ data: { size: 100 } });
      }
      await recorderInstance.onstop();

      expect(inputEl.value).toBe('@silas check the memory pressure');
    });

    it('does not prepend role when room picker value is not a known role (e.g. team or custom)', async () => {
      app.setRoleFilter(null);
      const inputEl = { value: '', focus: jest.fn() };
      const pickerEl = { value: 'team' };
      let clickHandler: Function | null = null;
      const micEl: any = {
        dataset: {},
        setAttribute: jest.fn(),
        addEventListener: jest.fn((event, fn) => {
          if (event === 'click') clickHandler = fn;
        }),
      };
      const errorEl = { textContent: '', innerHTML: '' };

      (global as any).document = {
        getElementById: (id: string) => {
          if (id === 'composer-mic') return micEl;
          if (id === 'composer-input') return inputEl;
          if (id === 'room-picker') return pickerEl;
          if (id === 'send-error') return errorEl;
          return null;
        },
        querySelectorAll: (sel: string) => sel.includes('actor-tile') ? [{ dataset: { role: 'silas' } }] : [],
      };

      (global as any).Blob = class {
        size = 100;
        parts: any[];
        constructor(parts: any[]) { this.parts = parts || []; }
      };

      let recorderInstance: any = null;
      (global as any).navigator = {
        mediaDevices: {
          getUserMedia: jest.fn().mockResolvedValue({
            getTracks: () => [{ stop: jest.fn() }],
          }),
        },
      };
      (global as any).MediaRecorder = class {
        state = 'inactive';
        mimeType = 'audio/webm';
        ondataavailable: any = null;
        onstop: any = null;
        constructor() {
          recorderInstance = this;
        }
        start() { this.state = 'recording'; }
        stop() { this.state = 'inactive'; }
      };

      (global as any).fetch = jest.fn().mockResolvedValue({
        ok: true,
        json: async () => ({ ok: true, transcript: 'team announcement here' }),
      });

      app.watchMicButton();
      await clickHandler!();
      if (recorderInstance.ondataavailable) {
        recorderInstance.ondataavailable({ data: { size: 100 } });
      }
      await recorderInstance.onstop();

      expect(inputEl.value).toBe('team announcement here');
    });

    it('never prepends @jeff even if room picker or actor filter is jeff', async () => {
      app.setRoleFilter('jeff');
      const inputEl = { value: '', focus: jest.fn() };
      const pickerEl = { value: 'jeff' };
      let clickHandler: Function | null = null;
      const micEl: any = {
        dataset: {},
        setAttribute: jest.fn(),
        addEventListener: jest.fn((event, fn) => {
          if (event === 'click') clickHandler = fn;
        }),
      };
      const errorEl = { textContent: '', innerHTML: '' };

      (global as any).document = {
        getElementById: (id: string) => {
          if (id === 'composer-mic') return micEl;
          if (id === 'composer-input') return inputEl;
          if (id === 'room-picker') return pickerEl;
          if (id === 'send-error') return errorEl;
          return null;
        },
        querySelectorAll: (sel: string) => sel.includes('actor-tile') ? [{ dataset: { role: 'wren' } }] : [],
      };

      (global as any).Blob = class {
        size = 100;
        parts: any[];
        constructor(parts: any[]) { this.parts = parts || []; }
      };

      let recorderInstance: any = null;
      (global as any).navigator = {
        mediaDevices: {
          getUserMedia: jest.fn().mockResolvedValue({
            getTracks: () => [{ stop: jest.fn() }],
          }),
        },
      };
      (global as any).MediaRecorder = class {
        state = 'inactive';
        mimeType = 'audio/webm';
        ondataavailable: any = null;
        onstop: any = null;
        constructor() {
          recorderInstance = this;
        }
        start() { this.state = 'recording'; }
        stop() { this.state = 'inactive'; }
      };

      (global as any).fetch = jest.fn().mockResolvedValue({
        ok: true,
        json: async () => ({ ok: true, transcript: 'my personal memo' }),
      });

      app.watchMicButton();
      await clickHandler!();
      if (recorderInstance.ondataavailable) {
        recorderInstance.ondataavailable({ data: { size: 100 } });
      }
      await recorderInstance.onstop();

      expect(inputEl.value).toBe('my personal memo');
      expect(inputEl.value).not.toMatch(/@jeff/);
    });
  });

  describe('AC5: 0-byte chunk and MediaRecorder error surfacing', () => {
    it('AC5: displays visible error and does not call transcribeVoice when 0 audio chunks are recorded', async () => {
      const errorEl = { textContent: '', innerHTML: '' };
      const micEl: any = {
        addEventListener: jest.fn(),
        dataset: {},
        setAttribute: jest.fn(),
      };

      (global as any).document = {
        getElementById: (id: string) => {
          if (id === 'composer-mic') return micEl;
          if (id === 'send-error') return errorEl;
          return null;
        },
      };

      let recorderInstance: any = null;
      (global as any).navigator = {
        mediaDevices: {
          getUserMedia: jest.fn().mockResolvedValue({
            getTracks: () => [{ stop: jest.fn() }],
          }),
        },
      };
      (global as any).MediaRecorder = class {
        state = 'inactive';
        mimeType = 'audio/webm';
        ondataavailable: any = null;
        onstop: any = null;
        constructor() {
          recorderInstance = this;
        }
        start() { this.state = 'recording'; }
        stop() { this.state = 'inactive'; }
      };

      const fetchSpy = jest.fn();
      (global as any).fetch = fetchSpy;

      app.watchMicButton();
      const clickHandler = micEl.addEventListener.mock.calls[0][1];
      await clickHandler(); // start

      // Stop with NO chunks pushed
      await recorderInstance.onstop();

      expect(errorEl.textContent).toMatch(/Voice not added: no audio recorded \(0 bytes\)/i);
      expect(fetchSpy).not.toHaveBeenCalled();
    });

    it('AC5: surfaces error when MediaRecorder constructor throws', async () => {
      const errorEl = { textContent: '', innerHTML: '' };
      const micEl: any = {
        addEventListener: jest.fn(),
        dataset: {},
        setAttribute: jest.fn(),
      };

      (global as any).document = {
        getElementById: (id: string) => {
          if (id === 'composer-mic') return micEl;
          if (id === 'send-error') return errorEl;
          return null;
        },
      };

      (global as any).navigator = {
        mediaDevices: {
          getUserMedia: jest.fn().mockResolvedValue({
            getTracks: () => [{ stop: jest.fn() }],
          }),
        },
      };
      (global as any).MediaRecorder = class {
        constructor() {
          throw new Error('Unsupported codec');
        }
      };

      app.watchMicButton();
      const clickHandler = micEl.addEventListener.mock.calls[0][1];
      await clickHandler();

      expect(errorEl.textContent).toMatch(/Voice recorder failed: Unsupported codec/i);
    });

    it('AC5: surfaces error when MediaRecorder encounters runtime error', async () => {
      const errorEl = { textContent: '', innerHTML: '' };
      const micEl: any = {
        addEventListener: jest.fn(),
        dataset: {},
        setAttribute: jest.fn(),
      };

      (global as any).document = {
        getElementById: (id: string) => {
          if (id === 'composer-mic') return micEl;
          if (id === 'send-error') return errorEl;
          return null;
        },
      };

      let recorderInstance: any = null;
      (global as any).navigator = {
        mediaDevices: {
          getUserMedia: jest.fn().mockResolvedValue({
            getTracks: () => [{ stop: jest.fn() }],
          }),
        },
      };
      (global as any).MediaRecorder = class {
        state = 'inactive';
        mimeType = 'audio/webm';
        ondataavailable: any = null;
        onerror: any = null;
        onstop: any = null;
        constructor() {
          recorderInstance = this;
        }
        start() { this.state = 'recording'; }
        stop() { this.state = 'inactive'; }
      };

      app.watchMicButton();
      const clickHandler = micEl.addEventListener.mock.calls[0][1];
      await clickHandler();

      recorderInstance.onerror({ error: new Error('Audio hardware disconnected') });
      expect(errorEl.textContent).toMatch(/Voice recording error: Audio hardware disconnected/i);
    });
  });
});
