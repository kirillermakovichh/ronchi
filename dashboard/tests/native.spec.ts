import { test, expect } from "@playwright/test";
import { cli, project, up, clean, rpc } from "./helpers";
test("Solid Workspace displays native live traffic and keeps selection across updates", async ({
  page,
}, info) => {
  const path = project();
  let id: string | undefined;
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  try {
    const lab = up(path);
    id = lab.lab_id;
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto(cli(path, "dashboard").result.launch_url);
    await expect(
      page
        .getByRole("navigation")
        .getByRole("button", { name: "Workspace", exact: true }),
    ).toHaveAttribute("aria-current", "page");
    await expect(
      page.getByRole("button", { name: "Arm Case", exact: true }),
    ).toBeEnabled();
    await rpc(lab.rpc_address, "getLatestBlockhash");
    await expect(
      page.locator('[data-widget="traffic"] .traffic-row'),
    ).toHaveCount(1);
    await page.locator('[data-widget="traffic"] .traffic-row').first().click();
    await expect(
      page.getByRole("heading", { name: "getLatestBlockhash", exact: true }),
    ).toBeVisible();
    await rpc(lab.rpc_address, "getBlockHeight");
    await expect(
      page.locator('[data-widget="traffic"] .traffic-row'),
    ).toHaveCount(2);
    await expect(
      page.getByRole("heading", { name: "getLatestBlockhash", exact: true }),
    ).toBeVisible();
    await page.screenshot({ path: info.outputPath("workspace-desktop.png") });
    expect(errors).toEqual([]);
  } finally {
    clean(path, id);
  }
});

test("a fault episode appears before follow-ups, then its expanded path receives actual lookup replies", async ({
  page,
}, info) => {
  const { incident } = await import("./incidents");
  const path = project();
  let id: string | undefined;
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  try {
    const lab = up(path);
    id = lab.lab_id;
    await page.setViewportSize({ width: 1600, height: 950 });
    await page.goto(cli(path, "dashboard").result.launch_url);
    await page.getByRole("button", { name: "Arm Case", exact: true }).click();
    await expect(
      page.getByText("Case enabled.", { exact: false }),
    ).toBeVisible();
    const sig = await incident(lab.rpc_address, 77, false, 700);
    await expect(page.locator(".observation-row")).toHaveCount(1);
    await page.locator(".observation-row").click();
    await expect(page.locator(".path")).toContainText(
      "Ronchi withheld the response",
    );
    await expect(page.locator(".observed-result h2")).toContainText(
      "Result unknown",
    );
    await rpc(lab.rpc_address, "getSignatureStatuses", [[sig]]);
    await expect(page.locator(".path")).toContainText(
      "Application checked status",
    );
    await expect
      .poll(
        async () =>
          (await rpc(lab.rpc_address, "getSignatureStatuses", [[sig]])).result
            .value[0]?.confirmationStatus,
        { timeout: 10000 },
      )
      .toBe("finalized");
    await expect(page.locator(".path")).toContainText(
      /Simulated RPC reply: (processed|confirmed|finalized)/,
    );
    await page.locator(".path details").first().locator("summary").click();
    await expect(page.locator(".path details").first()).toHaveAttribute(
      "open",
      "",
    );
    await rpc(lab.rpc_address, "getSignatureStatuses", [[sig]]);
    await expect(page.locator(".path details").first()).toHaveAttribute(
      "open",
      "",
    );
    await expect(page.locator(".observation-row")).toHaveCount(1);
    await expect(page.locator(".traffic-row.related").first()).toBeVisible();
    await page
      .locator('[data-widget="traffic"] .traffic-row')
      .filter({ hasText: "getSignatureStatuses" })
      .first()
      .click();
    await expect(
      page.getByRole("heading", { name: "getSignatureStatuses", exact: true }),
    ).toBeVisible();
    const copy = page.getByRole("button", {
      name: "Copy signature",
      exact: true,
    });
    await expect(copy).toBeVisible();
    await expect(copy).toHaveText("");
    await expect(copy.locator(".copy-glyph")).toHaveCount(1);
    expect((await copy.locator(".copy-glyph").boundingBox())?.width).toBe(14);
    await expect(
      page.locator('[data-widget="traffic"] .list-footer'),
    ).not.toContainText("Snapshot");
    await page.screenshot({ path: info.outputPath("signature-copy-icon.png") });
    await page
      .getByRole("button", { name: "← Back to Observation", exact: true })
      .click();
    await expect(page.locator(".path details").first()).toHaveAttribute(
      "open",
      "",
    );
    await page
      .getByRole("button", { name: "Technical details", exact: true })
      .click();
    await expect(
      page.getByRole("heading", { name: "Supporting evidence", exact: true }),
    ).toBeVisible();
    await page.screenshot({ path: info.outputPath("observation-desktop.png") });
    await page.setViewportSize({ width: 1280, height: 720 });
    await page
      .locator('[data-widget="inspector"] .widget-body')
      .evaluate((e) => (e.scrollTop = 0));
    await page.screenshot({ path: info.outputPath("observation-laptop.png") });
    await page.getByRole("button", { name: "Filters", exact: true }).click();
    await page.getByLabel("kind", { exact: true }).fill("rpc.registered");
    await page
      .getByRole("button", { name: "Apply filters", exact: true })
      .click();
    await expect(page.locator(".observation-row")).toHaveCount(0);
    await page
      .getByRole("button", { name: "Clear filters", exact: true })
      .click();
    await expect(page.locator(".observation-row")).toHaveCount(1);
    expect(errors).toEqual([]);
  } finally {
    clean(path, id);
  }
});

