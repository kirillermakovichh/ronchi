import { test, expect } from "@playwright/test";
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
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
function normalDownWithDiagnostics(project: string, labId: string) {
  try {
    cli(project, "down");
  } catch (original) {
    let resources: unknown = { state: "unknown" },
      cleanup = "not attempted: status unavailable";
    try {
      const current = cli(project, "--lab-id", labId, "status").result;
      resources = {
        lab_id: current.lab_id,
        state: current.state,
        health: current.health,
        journal_state: current.journal_state,
        snapshot_seq: current.snapshot_seq,
        pid: current.process?.pid,
      };
      if (current.state === "ready") {
        try {
          cli(project, "--lab-id", labId, "down", "--force");
          cleanup = "owned cleanup completed";
        } catch {
          cleanup = "owned cleanup failed";
        }
      } else {
        cleanup = "no ready owned Lab remained";
      }
    } catch {
      /* Keep the original normal-down failure even if inspection fails. */
    }
    throw new Error(
      `${String(original)}; sequence=up-ready -> normal down; known_lab_id=${labId}; resource_state=${JSON.stringify(resources)}; cleanup=${cleanup}`,
    );
  }
}
test("native embedded dashboard authenticates, pins evidence, and leaves Lab running after browser closure", async ({
  browser,
}, info) => {
  const project = mkdtempSync(join(tmpdir(), "ronchi-dashboard-"));
  try {
    const lab = cli(
      project,
      "up",
      "--detach",
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
    const endpoint = new URL(launch);
    const reused = await page.request.post(
      `${endpoint.origin}/ui/${lab.lab_id}/session`,
      {
        headers: { Origin: endpoint.origin },
        data: {
          ticket: new URLSearchParams(endpoint.hash.slice(1)).get("ticket"),
        },
      },
    );
    expect(reused.status()).toBe(200);
    const session = (await context.cookies()).find((c) =>
      c.name.startsWith("ronchi_"),
    )!;
    expect(session.httpOnly).toBe(true);
    expect(session.sameSite).toBe("Strict");
    expect(session.expires).toBe(-1);
    const second = cli(
      project,
      "--lab",
      "second",
      "up",
      "--detach",
      "--rpc-listen",
      "127.0.0.1:0",
      "--control-listen",
      "127.0.0.1:0",
    ).result;
    const secondPage = await context.newPage();
    try {
      await secondPage.goto(
        cli(project, "--lab-id", second.lab_id, "dashboard").result.launch_url,
      );
      await expect(
        secondPage.getByRole("heading", { name: "Timeline", exact: true }),
      ).toBeVisible();
      expect(
        (await context.cookies()).filter((c) => c.name.startsWith("ronchi_"))
          .length,
      ).toBe(2);
      await page.reload();
      await expect(
        page.getByRole("heading", { name: "Timeline", exact: true }),
      ).toBeVisible();
    } finally {
      await secondPage.close();
      cli(project, "--lab-id", second.lab_id, "down", "--force");
    }

    let releaseCatalog:()=>void=()=>{};
    const catalogGate=new Promise<void>(resolve=>{releaseCatalog=resolve;});
    let catalogFinished:()=>void=()=>{};
    const catalogDone=new Promise<void>(resolve=>{catalogFinished=resolve;});
    await page.route("**/api/cases",async route=>{
      try{
        const response=await route.fetch({headers:{...await route.request().allHeaders(),origin:endpoint.origin}});
        expect(response.status(),await response.text()).toBe(200);
        await catalogGate;
        await route.fulfill({response});
      }finally{catalogFinished();}
    });
    await page.getByText("Case controls", { exact: false }).click();
    const customArtifact = {schema:"ronchi.case/v1",id:"custom.browser",revision:"1",requires:{primitives:["observe.rpc@1"]},modes:["simulator"],filter:{methods:["getHealth"],transaction_origin:"known_local"},state:{key:"case",scope:"activation",max_keys:1,on_exhausted:"error"},sequence:[{id:"observe",using:"observe.rpc@1",until:{matches:1},next:"done"}]};
    try {
    await page.getByRole("textbox",{name:"Custom Case YAML"}).fill(JSON.stringify(customArtifact));
    const [validationResponse]=await Promise.all([
      page.waitForResponse(response=>response.url().endsWith("/api/case/validate")),
      page.getByRole("button",{name:"Validate Custom Case"}).click(),
    ]);
    expect(validationResponse.status(),await validationResponse.text()).toBe(200);
    expect(await validationResponse.json()).toMatchObject({result:{digest:expect.any(String)}});
    await expect(page.getByLabel("Resolved Custom Case plan")).toContainText('"digest"');
    }finally{
      releaseCatalog();
      await catalogDone;
    }
    await page.getByRole("button",{name:"Activate Custom Case"}).click();
    await expect.poll(()=>cli(project,"status").result.active_case?.case_id).toBe("custom.browser");
    const customRevision=cli(project,"status").result.active_case.revision;
    await page.getByRole("textbox",{name:"Custom Case YAML"}).fill(JSON.stringify({...customArtifact,schema:"ronchi.case/v2"}));
    await expect(page.getByRole("button",{name:"Activate Custom Case"})).toBeDisabled();
    await page.getByRole("button",{name:"Validate Custom Case"}).click();
    await expect(page.locator(".controls [role=alert]")).toContainText("unsupported_schema");
    expect(cli(project,"status").result.active_case.revision).toBe(customRevision);
    await page.getByRole("textbox",{name:"Custom Case YAML"}).fill("");
    await page
      .getByRole("button", { name: "Arm / replace tx-response-lost" })
      .click();
    await expect
      .poll(() => cli(project, "status").result.active_case?.case_id)
      .toBe("tx-response-lost");
    await expect(
      page.getByText("waiting_for_match.", { exact: false }),
    ).toBeVisible();
    await page
      .getByRole("button", { name: "Disarm Case", exact: true })
      .click();
    await expect
      .poll(() => cli(project, "status").result.active_case)
      .toBe(null);

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
    await page.locator("aside pre").scrollIntoViewIfNeeded();
    await expect(page.locator("aside pre")).toContainText("items");
    await page.screenshot({ path: info.outputPath("narrow-evidence.png") });
    expect(errors).toEqual([]);
    await context.addCookies([
      { ...session, expires: Math.floor(Date.now() / 1000) - 1 },
    ]);
    await page.reload();
    await expect(
      page.getByRole("alert").filter({ hasText: "Operational query error:" }),
    ).toContainText("401");
    // The original launch URL remains reusable after the browser loses its cookie.
    await page.goto(launch);
    await expect(page.getByRole("heading", { name: "Timeline", exact: true })).toBeVisible();
    await page.reload();
    await expect(page.getByRole("heading", { name: "Timeline", exact: true })).toBeVisible();
    expect(cli(project, "dashboard").result.launch_url).toBe(launch);

    cli(
      project,
      "--lab-id",
      lab.lab_id,
      "case",
      "arm",
      "tx-response-lost",
      "--hold-ms",
      "30",
    );
    await context.close();
    expect(cli(project, "status").result.active_case.case_id).toBe(
      "tx-response-lost",
    );
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

test("retained viewer reads the original immutable Journal without restarting a Lab", async ({
  page,
}) => {
  const project = mkdtempSync(join(tmpdir(), "ronchi-viewer-"));
  let child: ReturnType<typeof spawn> | undefined;
  try {
    const lab = cli(
      project,
      "up",
      "--detach",
      "--rpc-listen",
      "127.0.0.1:0",
      "--control-listen",
      "127.0.0.1:0",
    ).result;
    normalDownWithDiagnostics(project, lab.lab_id);
    child = spawn(
      binary,
      [
        "--project",
        project,
        "--json",
        "--lab-id",
        lab.lab_id,
        "dashboard",
        "--serve",
      ],
      { stdio: ["ignore", "pipe", "pipe"] },
    );
    const ready = await new Promise<{ result: { launch_url: string } }>(
      (resolve, reject) => {
        const timer = setTimeout(
          () => reject(new Error("Viewer readiness deadline exceeded")),
          5000,
        );
        let text = "";
        child!.stdout!.on("data", (chunk) => {
          text += String(chunk);
          if (text.includes("\n")) {
            clearTimeout(timer);
            try {
              resolve(JSON.parse(text.split("\n")[0]));
            } catch (e) {
              reject(e);
            }
          }
        });
        child!.once("exit", (code) => {
          clearTimeout(timer);
          reject(new Error(`Viewer exited before readiness: ${code}`));
        });
      },
    );
    await page.goto(ready.result.launch_url);
    await expect(
      page.getByRole("heading", { name: "Timeline", exact: true }),
    ).toBeVisible();
    await expect(
      page.getByText("stopped · healthy", { exact: true }),
    ).toBeVisible();
    await page.getByText("Case controls", { exact: false }).click();
    await expect(
      page.getByRole("button", { name: "Disarm Case", exact: true }),
    ).toBeDisabled();
    await page.getByRole("button", { name: /^#/ }).first().click();
    await expect(
      page.getByRole("button", { name: "Load raw payload preview" }),
    ).toBeVisible();
    expect(cli(project, "--lab-id", lab.lab_id, "status").result.state).toBe(
      "stopped",
    );
    const replacement = cli(
      project,
      "up",
      "--detach",
      "--rpc-listen",
      "127.0.0.1:0",
      "--control-listen",
      "127.0.0.1:0",
    ).result;
    try {
      expect(replacement.lab_id).not.toBe(lab.lab_id);
      await page.reload();
      await expect(
        page.getByRole("button", { name: lab.lab_id, exact: true }),
      ).toBeVisible();
    } finally {
      cli(project, "--lab-id", replacement.lab_id, "down", "--force");
    }
    child.kill("SIGINT");
    await new Promise<void>((resolve) => child!.once("exit", () => resolve()));
    child = undefined;
  } finally {
    if (child) {
      child.kill("SIGKILL");
      await new Promise<void>((r) => child!.once("exit", () => r()));
    }
    rmSync(project, { recursive: true, force: true });
  }
});

test("response-loss Findings, public collection and explicit evaluation use real reference evidence", async ({
  page,
}) => {
  test.setTimeout(60000);
  const project = mkdtempSync(join(tmpdir(), "ronchi-browser-proof-"));
  let child: ReturnType<typeof spawn> | undefined;
  let labId: string | undefined;
  try {
    const report = cli(
      project,
      "demo",
      "tx-response-lost",
      "--profile",
      "unsafe",
    ).result;
    labId = report.lab_id;
    const status = cli(project, "--lab-id", report.lab_id, "status").result;
    const source = new URL(status.configuration.checks.sources[0].url);
    child = spawn(
      binary,
      [
        "--project",
        project,
        "--lab-id",
        report.lab_id,
        "--json",
        "reference",
        "relayer",
        "--name",
        report.reference_name,
        "--profile",
        "unsafe",
        "--listen",
        `${source.hostname}:${source.port}`,
      ],
      { stdio: ["ignore", "pipe", "pipe"] },
    );
    await new Promise<void>((resolve, reject) => {
      const timer = setTimeout(
        () => reject(new Error("Relayer readiness deadline")),
        5000,
      );
      let text = "";
      child!.stdout!.on("data", (chunk) => {
        text += String(chunk);
        if (text.includes("\n")) {
          clearTimeout(timer);
          resolve();
        }
      });
      child!.once("exit", (code) => {
        clearTimeout(timer);
        reject(new Error(`Relayer exited ${code}`));
      });
    });
    await page.goto(
      cli(project, "--lab-id", report.lab_id, "dashboard").result.launch_url,
    );
    await expect(page.locator(".findings button").first()).toBeVisible();
    await page.locator(".findings button").first().click();
    await expect(
      page.getByRole("heading", { name: "Observed statement" }),
    ).toBeVisible();
    await expect(
      page.getByRole("heading", { name: "Developer guidance" }),
    ).toBeVisible();
    await expect(page.getByText(/Pinned view/)).toBeVisible();
    const pinned = await page
      .locator(".summary .section-title span")
      .textContent();
    await page.getByText("Application Checks", { exact: true }).click();
    await page
      .getByLabel("Intent identity", { exact: true })
      .fill(report.intent_id);
    await page
      .getByRole("button", { name: "Collect observations", exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Refresh collection status" }),
    ).toBeVisible();
    await page
      .getByRole("button", { name: "Refresh collection status" })
      .click();
    // Collection does not silently move an open evidence detail to a new prefix.
    expect(
      await page.locator(".summary .section-title span").textContent(),
    ).toBe(pinned);
    await page
      .getByRole("button", { name: /Evaluate pinned snapshot/ })
      .click();
    await expect(page.getByLabel("Explicit Check evaluation")).toContainText(
      '"verdict": "fail"',
    );
    await expect(page.getByLabel("Explicit Check evaluation")).toContainText(
      "prepared",
    );
    await page
      .getByRole("button", { name: "Load raw payload preview" })
      .click();
    await expect(page.locator("aside pre").last()).toContainText("payload");
    child.kill("SIGINT");
    await new Promise<void>((r) => child!.once("exit", () => r()));
    child = undefined;
    cli(project, "--lab-id", report.lab_id, "down", "--force");
    child = spawn(
      binary,
      [
        "--project",
        project,
        "--json",
        "--lab-id",
        report.lab_id,
        "dashboard",
        "--serve",
      ],
      { stdio: ["ignore", "pipe", "pipe"] },
    );
    const retained = await new Promise<{ result: { launch_url: string } }>(
      (resolve, reject) => {
        const timer = setTimeout(
          () => reject(new Error("Retained readiness timeout")),
          5000,
        );
        let text = "";
        child!.stdout!.on("data", (chunk) => {
          text += String(chunk);
          if (text.includes("\n")) {
            clearTimeout(timer);
            resolve(JSON.parse(text.split("\n")[0]));
          }
        });
        child!.once("exit", (code) => {
          clearTimeout(timer);
          reject(new Error(`Viewer exited ${code}`));
        });
      },
    );
    const retainedUrl = new URL(retained.result.launch_url);
    retainedUrl.searchParams.set("snapshot", String(report.snapshot_seq));
    await page.goto(retainedUrl.toString());
    await expect(
      page.getByRole("heading", { name: "Timeline", exact: true }),
    ).toBeVisible();
    await page.getByText("Application Checks", { exact: true }).click();
    await page
      .getByLabel("Intent identity", { exact: true })
      .fill(report.intent_id);
    await expect(
      page.getByRole("button", { name: "Collect observations", exact: true }),
    ).toBeDisabled();
    await page
      .getByRole("button", { name: /Evaluate pinned snapshot/ })
      .click();
    await expect(page.getByLabel("Explicit Check evaluation")).toContainText(
      '"verdict": "fail"',
    );
  } finally {
    if (child) {
      child.kill("SIGINT");
      await new Promise<void>((r) => child!.once("exit", () => r()));
    }
    if (
      labId &&
      cli(project, "--lab-id", labId, "status").result.state === "ready"
    )
      cli(project, "--lab-id", labId, "down", "--force");
    rmSync(project, { recursive: true, force: true });
  }
});

test("real Findings continue at a pinned prefix and restore an event bookmark", async ({
  page,
  context,
}) => {
  const project = mkdtempSync(join(tmpdir(), "ronchi-findings-"));
  let id: string | undefined;
  try {
    const lab = cli(
      project,
      "up",
      "--detach",
      "--rpc-listen",
      "127.0.0.1:0",
      "--control-listen",
      "127.0.0.1:0",
    ).result;
    id = lab.lab_id;
    cli(project, "case", "arm", "tx-response-lost", "--hold-ms", "30");
    const { incident } = await import("./incidents");
    for (let seed = 1; seed <= 4; seed++) await incident(lab.rpc_address, seed);
    const expected = cli(project, "investigate", "findings", "--limit", "5");
    const next = cli(
      project,
      "investigate",
      "findings",
      "--limit",
      "5",
      "--cursor",
      expected.page.next_cursor,
    );
    expect(next.result.items.length).toBeGreaterThan(0);
    const launch = cli(project, "dashboard").result.launch_url;
    const url = new URL(launch);
    url.searchParams.set("snapshot", String(expected.view.snapshot_seq));
    await page.goto(url.toString());
    await expect(page.locator(".findings button")).toHaveCount(5);
    await page
      .getByRole("button", { name: "Next Findings", exact: true })
      .click();
    await expect(page.locator(".findings button")).toHaveCount(
      next.result.items.length,
    );
    // Later real incidents cannot enter the pinned page.
    await incident(lab.rpc_address, 5);
    expect(await page.locator(".findings button").count()).toBe(
      next.result.items.length,
    );
    await page.getByRole("button", { name: /^#/ }).first().focus();
    await page.keyboard.press("Enter");
    await expect(
      page.getByRole("button", { name: "Load raw payload preview" }),
    ).toBeVisible();
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    await page.getByRole("button", { name: "Copy evidence link" }).click();
    const saved = await page.evaluate(() => navigator.clipboard.readText());
    expect(saved).toContain("event=");
    expect(saved).not.toContain("ticket");
    expect(saved).not.toContain("cursor");
    const fresh = new URL(cli(project, "dashboard").result.launch_url);
    fresh.search = new URL(saved).search;
    await page.goto(fresh.toString());
    await expect(
      page.getByRole("button", { name: "Load raw payload preview" }),
    ).toBeVisible();
    await page.reload();
    await expect(
      page.getByRole("button", { name: "Load raw payload preview" }),
    ).toBeVisible();
    expect(page.url()).toContain("event=");
    expect(page.url()).not.toContain("ticket");
  } finally {
    if (id) cli(project, "--lab-id", id, "down", "--force");
    rmSync(project, { recursive: true, force: true });
  }
});

test("a Finding bookmark survives a new retained endpoint and name reuse", async ({
  page,
  context,
}) => {
  const project = mkdtempSync(join(tmpdir(), "ronchi-finding-link-"));
  let id: string | undefined,
    newId: string | undefined,
    viewer: ReturnType<typeof spawn> | undefined;
  try {
    const lab = cli(
      project,
      "up",
      "--detach",
      "--rpc-listen",
      "127.0.0.1:0",
      "--control-listen",
      "127.0.0.1:0",
    ).result;
    id = lab.lab_id;
    cli(project, "case", "arm", "tx-response-lost", "--hold-ms", "30");
    await (await import("./incidents")).incident(lab.rpc_address, 9);
    await page.goto(cli(project, "dashboard").result.launch_url);
    await page.locator(".findings button").first().click();
    await expect(
      page.getByRole("heading", { name: "Observed statement" }),
    ).toBeVisible();
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    await page.getByRole("button", { name: "Copy evidence link" }).click();
    const saved = new URL(
      await page.evaluate(() => navigator.clipboard.readText()),
    );
    expect(saved.searchParams.get("finding")).toMatch(/^f1-/);
    const fresh = cli(
      project,
      "dashboard",
      "--selection",
      saved.search.slice(1),
    ).result.launch_url;
    await page.goto(fresh);
    await expect(page.locator("aside pre").first()).toContainText("Ronchi");
    cli(project, "--lab-id", id, "down");
    newId = cli(
      project,
      "up",
      "--detach",
      "--rpc-listen",
      "127.0.0.1:0",
      "--control-listen",
      "127.0.0.1:0",
    ).result.lab_id;
    viewer = spawn(
      binary,
      [
        "--project",
        project,
        "--json",
        "--lab-id",
        id,
        "dashboard",
        "--serve",
        "--selection",
        saved.search.slice(1),
      ],
      { stdio: ["ignore", "pipe", "pipe"] },
    );
    const ready = await new Promise<{ result: { launch_url: string } }>(
      (resolve, reject) => {
        let text = "";
        const timer = setTimeout(
          () => reject(new Error("Viewer readiness timeout")),
          5000,
        );
        viewer!.stdout!.on("data", (c) => {
          text += String(c);
          if (text.includes("\n")) {
            clearTimeout(timer);
            resolve(JSON.parse(text.split("\n")[0]));
          }
        });
        viewer!.once("exit", (code) => {
          clearTimeout(timer);
          reject(new Error(`Viewer exited ${code}`));
        });
      },
    );
    await page.goto(ready.result.launch_url);
    await expect(page.locator("aside pre").first()).toContainText("Ronchi");
    await expect(
      page.getByRole("button", { name: id, exact: true }),
    ).toBeVisible();
    expect(page.url()).toContain(
      `finding=${saved.searchParams.get("finding")}`,
    );
    expect(page.url()).not.toContain("ticket");
  } finally {
    if (viewer) {
      viewer.kill("SIGINT");
      await new Promise<void>((resolve) =>
        viewer!.once("exit", () => resolve()),
      );
    }
    for (const known of [newId, id])
      if (
        known &&
        cli(project, "--lab-id", known, "status").result.state === "ready"
      )
        cli(project, "--lab-id", known, "down", "--force");
    rmSync(project, { recursive: true, force: true });
  }
});
