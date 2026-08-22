// SAFE COUNTERPART (#3981 negative proof, other direction) — the FIXED shape.
// Identity is derived from the verified session principal; the caller-supplied
// `from` is ignored. The semgrep rule must NOT flag this — proving the rule
// separates the violation from its fix, not just "matches ingest".

declare const messageRouter: { ingest: (m: unknown) => void };
declare function sessionPrincipal(req: unknown): { id: string } | null;

export function postMessageSAFE(req: { body: { text?: string; type?: string } }, res: { json: (x: unknown) => void }) {
  const p = sessionPrincipal(req);
  if (!p) return res.json({ error: 'authentication required' });
  const { text } = req.body;
  if (!text) return res.json({ error: 'text required' });
  // from is the VERIFIED principal, never the request body
  messageRouter.ingest({ from: p.id, text, ts: new Date().toISOString(), type: req.body.type || 'role-response' });
  res.json({ ok: true });
}
