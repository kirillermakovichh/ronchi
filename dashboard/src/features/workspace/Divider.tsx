import { onCleanup, type Accessor } from "solid-js";
import type { Layout } from "./layout";
export function Divider(props: {
  name: string;
  orientation: "horizontal" | "vertical";
  position: Accessor<number>;
  left?: Accessor<string>;
  top?: Accessor<string>;
  width?: Accessor<string>;
  grid: Accessor<HTMLElement>;
  layout: Accessor<Layout>;
  change: (layout: Layout, delta: number, size: number) => Layout;
  preview: (layout: Layout) => void;
  save: (layout: Layout) => void;
}) {
  let cleanup: (() => void) | undefined;
  onCleanup(() => cleanup?.());
  function start(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    cleanup?.();
    const node = e.currentTarget as HTMLElement,
      rect = props.grid().getBoundingClientRect(),
      base = props.layout(),
      origin = props.orientation === "vertical" ? e.clientX : e.clientY,
      size = props.orientation === "vertical" ? rect.width : rect.height;
    let pending: Layout | undefined, frame: number | undefined;
    node.setPointerCapture(e.pointerId);
    node.classList.add("resizing");
    const paint = () => {
      frame = undefined;
      if (pending) props.preview(pending);
    };
    const move = (event: PointerEvent) => {
      pending = props.change(
        base,
        (props.orientation === "vertical" ? event.clientX : event.clientY) -
          origin,
        size,
      );
      if (frame === undefined) frame = requestAnimationFrame(paint);
    };
    const finish = () => {
      if (frame !== undefined) cancelAnimationFrame(frame);
      paint();
      props.save(pending ?? base);
      node.removeEventListener("pointermove", move);
      node.removeEventListener("pointerup", finish);
      node.removeEventListener("pointercancel", cancel);
      node.classList.remove("resizing");
      if (node.hasPointerCapture(e.pointerId))
        node.releasePointerCapture(e.pointerId);
      document.removeEventListener("keydown", key);
      window.removeEventListener("blur", cancel);
      cleanup = undefined;
    };
    const cancel = () => {
      pending = base;
      finish();
    };
    const key = (event: KeyboardEvent) => {
      if (event.key === "Escape") cancel();
    };
    document.addEventListener("keydown", key);
    window.addEventListener("blur", cancel);
    cleanup = cancel;
    node.addEventListener("pointermove", move);
    node.addEventListener("pointerup", finish);
    node.addEventListener("pointercancel", cancel);
  }
  return (
    <div
      role="separator"
      tabIndex={0}
      aria-label={`Resize ${props.name}`}
      aria-orientation={props.orientation}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(props.position() * 100)}
      class={`panel-divider ${props.orientation}`}
      style={{
        left: props.left?.(),
        top: props.top?.(),
        width: props.width?.(),
      }}
      onPointerDown={start}
      onKeyDown={(e) => {
        if (!e.key.startsWith("Arrow")) return;
        e.preventDefault();
        const rect = props.grid().getBoundingClientRect();
        props.save(
          props.change(
            props.layout(),
            e.key === "ArrowLeft" || e.key === "ArrowUp" ? -16 : 16,
            props.orientation === "vertical" ? rect.width : rect.height,
          ),
        );
      }}
    />
  );
}
