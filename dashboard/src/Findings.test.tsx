// @vitest-environment jsdom
import { it, expect, vi } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Findings } from "./Findings";
import { Api } from "./api";
it("keeps the valid Findings page when bounded continuation fails", async () => {
  const fetcher = vi
    .spyOn(globalThis, "fetch")
    .mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          contract_version: 1,
          result: { items: [{ finding_id: "old", rule_id: "Observed rule" }] },
          view: { snapshot_seq: 9 },
          page: { next_cursor: "opaque" },
        }),
      ),
    )
    .mockResolvedValueOnce(new Response("source unavailable", { status: 503 }));
  try {
    render(
      <Findings
        api={new Api("L1")}
        scope={{}}
        snapshot={9}
        items={[{ finding_id: "old", rule_id: "Observed rule" }]}
        more
        pin={() => {}}
        select={() => {}}
      />,
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Next Findings" }),
    );
    expect(await screen.findByRole("alert")).toHaveProperty(
      "textContent",
      expect.stringContaining("previous Findings page remains visible"),
    );
    expect(screen.getByRole("button", { name: /Observed rule/ })).toBeTruthy();
    expect(fetcher).toHaveBeenCalledTimes(2);
    const first = JSON.parse(String(fetcher.mock.calls[0][1]?.body)),
      next = JSON.parse(String(fetcher.mock.calls[1][1]?.body));
    expect(first.arguments.snapshot_seq).toBe(9);
    expect(next.arguments).toEqual({ ...first.arguments, cursor: "opaque" });
  } finally {
    cleanup();
    fetcher.mockRestore();
  }
});
