import { test, expect } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
const binary = resolve("../target/debug/ronchi");
function cli(project: string, ...args: string[]) {
  return JSON.parse(
    execFileSync(binary, ["--project", project, "--json", ...args], {
      encoding: "utf8",
    }),
  );
}
test("native embedded dashboard authenticates, pins evidence, and leaves Lab running after browser closure", async ({
  browser,
}, info) => {
  const project = mkdtempSync(join(tmpdir(), "ronchi-dashboard-"));
  try {
    const lab = cli(
      project,
      "up",
      "--rpc-listen",
      "127.0.0.1:0",
      "--control-listen",
      "127.0.0.1:0",
    ).result;
    const launch = cli(project, "dashboard").result.launch_url;
    const context = await browser.newContext({
        viewport: { width: 1440, height: 960 },
      }),
      page = await context.newPage();
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(String(e)));
    await page.goto(launch);
    await expect(
      page.getByRole("heading", { name: "Timeline", exact: true }),
    ).toBeVisible();
    await expect(
      page.getByText("no_rpc_traffic.", { exact: false }),
    ).toBeVisible();
    await expect(page).not.toHaveURL(/ticket/);
    await page.screenshot({
      path: info.outputPath("desktop.png"),
      fullPage: true,
    });
    await page.getByRole("button", { name: /^#/ }).first().click();
    await expect(page.getByText(/Pinned view/)).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Load raw payload preview" }),
    ).toBeVisible();
    await page.setViewportSize({ width: 390, height: 844 });
    await page.screenshot({
      path: info.outputPath("narrow.png"),
      fullPage: true,
    });
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= window.innerWidth,
      ),
    ).toBe(true);
    expect(errors).toEqual([]);
    await context.close();
    expect(cli(project, "status").result.state).toBe("ready");
    expect(lab.lab_id).toBe(cli(project, "status").result.lab_id);
  } finally {
    try {
      cli(project, "down", "--force");
    } finally {
      rmSync(project, { recursive: true, force: true });
    }
  }
});
