// @test-type: unit — renders the Teammates view directly; no live pulse (#4997)
// @domain: identity
// @card: #4997
// @owner: silas
// #4997 AC1 (reworded, Jeff 19:24): the Teammates tile says "logged in" only when the role's
// Session row is open (pulse lists it live); otherwise "not logged in", with no reason there
// (the reason lives in `chorus-principal status`). An unread pulse says "unverified".
import { renderRolesPage } from "../src/views";

const actor = (id: string, sessions: string[], unmeasured?: string) => ({ id, name: id, sessions, unmeasured, state: "available", activity: "idle", card: null, cards: [], stream: [] });
const loginCell = (html: string, id: string): string => {
  const row = html.split("<tr").find((r) => r.includes(`<b>${id}</b>`)) || "";
  return (row.match(/class="login[^"]*">([^<]*)</) || [])[1] || "";
};

describe("#4997 the Teammates tile tells the truth about login", () => {
  test("an open, live Session row reads logged in", () => {
    const html = renderRolesPage({ actors: [actor("kade", ["kade-s1 · pane · live"])] } as never, []);
    expect(loginCell(html, "kade")).toBe("logged in");
  });

  test("NEGATIVE PROOF: no open row, or only rows not live, reads not logged in", () => {
    let html = renderRolesPage({ actors: [actor("kade", [])] } as never, []);
    expect(loginCell(html, "kade")).toBe("not logged in");
    html = renderRolesPage({ actors: [actor("kade", ["+1 not live"])] } as never, []);
    expect(loginCell(html, "kade")).toBe("not logged in");
  });

  test("NEGATIVE PROOF: an unread pulse never reads logged in, it reads unverified", () => {
    const html = renderRolesPage({ actors: [actor("kade", ["kade-s1 · pane · live"])], unmeasured: "athena:identity:sessions: timeout" } as never, []);
    expect(loginCell(html, "kade")).toBe("unverified");
  });

  test("NEGATIVE PROOF: one actor's own unread sessions read unverified; others are unaffected", () => {
    const html = renderRolesPage({ actors: [
      actor("kade", ["kade-s1 · pane · live"], "athena:identity:sessions: timeout"),
      actor("wren", ["wren-s1 · pane · live"], "pulse-api:streams: timeout"),
    ] } as never, []);
    expect(loginCell(html, "kade")).toBe("unverified");
    expect(loginCell(html, "wren")).toBe("logged in");
  });
});
