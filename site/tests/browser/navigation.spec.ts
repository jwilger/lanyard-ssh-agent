import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

test("the manual is navigable and has no detectable accessibility violations", async ({
  page,
}) => {
  await page.goto("./");
  await expect(
    page.getByRole("heading", { level: 1, name: /Lanyard/i }),
  ).toBeVisible();
  await page.getByRole("link", { name: /Install Lanyard/i }).click();
  await expect(page).toHaveURL(/\/docs\/quick-start\/$/);
  await expect(
    page.getByRole("heading", { level: 1, name: "Quick start" }),
  ).toBeVisible();

  const results = await new AxeBuilder({ page }).analyze();
  expect(results.violations).toEqual([]);
});

test("all documented manual routes load", async ({ page }) => {
  const routes = [
    "docs/",
    "docs/quick-start/",
    "docs/cli/",
    "docs/nix/",
    "docs/routing/",
    "docs/git-signing/",
    "docs/security/",
    "docs/troubleshooting/",
    "docs/architecture/",
    "docs/contributing/",
    "docs/releases/",
  ];
  for (const route of routes) {
    const response = await page.goto(route);
    expect(response?.ok(), `${route} should load`).toBe(true);
    await expect(page.locator("main h1")).toBeVisible();
  }
});
