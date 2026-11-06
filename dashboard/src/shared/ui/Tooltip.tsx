import {
  createEffect,
  createSignal,
  createUniqueId,
  onCleanup,
  Show,
  type JSX,
} from "solid-js";
import "./tooltip.css";

export function Tooltip(props: {
  children: JSX.Element;
  content: JSX.Element;
}) {
  const id = createUniqueId();
  const [hovered, setHovered] = createSignal(false);
  const [focused, setFocused] = createSignal(false);
  const [dismissed, setDismissed] = createSignal(false);
  const open = () => (hovered() || focused()) && !dismissed();
  createEffect(() => {
    if (!open()) return;
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setDismissed(true);
        event.stopPropagation();
      }
    };
    document.addEventListener("keydown", escape);
    onCleanup(() => document.removeEventListener("keydown", escape));
  });
  return (
    <span
      class="tooltip-anchor"
      tabIndex={0}
      aria-describedby={open() ? id : undefined}
      onMouseEnter={() => {
        setHovered(true);
        setDismissed(false);
      }}
      onMouseLeave={() => setHovered(false)}
      onFocus={() => {
        setFocused(true);
        setDismissed(false);
      }}
      onBlur={() => setFocused(false)}
    >
      {props.children}
      <Show when={open()}>
        <span class="tooltip-content" role="tooltip" id={id}>
          {props.content}
        </span>
      </Show>
    </span>
  );
}
