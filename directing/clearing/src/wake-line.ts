/**
 * #4339 — the only thing pulse types into a role's pane for a nudge or an
 * alert. Must equal WAKE_LINE in platform/pulse/src/delivery-worker.ts. Every
 * other prompt in a role's transcript is Jeff: peers no longer type words into
 * panes, so a "[nudge from" label in a prompt is text he typed.
 */
export const WAKE_LINE = '[chorus] a message is waiting in your context under Pending nudges';
