import { test, expect } from "@playwright/test";
import { cli, project, up, clean } from "./helpers";
test("widget controls preserve forms, layout and navigation", async ({
  page,
}, info) => {
  const path = project();
  let id: string | undefined;
  try {
    const lab = up(path);
    id = lab.lab_id;
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.goto(cli(path, "dashboard").result.launch_url);
    await expect(
      page.getByRole("button", { name: "Arm Case", exact: true }),
    ).toBeEnabled();
    await page.getByText("Custom Case", { exact: true }).click();
    await page.getByLabel("Custom Case YAML").fill("remember this draft");
    for (const widget of ["cases", "observations", "traffic", "inspector"])
      await expect(page.locator(`[data-widget="${widget}"]`)).toBeVisible();
    await expect(
      page.getByRole("button", {
        name: /Add widget|Hide |Collapse |Maximize /,
      }),
    ).toHaveCount(0);
    await page
      .getByRole("navigation")
      .getByRole("button", { name: "Lab", exact: true })
      .click();
    await expect(
      page.getByRole("heading", { name: "Lab", exact: true }),
    ).toBeVisible();
    await page
      .getByRole("navigation")
      .getByRole("button", { name: "Workspace", exact: true })
      .click();
    await expect(page.getByLabel("Custom Case YAML")).toHaveValue(
      "remember this draft",
    );
    await page.reload();
    await expect(page.locator('[data-widget="observations"]')).toBeVisible();
    await page
      .getByRole("button", { name: "Reset layout", exact: true })
      .click();
    await expect(page.locator('[data-widget="observations"]')).toBeVisible();
    await page.screenshot({ path: info.outputPath("workspace-laptop.png") });
  } finally {
    clean(path, id);
  }
});

test("one WebSocket stays idle, resynchronizes and never replays a mutation after lost acknowledgement", async ({
  page,
}) => {
  const path = project();
  let id: string | undefined;
  let calls = 0,
    frames = 0;
  const http: string[] = [];
  let drop = false;
  try {
    const lab = up(path);
    id = lab.lab_id;
    await page.routeWebSocket("**/socket", (socket) => {
      const server = socket.connectToServer();
      let command: string | undefined;
      socket.onMessage((message) => {
        frames++;
        const f = JSON.parse(String(message));
        if (f.type === "call" && f.path === "api/case/arm") {
          calls++;
          command = f.id;
        }
        server.send(message);
      });
      server.onMessage((message) => {
        const f = JSON.parse(String(message));
        if (drop && f.type === "reply" && f.id === command) {
          drop = false;
          socket.close({ code: 1012, reason: "lost acknowledgement" });
          server.close();
        } else socket.send(message);
      });
    });
    page.on("request", (r) => {
      if (/\/(api|v1)\//.test(r.url())) http.push(r.url());
    });
    await page.goto(cli(path, "dashboard").result.launch_url);
    await expect(
      page.getByRole("button", { name: "Arm Case", exact: true }),
    ).toBeEnabled();
    const count = frames;
    await page.waitForTimeout(700);
    expect(frames).toBe(count);
    drop = true;
    await page.getByRole("button", { name: "Arm Case", exact: true }).click();
    await expect(
      page.getByRole("alert").filter({ hasText: "effects: unknown" }),
    ).toBeVisible();
    await expect(page.locator(".case-state")).toContainText("waiting", {
      timeout: 10000,
    });
    expect(calls).toBe(1);
    expect(http).toEqual([]);
  } finally {
    clean(path, id);
  }
});

test("header drag, separator resizing and keyboard moves keep all widgets visible and persist", async ({
  page,
}, info) => {
  const path = project();
  let id: string | undefined;
  try {
    const lab = up(path);
    id = lab.lab_id;
    await page.setViewportSize({ width: 1500, height: 950 });
    await page.goto(cli(path, "dashboard").result.launch_url);
    await expect(
      page.getByRole("button", { name: "Arm Case", exact: true }),
    ).toBeEnabled();
    const traffic = page.locator('[data-widget="traffic"]'),
      inspector = page.locator('[data-widget="inspector"]');
    const before = await traffic.boundingBox(),
      from = await traffic.locator(".widget-head").boundingBox(),
      to = await inspector.locator(".widget-head").boundingBox();
    if (!from || !to || !before) throw new Error("Missing header");
    await page.mouse.move(from.x + 80, from.y + 20);
    await page.mouse.down();
    await page.mouse.move(to.x + to.width / 2, to.y + 20, { steps: 20 });
    await expect(page.locator(".widget.drop-target")).toHaveCount(1);
    await page.mouse.up();
    await expect
      .poll(async () => Math.round((await traffic.boundingBox())!.x))
      .not.toBe(Math.round(before.x));
    const separator = page.getByRole("separator", {
        name: "Resize Right column",
        exact: true,
      }),
      sep = await separator.boundingBox(),
      width = (await traffic.boundingBox())!.width;
    if (!sep) throw new Error("Missing resize separator");
    await page.mouse.move(sep.x + sep.width / 2, sep.y + 120);
    await page.mouse.down();
    await page.mouse.move(sep.x - 80, sep.y + 120, { steps: 10 });
    await page.mouse.up();
    await expect
      .poll(async () => (await traffic.boundingBox())!.width)
      .toBeGreaterThan(width + 40);
    const after = await traffic.boundingBox();
    await page.reload();
    await expect
      .poll(async () => Math.round((await traffic.boundingBox())!.width))
      .toBe(Math.round(after!.width));
    for (const widget of ["cases", "observations", "traffic", "inspector"])
      await expect(page.locator(`[data-widget="${widget}"]`)).toBeVisible();
    await page
      .getByRole("button", { name: "Move Traffic", exact: true })
      .focus();
    await page.keyboard.press("ArrowLeft");
    await expect
      .poll(async () => Math.round((await traffic.boundingBox())!.x))
      .toBeLessThan(Math.round(after!.x));
    await page.screenshot({ path: info.outputPath("workspace-dragged.png") });
  } finally {
    clean(path, id);
  }
});
