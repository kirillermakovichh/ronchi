import { describe, expect, it } from "vitest";
import {
  initialLayout,
  restoreLayout,
  swapWidgets,
  resizeColumns,
  resizeRows,
  adjacentSlot,
  widgets,
} from "./layout";
describe("fixed-panel workspace", () => {
  it("moves panels by exchanging occupied slots without hiding any widget", () => {
    const next = swapWidgets(initialLayout(), "traffic", "inspector");
    expect(next.order).toEqual([
      "cases",
      "observations",
      "inspector",
      "traffic",
    ]);
    expect([...next.order].sort()).toEqual([...widgets].sort());
    expect(next.columns).toEqual(initialLayout().columns);
  });
  it("rejects invalid persisted layouts and ignores obsolete hidden-panel preferences", () => {
    expect(restoreLayout([{ id: "traffic", hidden: true }])).toEqual(
      initialLayout(),
    );
    const x = initialLayout();
    x.order[1] = "cases";
    expect(restoreLayout(x)).toEqual(initialLayout());
    expect(
      restoreLayout({ ...initialLayout(), columns: [0.5, 0.5, 0.5] }),
    ).toEqual(initialLayout());
  });
  it("resizes adjacent columns while retaining all of the viewport", () => {
    const next = resizeColumns(initialLayout(), 0, 100, 1280);
    expect(next.columns[0]).toBeGreaterThan(0.25);
    expect(next.columns[2]).toBe(initialLayout().columns[2]);
    expect(next.columns.reduce((a, b) => a + b, 0)).toBeCloseTo(1);
    expect(restoreLayout(next)).toEqual(next);
    const clamped = resizeColumns(next, 1, 99999, 1280);
    expect(clamped.columns[2] * 1280).toBeCloseTo(240);
  });
  it("retains a valid minimum-height panel on a large monitor", () => {
    const next = resizeRows(initialLayout(), 99999, 2000);
    expect(restoreLayout(next)).toEqual(next);
  });
  it("clamps row sizes and provides directional keyboard moves", () => {
    expect(resizeRows(initialLayout(), 99999, 600).row).toBeCloseTo(
      1 - 140 / 600,
    );
    expect(adjacentSlot(2, "ArrowRight")).toBe(3);
    expect(adjacentSlot(0, "ArrowDown")).toBe(1);
    expect(adjacentSlot(3, "ArrowRight")).toBeUndefined();
  });
});
