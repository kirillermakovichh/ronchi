import { createVirtualizer } from "@tanstack/solid-virtual";
import { createEffect, For, type JSX } from "solid-js";
export function VirtualList<T>(props: {
  items: readonly T[];
  height: number;
  label: string;
  children: (item: T, index: number) => JSX.Element;
}) {
  let viewport!: HTMLDivElement;
  const keyOf = (item: T, index: number) => {
    const value = item as
      | { id?: string; event_ref?: { lab_id?: string; event_seq?: unknown } }
      | undefined;
    return (
      value?.id ??
      (value?.event_ref
        ? `${value.event_ref.lab_id}:${value.event_ref.event_seq}`
        : index)
    );
  };
  let previous: (string | number)[] = [];
  createEffect(() => {
    const keys = props.items.map(keyOf);
    const scroll = viewport?.scrollTop ?? 0;
    if (scroll > props.height && previous.length) {
      const index = Math.min(
        previous.length - 1,
        Math.floor(scroll / props.height),
      );
      const next = keys.indexOf(previous[index]);
      if (next >= 0 && next !== index)
        viewport.scrollTop =
          next * props.height + (scroll - index * props.height);
    }
    previous = keys;
  });
  const list = createVirtualizer({
    get count() {
      return props.items.length;
    },
    getScrollElement: () => viewport,
    estimateSize: () => props.height,
    getItemKey: (index) => keyOf(props.items[index], index),
    overscan: 5,
  });
  return (
    <div
      class="virtual-scroll"
      ref={viewport}
      aria-label={props.label}
      role="list"
    >
      <div class="virtual-space" style={{ height: `${list.getTotalSize()}px` }}>
        <For each={list.getVirtualItems()}>
          {(row) => (
            <div
              role="listitem"
              class="virtual-row"
              style={{
                height: `${row.size}px`,
                transform: `translateY(${row.start}px)`,
              }}
            >
              {props.children(props.items[row.index], row.index)}
            </div>
          )}
        </For>
      </div>
    </div>
  );
}
