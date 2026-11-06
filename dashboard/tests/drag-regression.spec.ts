import { test, expect } from "@playwright/test";
import { cli, project, up, clean } from "./helpers";
test("a visible widget moves by its title with all four panels present", async ({
  page,
}) => {
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
    const start = await traffic.boundingBox(),
      title = await traffic.locator(".widget-head strong").boundingBox(),
      target = await inspector.boundingBox();
    if (!start || !title || !target) throw new Error("Missing panels");
    await page.mouse.move(
      title.x + title.width / 2,
      title.y + title.height / 2,
    );
    await page.mouse.down();
    await page.mouse.move(
      target.x + target.width / 2,
      title.y + title.height / 2,
      { steps: 20 },
    );
    await page.mouse.up();
    await expect
      .poll(async () => Math.round((await traffic.boundingBox())!.x), {
        timeout: 1500,
      })
      .not.toBe(Math.round(start.x));
    for (const widget of ["cases", "observations", "traffic", "inspector"])
      await expect(page.locator(`[data-widget="${widget}"]`)).toBeVisible();
  } finally {
    clean(path, id);
  }
});

test("Escape and dropping outside Workspace leave the four-panel layout unchanged", async ({
  page,
}) => {
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
    const panel = page.locator('[data-widget="traffic"]'),
      header = await panel.locator(".widget-head").boundingBox(),
      before = await panel.boundingBox(),
      target = await page.locator('[data-widget="inspector"]').boundingBox();
    if (!header || !before || !target) throw new Error("Missing widget");
    await page.mouse.move(header.x + 100, header.y + 18);
    await page.mouse.down();
    await page.mouse.move(target.x + 100, header.y + 18, { steps: 12 });
    await expect(page.locator(".widget.drop-target")).toHaveCount(1);
    await page.keyboard.press("Escape");
    await page.mouse.up();
    expect(Math.round((await panel.boundingBox())!.x)).toBe(
      Math.round(before.x),
    );
    await page.mouse.move(header.x + 100, header.y + 18);
    await page.mouse.down();
    await page.mouse.move(1490, 945, { steps: 15 });
    await page.mouse.up();
    expect(Math.round((await panel.boundingBox())!.x)).toBe(
      Math.round(before.x),
    );
    await expect(page.locator(".widget-drag-overlay")).toHaveCount(0);
  } finally {
    clean(path, id);
  }
});
