import { readdir, readFile } from "node:fs/promises";
import { describe, expect, it } from "vitest";

describe("the generated documentation site", () => {
  it("introduces Lanyard at the repository URL", async () => {
    const home = await readFile("dist/index.html", "utf8");

    expect(home).toContain("One socket. Every agent within reach.");
  });

  it("generates every section in the operator manual", async () => {
    const entries = await readdir("dist/docs", { withFileTypes: true });
    const sections = entries
      .filter((entry) => entry.isDirectory())
      .map((entry) => entry.name);

    expect(sections.sort()).toEqual([
      "architecture",
      "cli",
      "contributing",
      "git-signing",
      "nix",
      "quick-start",
      "releases",
      "routing",
      "security",
      "troubleshooting",
    ]);
  });

  it("keeps generated internal links under the repository base path", async () => {
    const home = await readFile("dist/index.html", "utf8");
    const internalLinks = [...home.matchAll(/href="([^"]+)"/g)]
      .map((match) => match[1])
      .filter((href): href is string => href?.startsWith("/") === true);

    expect(
      internalLinks.every((href) => href.startsWith("/lanyard-ssh-agent/")),
    ).toBe(true);
  });
});
