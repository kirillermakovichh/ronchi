import { test, expect, type Page } from "@playwright/test";
async function journal(page: Page) {
  await page
    .getByRole("navigation", { name: "Main navigation" })
    .getByRole("button", { name: "Traffic", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Journal records", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Journal", exact: true }),
  ).toBeVisible();
}
async function checks(page: Page) {
  await page
    .getByRole("navigation", { name: "Main navigation" })
    .getByRole("button", { name: "Checks", exact: true })
    .click();
}

import { createServer } from "node:http";
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
const binary = resolve("../target/debug/ronchi");
function cli(project: string, ...args: string[]) {
  try {
    return JSON.parse(
      execFileSync(binary, ["--project", project, "--json", ...args], {
        encoding: "utf8",
      }),
    );
  } catch (error) {
    const e = error as { stdout?: Buffer; stderr?: Buffer };
    throw new Error(
      `CLI ${args.join(" ")} failed: ${String(e.stdout)} ${String(e.stderr)}`,
    );
  }
}
async function source() {
  let calls = 0;
  const listeners = new Set<() => void>();
  const text = "я🧪".repeat(1500);
  const server = createServer((req, res) => {
    let body = "";
    req.on("data", (chunk) => {
      body += String(chunk);
      if (body.length > 16384) req.destroy();
    });
    req.on("end", () => {
      const invocation = JSON.parse(body);
      calls++;
      for (const notify of listeners) notify();
      const response = JSON.stringify({
        contract_version: 1,
        invocation_id: invocation.invocation_id,
        source: "independent",
        schema_version: 1,
        observed_at: "2026-11-03T00:00:00Z",
        revision: `r${calls}`,
        facts: {
          text: { availability: "available", value: text },
          big: { availability: "available", value: "EXACT_INTEGER" },
        },
      }).replace('"EXACT_INTEGER"', "9007199254740993");
      res.setHeader("content-type", "application/json");
      res.end(response);
    });
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address() as { port: number };
  return {
    url: `http://127.0.0.1:${address.port}/observe`,
    text,
    calls: () => calls,
    wait(count: number, ms = 5000) {
      return new Promise<void>((resolve, reject) => {
        const timer = setTimeout(() => {
          listeners.delete(check);
          reject(
            new Error(`Expected ${count} source calls, observed ${calls}`),
          );
        }, ms);
        function check() {
          if (calls >= count) {
            clearTimeout(timer);
            listeners.delete(check);
            resolve();
          }
        }
        listeners.add(check);
        check();
      });
    },
    async close() {
      server.closeAllConnections();
      await new Promise<void>((resolve) => server.close(() => resolve()));
    },
  };
}

test("public collection cancels on confirmed status and Unicode payload follows byte offsets", async ({
  page,
}) => {
  const project = mkdtempSync(join(tmpdir(), "ronchi-payload-")),
    app = await source();
  let id: string | undefined;
  try {
    mkdirSync(join(project, "ronchi"));
    writeFileSync(
      join(project, "ronchi/config.toml"),
      `schema_version=1\n[[checks.sources]]\ncheck_id='state'\nversion=1\nsource='independent'\nschema_version=1\nurl='${app.url}'\n[checks.sources.facts.text]\ntype='string'\nmax_bytes=20000\n[checks.sources.facts.big]\ntype='integer'\n`,
    );
    id = cli(
      project,
      "up",
      "--detach",
      "--rpc-listen",
      "127.0.0.1:0",
      "--control-listen",
      "127.0.0.1:0",
    ).result.lab_id;
    await page.goto(cli(project, "dashboard").result.launch_url);
    await expect(
      page.getByRole("heading", { name: "Lab overview", exact: true }),
    ).toBeVisible();
    expect(app.calls()).toBe(0);
    await journal(page);
    await page.getByRole("button", { name: /^#/ }).first().click(); // UI polling is pinned; server work remains independent.
    await checks(page);
    await page.getByLabel("Intent identity", { exact: true }).fill("W1");
    await page.getByLabel("Explicit bounded polling (60 seconds)").check();
    let submissions = 0;
    await page.route("**/api/checks/observe", async (route) => {
      submissions++;
      const response = await route.fetch();
      expect(response.ok()).toBe(true);
      await route.abort("failed");
    });
    await page
      .getByRole("button", { name: "Collect observations", exact: true })
      .click();
    await expect(page.locator(".collection-result")).toContainText(
      "outcome_unknown",
    );
    await expect(page.locator(".controls [role=alert]")).toBeVisible();
    await page.getByRole("button", { name: "Inspect Lab status" }).click();
    await expect(
      page.getByText("Current public Lab status", { exact: true }),
    ).toBeVisible();
    await app.wait(2);
    expect(submissions).toBe(1);
    await page
      .getByRole("button", { name: "Refresh collection status" })
      .click();
    await expect(page.locator(".collection-result")).toContainText(
      '"status": "running"',
    );
    await page
      .getByRole("button", { name: "Cancel collection", exact: true })
      .click();
    await expect
      .poll(async () => {
        await page
          .getByRole("button", { name: "Refresh collection status" })
          .click();
        return await page.locator(".controls pre").first().textContent();
      })
      .toContain('"status": "cancelled"');
    const count = app.calls();
    await expect(app.wait(count + 1, 2200)).rejects.toThrow("Expected");
    await expect(
      page.getByLabel("Explicit Check evaluation"),
    ).not.toBeVisible();
    const observations = cli(
      project,
      "investigate",
      "timeline",
      "--kind",
      "check.observation",
    );
    const event = observations.result.items[0].event_ref.event_seq,
      snapshot = observations.view.snapshot_seq;
    await page.goto(
      cli(
        project,
        "dashboard",
        "--selection",
        `snapshot=${snapshot}&event=${event}`,
      ).result.launch_url,
    );
    await expect(
      page.getByRole("button", { name: "Load raw payload preview" }),
    ).toBeVisible();
    await page
      .getByRole("button", { name: "Load raw payload preview" })
      .click();
    let all = "",
      offset = 0;
    for (let chunks = 0; chunks < 10; chunks++) {
      await expect
        .poll(async () => {
          const value = JSON.parse(
            (await page
              .getByLabel("Payload response JSON", { exact: true })
              .textContent()) ?? "{}",
          );
          return value.items?.[0]?.payload?.offset;
        })
        .toBe(offset);
      const value = JSON.parse(
          (await page
            .getByLabel("Payload response JSON", { exact: true })
            .textContent()) ?? "{}",
        ),
        payload = value.items[0].payload;
      expect(value.items[0].event_ref).toEqual({
        lab_id: id,
        event_seq: event,
      });
      all += payload.preview;
      if (payload.next_offset === null) {
        await expect(
          page.getByRole("button", { name: "Next payload chunk" }),
        ).toHaveCount(0);
        break;
      }
      expect(payload.next_offset).toBe(
        offset + Buffer.byteLength(payload.preview),
      );
      offset = payload.next_offset;
      const next = page.getByRole("button", { name: "Next payload chunk" });
      await next.evaluate((button: HTMLButtonElement) => {
        button.click();
        button.click();
      });
    }
    expect(all).toContain(app.text);
    expect(all).toContain("9007199254740993");
    expect(all).not.toContain("�");
  } finally {
    if (id) cli(project, "--lab-id", id, "down", "--force");
    await app.close();
    rmSync(project, { recursive: true, force: true });
  }
});

test("obsolete native reads cannot replace a new scope and UI polling preserves focus", async ({
  page,
}) => {
  const project = mkdtempSync(join(tmpdir(), "ronchi-reads-"));
  let id: string | undefined;
  let release: () => void = () => {};
  try {
    id = cli(
      project,
      "up",
      "--detach",
      "--rpc-listen",
      "127.0.0.1:0",
      "--control-listen",
      "127.0.0.1:0",
    ).result.lab_id;
    await page.clock.install();
    await page.goto(cli(project, "dashboard").result.launch_url);
    await journal(page);
    await expect(page.locator(".event")).toHaveCount(2);
    await page.getByText("Filter this investigation", { exact: true }).click();
    await page.getByText("More filters", { exact: true }).click();
    let started: () => void = () => {};
    const observed = new Promise<void>((r) => (started = r)),
      barrier = new Promise<void>((r) => (release = r));
    await page.route("**/api/investigate", async (route) => {
      const request = route.request().postDataJSON();
      if (
        request.operation === "investigate.summary" &&
        request.arguments.scope.kind?.[0] === "rpc.boundary"
      ) {
        const real = await route.fetch();
        started();
        await barrier;
        try {
          await route.fulfill({ response: real });
        } catch {
          /* browser cancelled the obsolete read */
        }
      } else await route.continue();
    });
    await page
      .getByRole("textbox", { name: "kind", exact: true })
      .fill("rpc.boundary");
    await page.getByRole("button", { name: "Apply filters" }).click();
    await observed;
    await page
      .getByText("Investigation context and recovery", { exact: true })
      .click();
    await page.getByText("Committed scope", { exact: true }).click();
    await expect(
      page
        .locator(".summary details.raw")
        .filter({ hasText: "Committed scope" })
        .locator("pre"),
    ).toHaveText("{}");
    await page
      .getByRole("textbox", { name: "kind", exact: true })
      .fill("lab.ready");
    await page.getByRole("button", { name: "Apply filters" }).click();
    await expect(page.locator(".event")).toHaveCount(1);
    await expect(page.locator(".event")).toContainText("lab.ready");
    await expect(
      page
        .locator(".summary details.raw")
        .filter({ hasText: "Committed scope" })
        .locator("pre"),
    ).toContainText("lab.ready");
    release();
    let polls = 0;
    page.on("request", (request) => {
      if (request.url().includes("/api/investigate")) polls++;
    });
    // Controlled visibility signal in a real Chromium page; HTTP responses remain native.
    await page.evaluate(() =>
      Object.defineProperty(document, "hidden", {
        configurable: true,
        value: true,
      }),
    );
    await page.clock.runFor(8000);
    expect(polls).toBe(0);
    await page.evaluate(() =>
      Object.defineProperty(document, "hidden", {
        configurable: true,
        value: false,
      }),
    );
    const apply = page.getByRole("button", { name: "Apply filters" });
    await apply.focus();
    await page.keyboard.press("Tab");
    await page.keyboard.press("Shift+Tab");
    await page.clock.runFor(3000);
    await expect.poll(() => polls).toBeGreaterThan(0);
    await expect(page.locator(".event")).toHaveCount(1);
    await expect(page.locator(".event")).toContainText("lab.ready");
    await expect(apply).toBeFocused();
    expect(polls).toBe(1);
    expect(
      await apply.evaluate((el) => getComputedStyle(el).outlineStyle),
    ).not.toBe("none");
    const home = new URL(page.url()).origin;
    await page.goto(home);
    await expect(
      page.getByRole("heading", { name: "No Lab selected" }),
    ).toBeVisible();
    await page.goto(
      cli(project, "dashboard", "--selection", "snapshot=2&event=99999").result
        .launch_url,
    );
    await expect(page.locator("aside [role=alert]")).toContainText(
      "unavailable",
    );
    expect(page.url()).toContain("event=99999"); // No implicit latest fallback.
    await page
      .getByRole("button", { name: "Refresh to latest snapshot" })
      .click();
    await expect(page.locator("aside [role=alert]")).toHaveCount(0);
    // Native invalid watermark is an operational error; it cannot become a Verdict.
    await page.goto(
      cli(
        project,
        "dashboard",
        "--selection",
        "snapshot=9223372036854775807&event=1",
      ).result.launch_url,
    );
    await expect(page.getByRole("alert").first()).toBeVisible();
    await expect(page.getByLabel("Explicit Check evaluation")).toHaveCount(0);
  } finally {
    release();
    if (id) cli(project, "--lab-id", id, "down", "--force");
    rmSync(project, { recursive: true, force: true });
  }
});

test("connection loss preserves the pinned view and offers exact retained recovery", async ({
  page,
}, info) => {
  const project = mkdtempSync(join(tmpdir(), "ronchi-connection-"));
  let id: string | undefined;
  try {
    id = cli(
      project,
      "up",
      "--detach",
      "--rpc-listen",
      "127.0.0.1:0",
      "--control-listen",
      "127.0.0.1:0",
    ).result.lab_id;
    await page.goto(cli(project, "dashboard").result.launch_url);
    await journal(page);
    await page.getByRole("button", { name: /^#/ }).first().click();
    await expect(
      page.getByRole("button", { name: "Load raw payload preview" }),
    ).toBeVisible();
    const prefix = await page
      .locator(".summary .section-title > span")
      .textContent();
    cli(project, "--lab-id", id, "down");
    await expect(page.getByRole("alert").first()).toContainText("Connection", {
      timeout: 10000,
    });
    expect(
      await page.locator(".summary .section-title > span").textContent(),
    ).toBe(prefix);
    await expect(
      page.getByRole("button", { name: "Load raw payload preview" }),
    ).toBeVisible();
    await page
      .getByText("Investigation context and recovery", { exact: true })
      .click();
    await page
      .getByText("Open this pinned selection after Lab stop", { exact: true })
      .click();
    await expect(
      page.getByText(/dashboard --serve --lab-id/, { exact: false }).first(),
    ).toContainText(id);
    await expect(
      page.getByText(/--selection/, { exact: false }).first(),
    ).toContainText("event=");
    await page.screenshot({
      path: info.outputPath("connection-desktop.png"),
      fullPage: true,
    });
    await page.setViewportSize({ width: 390, height: 844 });
    await page.screenshot({
      path: info.outputPath("connection-narrow.png"),
      fullPage: true,
    });
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
  } finally {
    if (id && cli(project, "--lab-id", id, "status").result.state === "ready")
      cli(project, "--lab-id", id, "down", "--force");
    rmSync(project, { recursive: true, force: true });
  }
});