test("exact evidence bookmarks reopen a stopped Lab in the retained Solid viewer", async ({
  page,
}) => {
  const { down, viewer } = await import("./helpers");
  const path = project();
  let id: string | undefined, v: Awaited<ReturnType<typeof viewer>> | undefined;
  try {
    const lab = up(path);
    id = lab.lab_id;
    await rpc(lab.rpc_address, "getLatestBlockhash");
    const r = cli(path, "investigate", "timeline", "--kind", "rpc.registered");
    const seq = r.result.items[0].event_ref.event_seq;
    down(path, id!);
    v = await viewer(path, id!);
    const url = new URL(v.launch_url);
    url.searchParams.set("page", "journal");
    url.searchParams.set("snapshot", String(r.view.snapshot_seq));
    url.searchParams.set("event", String(seq));
    await page.goto(url.toString());
    await expect(
      page.getByRole("heading", { name: "getLatestBlockhash", exact: true }),
    ).toBeVisible();
    await page
      .getByRole("navigation")
      .getByRole("button", { name: "Workspace", exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Arm Case", exact: true }),
    ).toBeDisabled();
    await expect(page.locator(".case-controls")).toContainText("read-only");
    expect(cli(path, "--lab-id", id!, "status").result.state).toBe("stopped");
  } finally {
    await v?.stop();
    clean(path, id);
  }
});

test("native Surfpool episodes show returned status without treating source acceptance as business completion", async ({
  page,
}, info) => {
  test.skip(
    !process.env.RONCHI_TEST_SURFPOOL_ARCHIVE,
    "Requires the pinned native archive",
  );
  const { mkdirSync, writeFileSync } = await import("node:fs"),
    { join } = await import("node:path"),
    { signerAddress, incident } = await import("./incidents");
  const path = project();
  let id: string | undefined;
  try {
    const payer = signerAddress(73);
    mkdirSync(join(path, "ronchi"));
    writeFileSync(
      join(path, "snapshot.json"),
      JSON.stringify({
        [payer]: {
          lamports: 1000000000,
          owner: "11111111111111111111111111111111",
          executable: false,
          rentEpoch: 0,
          data: "",
          parsedData: null,
        },
      }),
    );
    writeFileSync(
      join(path, "ronchi/config.toml"),
      "schema_version=1\nsurfpool_snapshot='snapshot.json'\n",
    );
    cli(
      path,
      "tool",
      "surfpool",
      "install",
      "--archive",
      process.env.RONCHI_TEST_SURFPOOL_ARCHIVE!,
    );
    const lab = up(path, "--mode", "surfpool");
    id = lab.lab_id;
    await page.setViewportSize({ width: 1600, height: 900 });
    await page.goto(cli(path, "dashboard").result.launch_url);
    await page.getByRole("button", { name: "Arm Case", exact: true }).click();
    await expect(page.locator(".confirmation")).toContainText("Case enabled");
    const sig = await incident(lab.rpc_address, 73, false, 700);
    await expect(page.locator(".observation-row")).toHaveCount(1);
    await page.locator(".observation-row").click();
    await expect(page.locator(".path")).toContainText(
      "Surfpool returned a signature",
    );
    await expect
      .poll(
        async () =>
          (await rpc(lab.rpc_address, "getSignatureStatuses", [[sig]])).result
            .value[0]?.confirmationStatus,
        { timeout: 12000 },
      )
      .toMatch(/confirmed|finalized/);
    await expect(page.locator(".path")).toContainText(
      /RPC reply: (confirmed|finalized) · err: null/,
    );
    await expect(page.locator(".case-state")).toContainText("1 applied");
    await page.screenshot({
      path: info.outputPath("surfpool-observation.png"),
    });
    await page
      .getByRole("button", { name: "Technical details", exact: true })
      .click();
    await expect(page.locator(".technical").first()).toContainText(
      "business completion",
    );
  } finally {
    clean(path, id);
  }
});
