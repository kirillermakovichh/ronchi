import { test, expect } from "@playwright/test";
import { cli, project, up, clean, viewer } from "./helpers";

test("Lab-wide rates count batch calls, update while idle without requests and freeze in retained history", async ({
  page,
}, info) => {
  const path = project();
  let id: string | undefined;
  let retained: Awaited<ReturnType<typeof viewer>> | undefined;
  let frames = 0;
  const http: string[] = [];
  try {
    const lab = up(path);
    id = lab.lab_id;
    await page.routeWebSocket("**/socket", (socket) => {
      const server = socket.connectToServer();
      socket.onMessage((message) => {
        frames++;
        server.send(message);
      });
      server.onMessage((message) => socket.send(message));
    });
    page.on("request", (r) => {
      if (/\/(api|v1)\//.test(r.url())) http.push(r.url());
    });
    await page.setViewportSize({ width: 1280, height: 720 });
    await page.goto(cli(path, "dashboard").result.launch_url);
    await expect(page.getByTestId("rps-active")).toHaveText("0.0");
    const response = await fetch(`http://${lab.rpc_address}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(
        ["getBlockHeight", "getSlot", "getHealth"].map((method, id) => ({
          jsonrpc: "2.0",
          id,
          method,
        })),
      ),
    });
    expect(((await response.json()) as unknown[]).length).toBe(3);
    await expect(page.getByTestId("rps-active")).toHaveText("3.0");
    expect(cli(path, "status").result.rpc_traffic.requests).toBe(3);
    // Filtering away every row must not change Lab-wide counts.
    await page
      .getByRole("combobox", { name: "RPC method filter" })
      .selectOption("sendTransaction");
    await expect(
      page
        .locator('[data-widget="traffic"]')
        .getByText("No matching traffic", { exact: true }),
    ).toBeVisible();
    await expect(page.getByTestId("rps-active")).toHaveText("3.0");
    // Wait for the subscription update before measuring idle socket traffic.
    await expect
      .poll(async () => Number(await page.getByTestId("rps-avg").textContent()))
      .toBeLessThan(3);
    const before = Number(await page.getByTestId("rps-avg").textContent());
    const idleFrames = frames;
    await expect
      .poll(async () => Number(await page.getByTestId("rps-avg").textContent()))
      .toBeLessThan(before);
    expect(frames).toBe(idleFrames);
    expect(http).toEqual([]);
    await page.screenshot({ path: info.outputPath("rpc-rates-laptop.png") });
    cli(path, "--lab-id", id, "down");
    const stopped = cli(path, "--lab-id", id, "status").result.rpc_traffic;
    expect(stopped.running).toBe(false);
    expect(stopped.end_known).toBe(true);
    expect(stopped.requests).toBe(3);
    retained = await viewer(path, id);
    await page.goto(retained.launch_url);
    const expected = (
      3e9 /
      Math.max(1e9, stopped.observed_elapsed_ns - stopped.first_elapsed_ns)
    ).toFixed(1);
    await expect(page.getByTestId("rps-avg")).toHaveText(expected);
    await expect(page.getByTestId("rps-active")).toHaveText("3.0");
    await page.waitForTimeout(1200);
    await expect(page.getByTestId("rps-avg")).toHaveText(expected);
    expect(cli(path, "--lab-id", id, "status").result.rpc_traffic).toEqual(
      stopped,
    );
  } finally {
    await retained?.stop();
    clean(path, id);
  }
});
