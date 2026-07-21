import { readdir, readFile } from "node:fs/promises";
import { describe, expect, it } from "vitest";
import {
  STAGED_RELEASE_ADR,
  requireAdr,
  validateAdrReferences,
} from "../src/lib/adr-reference";

describe("the generated documentation site", () => {
  it("publishes the manual as production-ready", async () => {
    const home = await readFile("dist/index.html", "utf8");

    expect(home).toContain("PRODUCTION READY");
    expect(home).toContain("Version 1.0");
    expect(home).not.toContain("PRE-RELEASE");
  });

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

  it("publishes every canonical ADR through the architecture index", async () => {
    const canonical = (await readdir("../docs/adr"))
      .filter((entry) => entry.endsWith(".md"))
      .map((entry) => entry.replace(/\.md$/, ""))
      .sort();
    const published = (
      await readdir("dist/docs/architecture/decisions", {
        withFileTypes: true,
      })
    )
      .filter((entry) => entry.isDirectory())
      .map((entry) => entry.name)
      .sort();
    const index = await readFile(
      "dist/docs/architecture/decisions/index.html",
      "utf8",
    );

    expect(published).toEqual(canonical);
    expect(index).toContain("Architecture decision records");
    expect(index).toContain("ADR 0004");
    expect(index).toContain("Superseded by ADR 0006");
  });

  it("renders complete ADR content and links references to stable pages", async () => {
    const record = await readFile(
      "dist/docs/architecture/decisions/0003-availability-oriented-signing-failover/index.html",
      "utf8",
    );
    const architecture = await readFile(
      "dist/docs/architecture/index.html",
      "utf8",
    );

    expect(record).toContain("Prefer bounded availability for signing");
    expect(record).toContain("A denial at one backend may fall through");
    expect(architecture).toContain(
      'href="/lanyard-ssh-agent/docs/architecture/decisions/0002-functional-core-effectful-shell/"',
    );
  });

  it("rejects a reference to an ADR that the canonical collection lacks", () => {
    expect(() => requireAdr([], 99)).toThrow("Referenced ADR 0099 is missing");
    expect(() => {
      validateAdrReferences([
        { entry: { data: { number: STAGED_RELEASE_ADR } } },
      ]);
    }).toThrow("Referenced ADR 0001 is missing");
  });
});
