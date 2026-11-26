import { createSignal, For, type JSX, onCleanup, Show } from "solid-js";
import { Portal } from "solid-js/web";

export interface MenuItem {
  label: string;
  icon?: JSX.Element;
  onSelect: () => void;
  danger?: boolean;
  disabled?: boolean;
  separatorBefore?: boolean;
}

interface Props {
  items: () => MenuItem[];
  trigger: (props: { onClick: (e: MouseEvent) => void; "aria-expanded": boolean; "aria-haspopup": "menu" }) => JSX.Element;
  align?: "start" | "end";
}

/** A list of actions anchored to a trigger. Arrow keys move, Enter selects, Escape closes. */
export function Menu(props: Props) {
  const [open, setOpen] = createSignal(false);
  const [position, setPosition] = createSignal({ top: 0, left: 0, right: 0 });
  let list: HTMLDivElement | undefined;
  let trigger: HTMLElement | undefined;

  const close = (refocus = true) => {
    setOpen(false);
    if (refocus) trigger?.focus();
  };

  const onDocPointer = (e: PointerEvent) => {
    if (list && !list.contains(e.target as Node) && !trigger?.contains(e.target as Node)) close(false);
  };
  const onBlur = () => setOpen(false);
  document.addEventListener("pointerdown", onDocPointer);
  window.addEventListener("blur", onBlur);
  onCleanup(() => {
    document.removeEventListener("pointerdown", onDocPointer);
    window.removeEventListener("blur", onBlur);
  });

  const toggle = (e: MouseEvent) => {
    trigger = e.currentTarget as HTMLElement;
    const rect = trigger.getBoundingClientRect();
    setPosition({ top: rect.bottom + 6, left: rect.left, right: window.innerWidth - rect.right });
    setOpen((v) => !v);
    queueMicrotask(() => list?.querySelector<HTMLElement>("[role=menuitem]:not([disabled])")?.focus());
  };

  const onKey = (e: KeyboardEvent) => {
    const items = [...(list?.querySelectorAll<HTMLElement>("[role=menuitem]:not([disabled])") ?? [])];
    const index = items.indexOf(document.activeElement as HTMLElement);
    if (e.key === "Escape") {
      e.stopPropagation();
      close();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      items[(index + 1) % items.length]?.focus();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      items[(index - 1 + items.length) % items.length]?.focus();
    } else if (e.key === "Tab") {
      close(false);
    }
  };

  return (
    <>
      {props.trigger({ onClick: toggle, "aria-expanded": open(), "aria-haspopup": "menu" })}
      <Show when={open()}>
        <Portal>
          <div
            ref={list}
            class="menu"
            role="menu"
            onKeyDown={onKey}
            style={
              props.align === "start"
                ? { top: `${position().top}px`, left: `${position().left}px` }
                : { top: `${position().top}px`, right: `${position().right}px` }
            }
          >
            <For each={props.items()}>
              {(item) => (
                <>
                  <Show when={item.separatorBefore}>
                    <div class="menu-separator" role="separator" />
                  </Show>
                  <button
                    type="button"
                    role="menuitem"
                    class="menu-item"
                    data-danger={item.danger || undefined}
                    disabled={item.disabled}
                    onClick={() => {
                      close(false);
                      item.onSelect();
                    }}
                  >
                    {item.icon}
                    <span>{item.label}</span>
                  </button>
                </>
              )}
            </For>
          </div>
        </Portal>
      </Show>
    </>
  );
}
