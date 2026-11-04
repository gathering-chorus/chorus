// @test-type: unit — Clearing v2 composer delivery guarantee (#4877 AC7)
// @domain: messages
export {};
/**
 * Tests for guaranteed /api/send delivery and error handling (#4877 AC7).
 */
const deliveryApp = require('../public/app.js');

describe('guaranteed /api/send delivery (#4877 AC7)', () => {
  it('confirms delivery when /api/send returns 200', async () => {
    const inputEl = { value: 'status check' };
    const statusEl = { textContent: '', style: { display: 'none' } };
    const btnEl = { textContent: 'Send', classList: { add: jest.fn(), remove: jest.fn() } };
    const fetchFn = jest.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ ok: true, stored: true }),
    });

    await deliveryApp.handleSendMessage('status check', { inputEl, statusEl, btnEl, fetchFn });

    expect(fetchFn).toHaveBeenCalled();
    expect(btnEl.textContent).toBe('Sent');
    expect(statusEl.style.display).toBe('none');
  });

  it('displays visible error in #send-error and restores input when /api/send fails (500)', async () => {
    const inputEl = { value: 'important message' };
    const statusEl = { textContent: '', style: { display: 'none' } };
    const btnEl = { textContent: 'Send', classList: { add: jest.fn(), remove: jest.fn() } };
    const fetchFn = jest.fn().mockResolvedValue({
      ok: false,
      status: 500,
      json: async () => ({ error: 'pulse unreachable' }),
    });

    await deliveryApp.handleSendMessage('important message', { inputEl, statusEl, btnEl, fetchFn });

    expect(fetchFn).toHaveBeenCalled();
    expect(statusEl.textContent).toBe('Failed: pulse unreachable');
    expect(statusEl.style.display).toBe('block');
    expect(inputEl.value).toBe('important message');
  });

  it('displays visible error in #send-error when fetch rejects (network error)', async () => {
    const inputEl = { value: 'message during outage' };
    const statusEl = { textContent: '', style: { display: 'none' } };
    const btnEl = { textContent: 'Send', classList: { add: jest.fn(), remove: jest.fn() } };
    const fetchFn = jest.fn().mockRejectedValue(new Error('Network connection lost'));

    await deliveryApp.handleSendMessage('message during outage', { inputEl, statusEl, btnEl, fetchFn });

    expect(fetchFn).toHaveBeenCalled();
    expect(statusEl.textContent).toBe('Failed: Network connection lost');
    expect(statusEl.style.display).toBe('block');
    expect(inputEl.value).toBe('message during outage');
  });
});
