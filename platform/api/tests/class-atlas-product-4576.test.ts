// @test-type: unit — pure fold + HTML structure check; no live Fuseki or chorus-api (#4576)
// @domain: domains
// @card: #4576
// @owner: wren
import fs from "fs";
import path from "path";
import { buildClassAtlas, classAtlasHandler, SparqlBinding } from "../src/handlers/class-atlas";

const CH = "https://jeffbridwell.com/chorus#";
const uri = (l: string) => ({ type: "uri", value: CH + l });

const ROWS = [
  { product: uri("chorus"), domain: uri("security"), class: uri("Principal") },
  { product: uri("loom"), domain: uri("roles"), class: uri("Teammate") },
  { domain: uri("orphan-dom"), class: uri("OrphanClass") },
] as unknown as SparqlBinding[];

const HOMES = new Map([
  ["Principal", "security"],
  ["Teammate", "roles"],
  ["OrphanClass", "orphan-dom"],
]);

function checkAtlasHtmlGrouping(html: string): string[] {
  const missing: string[] = [];
  if (!/class="aprod"/.test(html)) missing.push("product-section");
  if (!/dom\.product\s*\|\|\s*.unassigned./.test(html)) missing.push("unassigned-fallback");
  if (!/querySelectorAll\(.\.aprod.\)/.test(html)) missing.push("pick-hides-empty-product");
  return missing;
}

describe("#4576 Class Atlas groups Product -> Domain -> Class", () => {
  it("carries each domain's owning product from SPARQL bindings and leaves productless domains undefined (negative proof)", () => {
    const atlas = buildClassAtlas(ROWS, HOMES);
    const sec = atlas.domains.find((d) => d.name === "security")!;
    const roles = atlas.domains.find((d) => d.name === "roles")!;
    const orphan = atlas.domains.find((d) => d.name === "orphan-dom")!;
    expect(sec.product).toBe("chorus");
    expect(roles.product).toBe("loom");
    expect(orphan.product).toBeUndefined();
  });

  it("classAtlasHandler queries urn:chorus:domains:products for chorus:Product hasDomain", async () => {
    const realFetch = global.fetch;
    let asked = "";
    global.fetch = (async (url: string, init?: { body?: string }) => {
      if (init?.body) asked = decodeURIComponent(init.body);
      return { ok: true, status: 200, json: async () => ({ results: { bindings: ROWS } }) };
    }) as unknown as typeof fetch;
    try {
      let body: any;
      const res: any = { status: () => res, json: (b: unknown) => { body = b; return res; } };
      await classAtlasHandler()({} as any, res);
      expect(asked).toContain("urn:chorus:domains:products");
      expect(asked).toContain("chorus:hasDomain");
      expect(body.domains.find((d: any) => d.name === "security").product).toBe("chorus");
    } finally {
      global.fetch = realFetch;
    }
  });

  it("class-atlas.html groups domains under their owning Product section (with negative proof)", () => {
    const html = fs.readFileSync(path.join(__dirname, "../public/athena/class-atlas.html"), "utf8");
    expect(checkAtlasHtmlGrouping(html)).toEqual([]);
    const oldHtml = "<section class=\"adom\"><h2>alerts</h2></section>";
    expect(checkAtlasHtmlGrouping(oldHtml)).toEqual([
      "product-section",
      "unassigned-fallback",
      "pick-hides-empty-product",
    ]);
  });
});
