export const widgets = [
  "cases",
  "observations",
  "traffic",
  "inspector",
] as const;
export type Widget = (typeof widgets)[number];
export interface Layout {
  order: Widget[];
  columns: [number, number, number];
  row: number;
}
export const initialLayout = (): Layout => ({
  order: [...widgets],
  columns: [0.25, 1 / 3, 5 / 12],
  row: 0.52,
});
export function swapWidgets(layout: Layout, from: Widget, to: Widget): Layout {
  const a = layout.order.indexOf(from),
    b = layout.order.indexOf(to);
  if (a < 0 || b < 0 || a === b) return layout;
  const order = [...layout.order];
  [order[a], order[b]] = [order[b], order[a]];
  return { ...layout, order };
}
export function resizeColumns(
  layout: Layout,
  boundary: 0 | 1,
  delta: number,
  width: number,
): Layout {
  if (!Number.isFinite(delta) || width <= 0) return layout;
  const columns = [...layout.columns] as Layout["columns"],
    sum = columns[boundary] + columns[boundary + 1],
    min = Math.min(240 / width, sum / 2);
  columns[boundary] = Math.max(
    min,
    Math.min(sum - min, columns[boundary] + delta / width),
  );
  columns[boundary + 1] = sum - columns[boundary];
  return { ...layout, columns };
}
export function resizeRows(
  layout: Layout,
  delta: number,
  height: number,
): Layout {
  if (!Number.isFinite(delta) || height <= 0) return layout;
  const min = Math.min(140 / height, 0.45);
  return {
    ...layout,
    row: Math.max(min, Math.min(1 - min, layout.row + delta / height)),
  };
}
export function restoreLayout(value: unknown): Layout {
  if (!value || typeof value !== "object") return initialLayout();
  const x = value as Layout;
  if (
    !Array.isArray(x.order) ||
    x.order.length !== 4 ||
    new Set(x.order).size !== 4 ||
    x.order.some((id) => !widgets.includes(id)) ||
    !Array.isArray(x.columns) ||
    x.columns.length !== 3 ||
    x.columns.some((n) => !Number.isFinite(n) || n <= 0 || n >= 1) ||
    Math.abs(x.columns.reduce((a, b) => a + b, 0) - 1) > 0.001 ||
    !Number.isFinite(x.row) ||
    x.row <= 0 ||
    x.row >= 1
  )
    return initialLayout();
  return { order: [...x.order], columns: [...x.columns], row: x.row };
}
export function adjacentSlot(slot: number, key: string): number | undefined {
  return key === "ArrowRight"
    ? slot < 2
      ? 2
      : slot === 2
        ? 3
        : undefined
    : key === "ArrowLeft"
      ? slot === 3
        ? 2
        : slot === 2
          ? 0
          : undefined
      : key === "ArrowDown" && slot === 0
        ? 1
        : key === "ArrowUp" && slot === 1
          ? 0
          : undefined;
}
