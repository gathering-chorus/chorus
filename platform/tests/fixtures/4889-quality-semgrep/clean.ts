export function f() {
  try { g(); } catch (e) { console.error(e); throw e; }
}
