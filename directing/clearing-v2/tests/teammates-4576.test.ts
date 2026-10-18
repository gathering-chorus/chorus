// @test-type: unit — renders the Teammates view directly; no live pulse or chorus-api (#4576)
// @domain: messages
// @card: #4576
// @owner: wren
import { renderRolesPage } from "../src/views";

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
