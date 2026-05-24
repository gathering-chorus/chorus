// #3058 - pure access-line formatter for chorus-api.
//
// Every chorus-api request writes one access line to stdout. Before this, the
// line carried no timestamp, so an event-loop-block alert at time T (from
// Silas's #3050 monitor) could not be matched to the request that caused it.
//
// With the request START timestamp on the line, a block at time T is the
// request whose window [start, start+ms] contains T. Pure + unit-tested so the
// format (and the timestamp) cannot silently regress.
export function formatAccessLine(
  startMs: number,
  method: string,
  path: string,
  status: number,
  ms: number,
): string {
  return `[chorus-api] ${new Date(startMs).toISOString()} ${method} ${path} ${status} ${ms}ms`;
}
