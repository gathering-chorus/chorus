// @test-type: unit:perf — a wall-clock limit on the closed-port refusal; the suite world's closed ports, no live service.
// @domain: cicd
// @needs: nightly — a 2s wall-clock limit measures the box (#4539: card runs leave it to the nightly)
// @card: #3772
// @owner: wren
/**
 * #3772 / #4539 — the timing half of reply-return-path-3772.test.ts. That file
 * proves the session write is refused; this one proves the refusal comes
 * within 2s. A wall-clock limit reads the box, so only the nightly runs it.
 */
describe('#3772 closed-port refusal, timed', () => {
  test('the closed-port refusal comes within 2s', async () => {
    // the suite world's closed port, or the fetch below proves nothing (Silas 04:30)
    expect(process.env.ATHENA_MAKE_URL).toBe('http://127.0.0.1:9');
    const t0 = Date.now();
    await expect(fetch(`${process.env.ATHENA_MAKE_URL}/v1/identity/sessions`, { method: 'PUT', body: '{}' })).rejects.toThrow();
    expect(Date.now() - t0).toBeLessThan(2000);
  });
});
