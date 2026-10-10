// Browser acceptance for the published scene lab (#152).
// Prerequisite: the Pages build has produced site/wasm/scenedetect_wasm.wasm
// (see `bun run pages:build`). The suite serves `site/` statically, with no
// backend, and drives the workbench in Chromium with a single worker.
import { defineConfig, devices } from "@playwright/test";

const port = Number(process.env.SITE_PORT ?? 4173);

export default defineConfig({
  testDir: ".",
  testMatch: "*.spec.mjs",
  outputDir: "../../target/playwright",
  workers: 1,
  fullyParallel: false,
  forbidOnly: Boolean(process.env.CI),
  retries: 0,
  timeout: 90_000,
  expect: { timeout: 15_000 },
  reporter: process.env.CI ? [["list"], ["github"]] : "list",
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    trace: "retain-on-failure",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    command: `node tests/browser/serve-static.mjs site ${port}`,
    cwd: "../..",
    url: `http://127.0.0.1:${port}/workbench.html`,
    reuseExistingServer: false,
    timeout: 30_000,
  },
});
