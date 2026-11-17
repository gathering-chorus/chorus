// @test-type: unit — renders the Teammates view directly; no live pulse or chorus-api (#4576)
// @domain: messages
// @card: #4576
// @owner: wren
// #4987 (AC3): every teammate's name sits beside its avatar on one line
import { renderRolesPage } from "../src/views";
import * as fs from "fs";
import * as path from "path";

function checkTeammatesPageWords(html: string): string[] {
  const issues: string[] = [];
  if (!/<a href="\/roles"[^>]*>Teammates<\/a>/.test(html)) issues.push("nav-tab-not-Teammates");
  if (!/<h2>Teammates<\/h2>/.test(html)) issues.push("h2-not-Teammates");
  if (!/<th>Teammate<\/th>/.test(html)) issues.push("th-not-Teammate");
  if (/<th>Actor<\/th>/.test(html)) issues.push("still-has-th-Actor");
  if (/>Roles<\/a>/.test(html)) issues.push("still-has-nav-Roles");
  return issues;
}

describe("#4576 Clearing v2 uses one word — Teammate / Teammates — for team members", () => {
  test("nav tab, page heading, and table column all use Teammate(s), never Roles or Actor", () => {
    const actors = [{ id: "abby-normal", name: "Abby Normal", sessions: ["pane · live"], state: "building", activity: "acting", card: null }];
    const html = renderRolesPage({ actors } as never, []);
    expect(checkTeammatesPageWords(html)).toEqual([]);
  });

  test("negative proof: old mixed Roles/Actor markup is caught on every check", () => {
    const oldHtml = "<nav><a href=\"/roles\">Roles</a></nav><h2>Roles</h2><table><thead><tr><th>Actor</th></tr></thead></table>";
    expect(checkTeammatesPageWords(oldHtml)).toEqual([
      "nav-tab-not-Teammates",
      "h2-not-Teammates",
      "th-not-Teammate",
      "still-has-th-Actor",
      "still-has-nav-Roles",
    ]);
  });
});

// #4987 (AC3) — Jeff's screenshot 11-17 05:45: "Abby Normal" and "Borg-queen" wrapped below
// their avatars while the other names sat beside theirs. The name cell's row may not wrap.
describe("#4987 a teammate's name stays beside its avatar", () => {
  const css = fs.readFileSync(path.join(__dirname, "../public/style.css"), "utf8");
  const whoRule = (sheet: string) => (sheet.match(/\.row\.who\s*\{([^}]*)\}/) || [])[1] || "";
  const nameCells = (html: string) => Array.from(html.matchAll(/<td><span class="([^"]+)"><span class="av [^"]+">.<\/span><b>([^<]+)<\/b><\/span><\/td>/g)).map((m) => [m[1], m[2]]);

  test("every name cell, one-word or two, uses the no-wrap row", () => {
    const actors = ["Abby Normal", "Borg-queen", "Kade"].map((name) => ({ id: name.toLowerCase().replace(/ /g, "-"), name, sessions: [], state: "available", activity: "idle", card: null }));
    const cells = nameCells(renderRolesPage({ actors } as never, []));
    expect(cells.map(([, n]) => n)).toEqual(["Abby Normal", "Borg-queen", "Kade"]);
    expect(cells.every(([cls]) => cls.split(" ").includes("who"))).toBe(true);
    expect(whoRule(css)).toMatch(/flex-wrap:\s*nowrap/);
    expect(whoRule(css)).toMatch(/white-space:\s*nowrap/);
  });

  test("NEGATIVE PROOF: the old wrapping row is caught", () => {
    expect(whoRule(".row { flex-wrap: wrap; }")).toBe("");
    expect(whoRule(".row.who { flex-wrap: wrap; }")).not.toMatch(/flex-wrap:\s*nowrap/);
  });
});
