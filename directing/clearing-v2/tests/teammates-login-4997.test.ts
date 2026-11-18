// @test-type: unit — pulse's actor JSON through toActorRow, rendered by the Teammates view; no live pulse (#4997)
// @domain: identity
// @card: #4997
// @owner: silas
// #4997 AC1 (reworded, Jeff 19:24): the Teammates tile says "logged in" only when pulse counts
// a live, open Session row for the role; otherwise "not logged in", with no reason there (the
// reason lives in `chorus-principal status`). Unread Session rows read "unverified".
// Fixtures are pulse's own actor JSON, run through toActorRow — the real display format
// ("<channel> · live · HH:MM"), never a hand-written session string.
import { renderRolesPage } from "../src/views";
import { toActorRow, sessionsSourceUnread } from "../src/sources/pulse";

const pulseActor = (id: string, sessions: Array<{ live: boolean }>, unmeasuredSource?: Array<{ source: string; reason: string }>) => ({
  aboutPrincipal: `https://jeffbridwell.com/chorus#principal-${id}`, name: id, stateValue: "available", activity: "idle",
  sessions: sessions.map((s, i) => ({ id: `${id}-s${i}`, channel: "pane", live: s.live, lastActionAt: "2026-11-17T23:50:00Z" })),
  unmeasuredSource,
});
const page = (actors: unknown[], unmeasured?: Array<{ source: string; reason: string }>) =>
  renderRolesPage({ actors: (actors as never[]).map(toActorRow), sessionsUnread: sessionsSourceUnread(unmeasured) } as never, []);
const loginCell = (html: string, id: string): string => {
  const row = html.split("<tr").find((r) => r.includes(`<b>${id}</b>`)) || "";
  return (row.match(/class="login[^"]*">([^<]*)</) || [])[1] || "";
};

describe("#4997 the Teammates tile tells the truth about login", () => {
  test("a live, open Session row (pulse's real format, with its time) reads logged in", () => {
    const row = toActorRow(pulseActor("kade", [{ live: true }]) as never);
    expect(row.sessions[0]).toMatch(/ · live · /); // the real display string carries the time after "live"
    expect(loginCell(page([pulseActor("kade", [{ live: true }])]), "kade")).toBe("logged in");
  });

  test("NEGATIVE PROOF: no rows, or only rows not live, read not logged in", () => {
    expect(loginCell(page([pulseActor("kade", [])]), "kade")).toBe("not logged in");
    expect(loginCell(page([pulseActor("kade", [{ live: false }, { live: false }])]), "kade")).toBe("not logged in");
  });

  test("NEGATIVE PROOF: Session rows unread for the page never read logged in, they read unverified", () => {
    const html = page([pulseActor("kade", [{ live: true }])], [{ source: "athena:identity:sessions", reason: "timeout" }]);
    expect(loginCell(html, "kade")).toBe("unverified");
  });

  test("NEGATIVE PROOF: one actor's own unread sessions read unverified; another source unread does not", () => {
    const html = page([
      pulseActor("kade", [{ live: true }], [{ source: "athena:identity:sessions", reason: "timeout" }]),
      pulseActor("wren", [{ live: true }], [{ source: "pulse-api:streams", reason: "timeout" }]),
    ]);
    expect(loginCell(html, "kade")).toBe("unverified");
    expect(loginCell(html, "wren")).toBe("logged in");
  });
});
