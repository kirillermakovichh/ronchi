import { defineConfig } from "@playwright/test";
export default defineConfig({
  testDir: "./tests",
  timeout: 30000,
  workers: 1,
  use: {
    browserName: "chromium",
    headless: true,
    launchOptions: process.env.RONCHI_CHROMIUM
      ? { executablePath: process.env.RONCHI_CHROMIUM }
      : {},
  },
  outputDir: "./test-results",
});
