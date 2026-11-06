import {
  createEffect,
  createSignal,
  For,
  onCleanup,
  type Accessor,
  type JSX,
} from "solid-js";
import {
  DragDropProvider,
  DragDropSensors,
  DragOverlay,
  createDraggable,
  createDroppable,
  useDragDropContext,
  type CollisionDetector,
} from "@thisbeyond/solid-dnd";
import { Divider } from "./Divider";
import { useLab } from "../../app/context";
import { Icon } from "../../shared/ui/primitives";
import {
  initialLayout,
  restoreLayout,
  resizeColumns,
  resizeRows,
  swapWidgets,
  adjacentSlot,
  widgets,
  type Layout,
  type Widget,
} from "./layout";
const titles: Record<Widget, string> = {
  cases: "Cases",
  observations: "Observations",
  traffic: "Traffic",
  inspector: "Inspector",
};
// Hit-test the pointer, rather than the centre of differently sized panels.
const pointerTarget: CollisionDetector = (draggable, droppables) => {
  const p = draggable.data.pointer as
    (() => { x: number; y: number } | undefined) | undefined;
  const point = p?.();
  return point
    ? (droppables.find(
        (d) =>
          point.x >= d.layout.left &&
          point.x <= d.layout.right &&
          point.y >= d.layout.top &&
          point.y <= d.layout.bottom,
      ) ?? null)
    : null;
};
export function Workspace(props: {
  render: (id: Widget, active: Accessor<boolean>) => JSX.Element;
  active: boolean;
}) {
  const lab = useLab(),
    [layout, setLayout] = createSignal<Layout>(initialLayout()),
    [notice, setNotice] = createSignal(""),
    [pointer, setPointer] = createSignal<{ x: number; y: number }>();
  let grid!: HTMLDivElement,
    key = "",
    cancelled = false;
  createEffect(() => {
    const project = lab.status()?.configuration.project;
    if (!project) return;
    const next = `ronchi.workspace.v2:${project}`;
    if (key === next) return;
    key = next;
    try {
      setLayout(restoreLayout(JSON.parse(localStorage.getItem(key) ?? "null")));
    } catch {
      setLayout(initialLayout());
    }
  });
  function save(next: Layout) {
    setLayout(next);
    if (key)
      try {
        localStorage.setItem(key, JSON.stringify(next));
      } catch {
        setNotice(
          "Layout stays available for this session. Browser storage is unavailable.",
        );
      }
  }
  function drop(from: Widget, to: Widget) {
    save(swapWidgets(layout(), from, to));
    setNotice(`${titles[from]} moved to ${titles[to]}'s position.`);
  }
  return (
    <section class="workspace" hidden={!props.active} aria-label="Workspace">
      <div class="workspace-tools">
        <span class="workspace-hint">
          Drag a panel title to move it. Drag a divider to resize.
        </span>
        <div class="actions">
          <span role="status" class="caption">
            {notice()}
          </span>
          <button
            onClick={() => {
              save(initialLayout());
              setNotice("Layout reset");
            }}
          >
            Reset layout
          </button>
        </div>
      </div>
      <DragDropProvider
        collisionDetector={pointerTarget}
        onDragStart={() => {
          cancelled = false;
        }}
        onDragEnd={({ draggable, droppable }) => {
          if (!cancelled && droppable && draggable.id !== droppable.id)
            drop(draggable.id as Widget, droppable.id as Widget);
          setPointer(undefined);
        }}
      >
        <DragDropSensors>
          <DragLifecycle
            active={() => props.active}
            cancel={() => {
              cancelled = true;
            }}
            pointer={setPointer}
          />
          <div
            class="workspace-grid"
            ref={grid}
            style={{
              "grid-template-columns": layout()
                .columns.map((n) => `${n}fr`)
                .join(" "),
              "grid-template-rows": `${layout().row}fr ${1 - layout().row}fr`,
            }}
          >
            <For each={widgets}>
              {(id) => (
                <Panel
                  id={id}
                  slot={() => layout().order.indexOf(id)}
                  active={() => props.active}
                  pointer={pointer}
                  render={props.render}
                  keyboard={(e) => {
                    const slot = layout().order.indexOf(id);
                    if (e.shiftKey) {
                      const rect = grid.getBoundingClientRect();
                      if (e.key === "ArrowUp" || e.key === "ArrowDown")
                        save(
                          resizeRows(
                            layout(),
                            e.key === "ArrowUp" ? -16 : 16,
                            rect.height,
                          ),
                        );
                      else
                        save(
                          resizeColumns(
                            layout(),
                            slot === 3 ? 1 : 0,
                            e.key === "ArrowLeft" ? -16 : 16,
                            rect.width,
                          ),
                        );
                    } else {
                      const next = adjacentSlot(slot, e.key);
                      if (next !== undefined) drop(id, layout().order[next]);
                    }
                  }}
                />
              )}
            </For>
            <Divider
              name="Left column"
              orientation="vertical"
              position={() => layout().columns[0]}
              left={() => `${layout().columns[0] * 100}%`}
              grid={() => grid}
              change={(base, delta, size) =>
                resizeColumns(base, 0, delta, size)
              }
              layout={layout}
              preview={setLayout}
              save={save}
            />
            <Divider
              name="Right column"
              orientation="vertical"
              position={() => layout().columns[0] + layout().columns[1]}
              left={() =>
                `${(layout().columns[0] + layout().columns[1]) * 100}%`
              }
              grid={() => grid}
              change={(base, delta, size) =>
                resizeColumns(base, 1, delta, size)
              }
              layout={layout}
              preview={setLayout}
              save={save}
            />
            <Divider
              name="Left panels"
              orientation="horizontal"
              position={() => layout().row}
              top={() => `${layout().row * 100}%`}
              width={() => `${layout().columns[0] * 100}%`}
              grid={() => grid}
              change={resizeRows}
              layout={layout}
              preview={setLayout}
              save={save}
            />
          </div>
          <DragOverlay class="widget-drag-overlay">
            {(draggable) => (
              <div>
                <Icon name="grip" />
                {draggable ? titles[draggable.id as Widget] : ""}
                <small>Drop on a panel to swap positions</small>
              </div>
            )}
          </DragOverlay>
        </DragDropSensors>
      </DragDropProvider>
    </section>
  );
}
function DragLifecycle(props: {
  active: Accessor<boolean>;
  cancel: () => void;
  pointer: (point: { x: number; y: number }) => void;
}) {
  const [state, actions] = useDragDropContext()!;
  const cancel = () => {
    if (state.active.draggable) {
      props.cancel();
      actions.dragEnd();
      actions.sensorEnd();
    }
  };
  const key = (e: KeyboardEvent) => {
    if (e.key === "Escape") cancel();
  };
  const move = (e: PointerEvent) => {
    if (e.buttons || state.active.draggable)
      props.pointer({ x: e.clientX, y: e.clientY });
  };
  document.addEventListener("pointermove", move, true);
  window.addEventListener("blur", cancel);
  document.addEventListener("keydown", key);
  document.addEventListener("pointercancel", cancel);
  createEffect(() => {
    if (!props.active()) cancel();
  });
  onCleanup(() => {
    cancel();
    document.removeEventListener("pointermove", move, true);
    window.removeEventListener("blur", cancel);
    document.removeEventListener("keydown", key);
    document.removeEventListener("pointercancel", cancel);
  });
  return null;
}
function Panel(props: {
  id: Widget;
  slot: Accessor<number>;
  active: Accessor<boolean>;
  pointer: Accessor<{ x: number; y: number } | undefined>;
  render: (id: Widget, active: Accessor<boolean>) => JSX.Element;
  keyboard: (e: KeyboardEvent) => void;
}) {
  const draggable = createDraggable(props.id, { pointer: props.pointer }),
    droppable = createDroppable(props.id),
    [state] = useDragDropContext()!;
  return (
    <section
      ref={(el) => {
        draggable.ref(el);
        droppable.ref(el);
      }}
      data-widget={props.id}
      class="widget"
      classList={{
        "drag-source": draggable.isActiveDraggable,
        "drop-target":
          droppable.isActiveDroppable && state.active.draggableId !== props.id,
      }}
      style={{
        "grid-column": props.slot() < 2 ? "1" : String(props.slot()),
        "grid-row": props.slot() < 2 ? String(props.slot() + 1) : "1 / span 2",
      }}
      aria-label={`${titles[props.id]} widget`}
    >
      <header
        class="widget-head"
        role="button"
        tabIndex={0}
        aria-label={`Move ${titles[props.id]}`}
        aria-describedby="workspace-drag-help"
        onPointerDown={(e) => {
          draggable.dragActivators.onpointerdown?.(e);
        }}
        onKeyDown={(e) => {
          if (e.key.startsWith("Arrow")) {
            e.preventDefault();
            props.keyboard(e);
          }
        }}
      >
        <Icon name="grip" size={14} />
        <Icon name={props.id} />
        <strong>{titles[props.id]}</strong>
        <span class="panel-drag-hint">Drag to move</span>
      </header>
      <div class="widget-body">{props.render(props.id, props.active)}</div>
      <span
        id={props.id === "cases" ? "workspace-drag-help" : undefined}
        class="sr-only"
      >
        Drag the title onto another panel. Arrow keys move panels. Shift and
        arrow keys resize.
      </span>
    </section>
  );
}
