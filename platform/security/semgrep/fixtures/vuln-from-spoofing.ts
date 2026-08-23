// SEEDED VIOLATION (#3981 negative proof) — mirrors the live #3980 hole.
// Caller-supplied `from` reaches messageRouter.ingest with nothing binding it
// to the authenticated identity. The semgrep rule MUST flag this.
// This file is a fixture, never wired into a running service.

declare const messageRouter: { ingest: (m: unknown) => void };

export function postMessageVULN(req: { body: { from?: string; text?: string; type?: string } }, res: { json: (x: unknown) => void }) {
  const { from, text } = req.body;
  if (!from || !text) return res.json({ error: 'from and text required' });
  // ruleid: chorus-caller-supplied-identity-to-sink
  messageRouter.ingest({ from, text, ts: new Date().toISOString(), type: req.body.type || 'role-response' });
  res.json({ ok: true });
}
