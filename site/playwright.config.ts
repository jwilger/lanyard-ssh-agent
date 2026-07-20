import { defineConfig, devices } from "@playwright/test";

const port = process.env.LANYARD_SITE_PORT ?? "4327";

export default defineConfig({
  testDir: "tests/browser",
  outputDir: "test-results",
  reporter: [["list"], ["html", { open: "never" }]],
  use: {
    baseURL: `http://127.0.0.1:${port}/lanyard-ssh-agent/`,
    trace: "retain-on-failure",
  },
  webServer: {
    command: `ASTRO_DEV_BACKGROUND=0 npm run dev -- --ignore-lock --host 127.0.0.1 --port ${port}`,
    url: `http://127.0.0.1:${port}/lanyard-ssh-agent/`,
    reuseExistingServer: !process.env.CI,
  },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
    { name: "mobile", use: { ...devices["Pixel 7"] } },
  ],
});
