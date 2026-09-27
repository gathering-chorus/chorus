/**
 * #4362 — is Jeff typing in this pane right now?
 *
 * Claude Code draws its input line as "❯" followed by a non-breaking space,
 * between two box rules at the bottom of the pane. Words after the prompt
 * mean a keystroke from pulse would land in the middle of Jeff's message
 * (it did, three times on 2026-09-27). Only the LAST input line counts;
 * a "❯" in scrollback is history, not typing.
 */

// eslint-disable-next-line no-control-regex -- stripping terminal colour codes is the point
const ANSI = /\u001b\[[0-9;?]*[A-Za-z]/g;
const PROMPT = /^\s*[❯>][\u00a0 ]?(.*)$/;

export function paneHasTypedInput(capture: string): boolean {
  const lines = capture.replace(ANSI, '').split('\n');
  for (let i = lines.length - 1; i >= 0; i--) {
    // eslint-disable-next-line security/detect-object-injection -- i is a bounded loop index
    const m = PROMPT.exec(lines[i]);
    if (m) return m[1].replace(/\u00a0/g, ' ').trim().length > 0;
  }
  return false;
}
